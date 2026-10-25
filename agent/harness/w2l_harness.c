// w2l_harness: native test harness for modules produced by wasm2llvm.
//
// Loads translated modules (compiled to shared objects) and executes
// line-based commands read from stdin. See PROTOCOL.md and ../TASK.md (ABI).
//
// Build:  clang -O2 -g -o w2l_harness w2l_harness.c -ldl -lpthread -rdynamic
//
// The harness provides to translated code:
//   void w2l_trap(int32_t code)                       (noreturn)
//   void w2l_import_<hex(module)>_<hex(name)>(void *args, void *results)
//        for the host functions listed in host_funcs below.

#define _GNU_SOURCE
#include <dlfcn.h>
#include <inttypes.h>
#include <pthread.h>
#include <setjmp.h>
#include <stdarg.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#define STACK_SIZE (8u << 20)  // usable stack for wasm execution
#define GUARD_SIZE (64u << 20) // PROT_NONE region below the stack
#define MAX_SLOTS 4096
#define MAX_MODULES 1024
#define TRAP_EXHAUSTION 8

typedef void (*tramp_fn)(void *args, void *results);
typedef void (*init_fn)(void);
typedef void (*global_fn)(void *out);
typedef uint8_t *(*memory_fn)(uint64_t *size_out);

static sigjmp_buf trap_jmp;
static volatile sig_atomic_t in_wasm = 0;
static uint8_t *guard_lo, *guard_hi;

static struct {
  char *id;
  void *handle;
} modules[MAX_MODULES];
static int nmodules;

static uint8_t *host_out;
static size_t host_out_len, host_out_cap;

static _Alignas(16) uint8_t arg_slots[MAX_SLOTS * 16];
static _Alignas(16) uint8_t res_slots[MAX_SLOTS * 16];

// ---------------------------------------------------------------------------
// Runtime services for translated code

__attribute__((noreturn, visibility("default"))) void w2l_trap(int32_t code) {
  if (!in_wasm) {
    fprintf(stderr, "w2l_harness: w2l_trap(%d) called outside of wasm execution\n", code);
    abort();
  }
  siglongjmp(trap_jmp, code + 1);
}

static void out_append(const void *p, size_t n) {
  if (host_out_len + n > host_out_cap) {
    host_out_cap = (host_out_len + n) * 2 + 64;
    host_out = realloc(host_out, host_out_cap);
  }
  memcpy(host_out + host_out_len, p, n);
  host_out_len += n;
}

static void out_printf(const char *fmt, ...) {
  char buf[128];
  va_list ap;
  va_start(ap, fmt);
  int n = vsnprintf(buf, sizeof buf, fmt, ap);
  va_end(ap);
  out_append(buf, (size_t)n);
}

static void out_i32(const uint8_t *s) { int32_t v; memcpy(&v, s, 4); out_printf("i32:%" PRId32 "\n", v); }
static void out_i64(const uint8_t *s) { int64_t v; memcpy(&v, s, 8); out_printf("i64:%" PRId64 "\n", v); }
static void out_f32(const uint8_t *s) { uint32_t v; memcpy(&v, s, 4); out_printf("f32:0x%08" PRIx32 "\n", v); }
static void out_f64(const uint8_t *s) { uint64_t v; memcpy(&v, s, 8); out_printf("f64:0x%016" PRIx64 "\n", v); }
static void out_v128(const uint8_t *s) {
  out_printf("v128:");
  for (int i = 0; i < 16; i++) out_printf("%02x", s[i]);
  out_printf("\n");
}

#define HOST(sym) __attribute__((visibility("default"))) void sym(void *args, void *results)
#define A(i) ((const uint8_t *)args + 16 * (i))

// spectest.*
HOST(w2l_import_7370656374657374_7072696e74) { (void)args; (void)results; out_printf("print\n"); }
HOST(w2l_import_7370656374657374_7072696e745f693332) { (void)results; out_i32(A(0)); }
HOST(w2l_import_7370656374657374_7072696e745f693634) { (void)results; out_i64(A(0)); }
HOST(w2l_import_7370656374657374_7072696e745f663332) { (void)results; out_f32(A(0)); }
HOST(w2l_import_7370656374657374_7072696e745f663634) { (void)results; out_f64(A(0)); }
HOST(w2l_import_7370656374657374_7072696e745f6933325f663332) { (void)results; out_i32(A(0)); out_f32(A(1)); }
HOST(w2l_import_7370656374657374_7072696e745f6636345f663634) { (void)results; out_f64(A(0)); out_f64(A(1)); }
// env.*
HOST(w2l_import_656e76_7072696e745f693332) { (void)results; out_i32(A(0)); }
HOST(w2l_import_656e76_7072696e745f693634) { (void)results; out_i64(A(0)); }
HOST(w2l_import_656e76_7072696e745f663332) { (void)results; out_f32(A(0)); }
HOST(w2l_import_656e76_7072696e745f663634) { (void)results; out_f64(A(0)); }
HOST(w2l_import_656e76_7072696e745f76313238) { (void)results; out_v128(A(0)); }
HOST(w2l_import_656e76_70757463686172) { (void)results; out_append(A(0), 1); }

// ---------------------------------------------------------------------------
// Signal handling: stack overflow inside wasm code becomes trap 8.

static void segv_handler(int sig, siginfo_t *si, void *uc) {
  (void)uc;
  uint8_t *a = (uint8_t *)si->si_addr;
  if (in_wasm && a >= guard_lo && a < guard_hi) siglongjmp(trap_jmp, TRAP_EXHAUSTION + 1);
  // Genuine crash: restore default action and re-raise.
  signal(sig, SIG_DFL);
  raise(sig);
}

static void install_signals(void) {
  static uint8_t altstack[1 << 16];
  stack_t ss = {.ss_sp = altstack, .ss_size = sizeof altstack, .ss_flags = 0};
  sigaltstack(&ss, NULL);
  struct sigaction sa;
  memset(&sa, 0, sizeof sa);
  sa.sa_sigaction = segv_handler;
  sa.sa_flags = SA_SIGINFO | SA_ONSTACK | SA_NODEFER;
  sigemptyset(&sa.sa_mask);
  sigaction(SIGSEGV, &sa, NULL);
  sigaction(SIGBUS, &sa, NULL);
}

// ---------------------------------------------------------------------------
// Command processing

static int hexval(char c) {
  if (c >= '0' && c <= '9') return c - '0';
  if (c >= 'a' && c <= 'f') return c - 'a' + 10;
  if (c >= 'A' && c <= 'F') return c - 'A' + 10;
  return -1;
}

static int parse_slot(const char *tok, uint8_t *slot) {
  if (strlen(tok) != 32) return -1;
  for (int i = 0; i < 16; i++) {
    int h = hexval(tok[2 * i]), l = hexval(tok[2 * i + 1]);
    if (h < 0 || l < 0) return -1;
    slot[i] = (uint8_t)(h * 16 + l);
  }
  return 0;
}

static void print_slot(const uint8_t *s) {
  for (int i = 0; i < 16; i++) printf("%02x", s[i]);
}

static void *find_module(const char *id) {
  for (int i = 0; i < nmodules; i++)
    if (strcmp(modules[i].id, id) == 0) return modules[i].handle;
  return NULL;
}

static void *find_sym(const char *id, const char *prefix, const char *hexname) {
  void *h = find_module(id);
  if (!h) return NULL;
  char sym[8192];
  snprintf(sym, sizeof sym, "%s%s", prefix, strcmp(hexname, "-") == 0 ? "" : hexname);
  return dlsym(h, sym);
}

// Runs fn(a, b) with trap protection. Returns -1 on success, trap code otherwise.
static int protected_call(void (*fn)(void *, void *), void *a, void *b) {
  int r = sigsetjmp(trap_jmp, 1);
  if (r == 0) {
    in_wasm = 1;
    fn(a, b);
    in_wasm = 0;
    return -1;
  }
  in_wasm = 0;
  return r - 1;
}

static void call_init(void *f, void *unused) { (void)unused; ((init_fn)f)(); }
static void call_tramp(void *f, void *nargs_unused) { (void)nargs_unused; ((tramp_fn)f)(arg_slots, res_slots); }

#define MAXTOK (MAX_SLOTS + 8)

static void handle(char **tok, int ntok) {
  const char *cmd = tok[0];
  if (strcmp(cmd, "ping") == 0) {
    printf("ok\n");
  } else if (strcmp(cmd, "load") == 0 && ntok == 3) {
    if (nmodules >= MAX_MODULES) { printf("error too many modules\n"); return; }
    void *h = dlopen(tok[2], RTLD_NOW | RTLD_LOCAL);
    if (!h) {
      char *e = dlerror();
      for (char *p = e; p && *p; p++) if (*p == '\n') *p = ' ';
      printf("error dlopen: %s\n", e ? e : "?");
      return;
    }
    modules[nmodules].id = strdup(tok[1]);
    modules[nmodules].handle = h;
    nmodules++;
    printf("ok\n");
  } else if (strcmp(cmd, "init") == 0 && ntok == 2) {
    void *h = find_module(tok[1]);
    void *f = h ? dlsym(h, "w2l_init") : NULL;
    if (!f) { printf("error no w2l_init\n"); return; }
    int t = protected_call(call_init, f, NULL);
    if (t < 0) printf("ok\n"); else printf("trap %d\n", t);
  } else if (strcmp(cmd, "invoke") == 0 && ntok >= 4) {
    void *f = find_sym(tok[1], "w2l_export_", tok[2]);
    if (!f) { printf("error no such export\n"); return; }
    int nres = atoi(tok[3]);
    int nargs = ntok - 4;
    if (nres < 0 || nres > MAX_SLOTS || nargs > MAX_SLOTS) { printf("error too many values\n"); return; }
    memset(arg_slots, 0, sizeof arg_slots);
    memset(res_slots, 0, sizeof res_slots);
    for (int i = 0; i < nargs; i++)
      if (parse_slot(tok[4 + i], arg_slots + 16 * i)) { printf("error bad slot\n"); return; }
    int t = protected_call(call_tramp, f, NULL);
    if (t >= 0) { printf("trap %d\n", t); return; }
    printf("ok");
    for (int i = 0; i < nres; i++) { putchar(' '); print_slot(res_slots + 16 * i); }
    printf("\n");
  } else if (strcmp(cmd, "global") == 0 && ntok == 3) {
    void *f = find_sym(tok[1], "w2l_global_", tok[2]);
    if (!f) { printf("error no such export\n"); return; }
    _Alignas(16) uint8_t slot[16] = {0};
    ((global_fn)f)(slot);
    printf("ok ");
    print_slot(slot);
    printf("\n");
  } else if (strcmp(cmd, "memhash") == 0 && ntok == 3) {
    void *f = find_sym(tok[1], "w2l_memory_", tok[2]);
    if (!f) { printf("error no such export\n"); return; }
    uint64_t size = 0;
    uint8_t *base = ((memory_fn)f)(&size);
    uint64_t h = 0xcbf29ce484222325ull;
    for (uint64_t i = 0; i < size; i++) { h ^= base[i]; h *= 0x100000001b3ull; }
    printf("ok %" PRIu64 " %016" PRIx64 "\n", size, h);
  } else if (strcmp(cmd, "output") == 0 && ntok == 2) {
    printf("ok ");
    if (host_out_len == 0) printf("-");
    for (size_t i = 0; i < host_out_len; i++) printf("%02x", host_out[i]);
    printf("\n");
    host_out_len = 0;
  } else {
    printf("error bad command\n");
  }
}

static void *worker(void *unused) {
  (void)unused;
  install_signals();
  char *line = NULL;
  size_t cap = 0;
  static char *tok[MAXTOK];
  while (getline(&line, &cap, stdin) > 0) {
    int ntok = 0;
    for (char *p = strtok(line, " \t\r\n"); p && ntok < MAXTOK; p = strtok(NULL, " \t\r\n")) tok[ntok++] = p;
    if (ntok == 0) continue;
    if (strcmp(tok[0], "quit") == 0) break;
    handle(tok, ntok);
    fflush(stdout);
  }
  return NULL;
}

int main(void) {
  // Run everything on a thread whose stack has a large guard region beneath it,
  // so that runaway recursion in translated code can be reported as trap 8.
  size_t total = (size_t)GUARD_SIZE + STACK_SIZE;
  uint8_t *mem = mmap(NULL, total, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0);
  if (mem == MAP_FAILED) { perror("mmap"); return 1; }
  mprotect(mem, GUARD_SIZE, PROT_NONE);
  guard_lo = mem;
  guard_hi = mem + GUARD_SIZE;
  pthread_attr_t attr;
  pthread_attr_init(&attr);
  pthread_attr_setstack(&attr, mem + GUARD_SIZE, STACK_SIZE);
  pthread_t th;
  if (pthread_create(&th, &attr, worker, NULL)) { fprintf(stderr, "pthread_create failed\n"); return 1; }
  pthread_join(th, NULL);
  return 0;
}
