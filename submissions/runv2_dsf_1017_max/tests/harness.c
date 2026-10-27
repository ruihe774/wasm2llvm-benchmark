// Test harness implementing the w2l ABI and the w2l-ref line protocol.
// Not part of the translator; used for local differential testing.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <inttypes.h>
#include <setjmp.h>
#include <stdarg.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <link.h>

typedef void (*init_fn)(void);
typedef void (*export_fn)(void *, void *);
typedef void (*global_fn)(void *);
typedef void *(*memory_fn)(void *);

static sigjmp_buf trap_env;
static void *g_cur_handle = NULL;
static volatile int trap_armed = 0;
static volatile int trap_code = 0;
static void *cur_handle = NULL;

void w2l_trap(int code) {
    if (!trap_armed) {
        fprintf(stderr, "w2l_trap(%d) outside a protected call\n", code);
        _exit(97);
    }
    trap_code = code;
    {
        void *ra = __builtin_return_address(0);
        unsigned long off = 0;
        struct link_map *lm = NULL;
        if (cur_handle && dlinfo(cur_handle, RTLD_DI_LINKMAP, &lm) == 0 && lm)
            off = (unsigned long)((char *)ra - (char *)lm->l_addr);
        fprintf(stderr, "TRAP %d at so+0x%lx caller=so+0x%lx\n", code, off,
                (unsigned long)((char *)__builtin_return_address(1) - (char *)lm->l_addr));
    }
    siglongjmp(trap_env, 1);
}

// ------------------------- host output -------------------------
static char *outbuf = NULL;
static size_t outlen = 0, outcap = 0;

static void out_reset(void) {
    outlen = 0;
    if (outbuf) outbuf[0] = 0;
}

static void out_append(const char *s, size_t n) {
    if (outlen + n + 1 > outcap) {
        size_t nc = (outcap ? outcap * 2 : 256);
        while (nc < outlen + n + 1) nc *= 2;
        outbuf = realloc(outbuf, nc);
        outcap = nc;
    }
    memcpy(outbuf + outlen, s, n);
    outlen += n;
    outbuf[outlen] = 0;
}

static void out_printf(const char *fmt, ...) {
    char tmp[256];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(tmp, sizeof tmp, fmt, ap);
    va_end(ap);
    if (n > 0) out_append(tmp, (size_t)n);
}

static uint32_t slot_u32(const uint8_t *p) {
    uint32_t v;
    memcpy(&v, p, 4);
    return v;
}
static uint64_t slot_u64(const uint8_t *p) {
    uint64_t v;
    memcpy(&v, p, 8);
    return v;
}

void w2l_dbg9(int site, int a, int b, int c);
void w2l_dbg9(int site, int a, int b, int c) {
    fprintf(stderr, "DBG9 site=%d args=%d,%d,%d\n", site, a, b, c);
    w2l_trap(0);
}

#define IMPORT(name) void name(void *args, void *results)
IMPORT(w2l_import_7370656374657374_7072696e74) {
    (void)args; (void)results;
    out_printf("print\n");
}
IMPORT(w2l_import_7370656374657374_7072696e745f693332) {
    (void)results;
    out_printf("i32:%d\n", (int32_t)slot_u32((uint8_t *)args + 0));
}
IMPORT(w2l_import_7370656374657374_7072696e745f693634) {
    (void)results;
    out_printf("i64:%lld\n", (long long)(int64_t)slot_u64((uint8_t *)args + 0));
}
IMPORT(w2l_import_7370656374657374_7072696e745f663332) {
    (void)results;
    out_printf("f32:0x%08x\n", slot_u32((uint8_t *)args + 0));
}
IMPORT(w2l_import_7370656374657374_7072696e745f663634) {
    (void)results;
    out_printf("f64:0x%016llx\n", (unsigned long long)slot_u64((uint8_t *)args + 0));
}
IMPORT(w2l_import_7370656374657374_7072696e745f6933325f663332) {
    (void)results;
    out_printf("i32:%d\n", (int32_t)slot_u32((uint8_t *)args + 0));
    out_printf("f32:0x%08x\n", slot_u32((uint8_t *)args + 16));
}
IMPORT(w2l_import_7370656374657374_7072696e745f6636345f663634) {
    (void)results;
    out_printf("f64:0x%016llx\n", (unsigned long long)slot_u64((uint8_t *)args + 0));
    out_printf("f64:0x%016llx\n", (unsigned long long)slot_u64((uint8_t *)args + 16));
}
IMPORT(w2l_import_656e76_7072696e745f693332) {
    (void)results;
    out_printf("i32:%d\n", (int32_t)slot_u32((uint8_t *)args + 0));
}
IMPORT(w2l_import_656e76_7072696e745f693634) {
    (void)results;
    out_printf("i64:%lld\n", (long long)(int64_t)slot_u64((uint8_t *)args + 0));
}
IMPORT(w2l_import_656e76_7072696e745f663332) {
    (void)results;
    out_printf("f32:0x%08x\n", slot_u32((uint8_t *)args + 0));
}
IMPORT(w2l_import_656e76_7072696e745f663634) {
    (void)results;
    out_printf("f64:0x%016llx\n", (unsigned long long)slot_u64((uint8_t *)args + 0));
}
IMPORT(w2l_import_656e76_7072696e745f76313238) {
    (void)results;
    unsigned char *p = (unsigned char *)args;
    char hex[33];
    for (int i = 0; i < 16; i++) sprintf(hex + 2 * i, "%02x", p[i]);
    out_printf("v128:%s\n", hex);
}
IMPORT(w2l_import_656e76_70757463686172) {
    (void)results;
    unsigned char c = (unsigned char)slot_u32((uint8_t *)args + 0);
    out_append((char *)&c, 1);
}

// ------------------------- signal handling -------------------------
static void segv_handler(int sig, siginfo_t *info, void *ctx) {
    (void)ctx;
    if (!trap_armed) {
        // Not inside a protected call: real crash.
        const char *msg = "harness: unhandled fault\n";
        write(2, msg, strlen(msg));
        _exit(99);
    }
    // Native stack overflow (the module's guard region): trap 8.
    trap_code = 8;
    siglongjmp(trap_env, 1);
}

static void install_signals(void) {
    static char altstack[1 << 16];
    stack_t ss;
    ss.ss_sp = altstack;
    ss.ss_size = sizeof altstack;
    ss.ss_flags = 0;
    sigaltstack(&ss, NULL);
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = segv_handler;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGSEGV, &sa, NULL);
    sigaction(SIGBUS, &sa, NULL);
}

// ------------------------- module state -------------------------
#define MAX_MODULES 8
typedef struct {
    void *handle;
    int used;
} ModEntry;
static ModEntry modules[MAX_MODULES];

static int hex2bytes(const char *hex, uint8_t *out, size_t maxn, size_t *outn) {
    size_t n = strlen(hex);
    if (n % 2) return -1;
    if (n / 2 > maxn) return -1;
    for (size_t i = 0; i < n / 2; i++) {
        unsigned v;
        if (sscanf(hex + 2 * i, "%2x", &v) != 1) return -1;
        out[i] = (uint8_t)v;
    }
    *outn = n / 2;
    return 0;
}

static int resolve(int id, const char *hexname, const char *prefix, void **out) {
    if (id < 0 || id >= MAX_MODULES || !modules[id].used) return -1;
    char sym[512];
    const char *nm = strcmp(hexname, "-") == 0 ? "" : hexname;
    snprintf(sym, sizeof sym, "%s%s", prefix, nm);
    void *p = dlsym(modules[id].handle, sym);
    if (!p) return -1;
    *out = p;
    return 0;
}

static int protected_call(void (*fn)(void *, void *), void *args, void *results) {
    cur_handle = g_cur_handle;
    trap_armed = 1;
    if (sigsetjmp(trap_env, 1) == 0) {
        fn(args, results);
        trap_armed = 0;
        return -1;
    } else {
        trap_armed = 0;
        return trap_code;
    }
}

static int protected_init(init_fn fn) {
    cur_handle = g_cur_handle;
    trap_armed = 1;
    if (sigsetjmp(trap_env, 1) == 0) {
        fn();
        trap_armed = 0;
        return -1;
    } else {
        trap_armed = 0;
        return trap_code;
    }
}

static uint64_t fnv1a64(const uint8_t *p, size_t n) {
    uint64_t h = 14695981039346656037ULL;
    for (size_t i = 0; i < n; i++) {
        h ^= p[i];
        h *= 1099511628211ULL;
    }
    return h;
}

static void print_slot(const uint8_t *p) {
    for (int i = 0; i < 16; i++) printf("%02x", p[i]);
}

static void *module_mem(int id, const char *hexname, uint64_t *size) {
    void *fp = NULL;
    if (resolve(id, hexname, "w2l_memory_", &fp)) return NULL;
    memory_fn f = (memory_fn)fp;
    uint64_t sz = 0;
    void *base = f(&sz);
    if (size) *size = sz;
    return base;
}

int main(void) {
    install_signals();
    setvbuf(stdout, NULL, _IOFBF, 1 << 16);
    char line[1 << 20];
    while (fgets(line, sizeof line, stdin)) {
        size_t L = strlen(line);
        while (L && (line[L - 1] == '\n' || line[L - 1] == '\r')) line[--L] = 0;
        if (L == 0) continue;
        char *save = NULL;
        char *cmd = strtok_r(line, " ", &save);
        if (!cmd) continue;
        if (strcmp(cmd, "quit") == 0) {
            return 0;
        } else if (strcmp(cmd, "ping") == 0) {
            printf("ok\n");
        } else if (strcmp(cmd, "load") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            char *path = strtok_r(NULL, " ", &save);
            if (!ids || !path) { printf("error bad load\n"); continue; }
            int id = atoi(ids);
            if (id < 0 || id >= MAX_MODULES) { printf("error bad id\n"); continue; }
            if (modules[id].used) { dlclose(modules[id].handle); modules[id].used = 0; }
            void *h = dlopen(path, RTLD_NOW | RTLD_LOCAL);
            if (!h) {
                printf("error %s\n", dlerror());
                continue;
            }
            modules[id].handle = h;
            modules[id].used = 1;
            printf("ok\n");
        } else if (strcmp(cmd, "init") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            if (!ids) { printf("error bad init\n"); continue; }
            int id = atoi(ids);
            void *p = NULL;
            if (resolve(id, "", "w2l_init", &p)) { printf("error no w2l_init\n"); continue; }
            g_cur_handle = modules[id].handle;
            int r = protected_init((init_fn)p);
            if (r >= 0) printf("trap %d\n", r); else printf("ok\n");
        } else if (strcmp(cmd, "invoke") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            char *hexname = strtok_r(NULL, " ", &save);
            char *nrs = strtok_r(NULL, " ", &save);
            if (!ids || !hexname || !nrs) { printf("error bad invoke\n"); continue; }
            int id = atoi(ids);
            int nres = atoi(nrs);
            uint8_t args[64 * 16];
            uint8_t results[64 * 16];
            memset(args, 0, sizeof args);
            memset(results, 0, sizeof results);
            int nargs = 0;
            char *slot;
            while ((slot = strtok_r(NULL, " ", &save))) {
                size_t n;
                if (nargs >= 64 || hex2bytes(slot, args + 16 * nargs, 16, &n) || n != 16) {
                    printf("error bad slot\n");
                    nargs = -1;
                    break;
                }
                nargs++;
            }
            if (nargs < 0) continue;
            void *p = NULL;
            if (resolve(id, hexname, "w2l_export_", &p)) { printf("error no export\n"); continue; }
            g_cur_handle = modules[id].handle;
            int r = protected_call((export_fn)p, args, results);
            if (r >= 0) {
                printf("trap %d\n", r);
            } else {
                printf("ok");
                for (int i = 0; i < nres; i++) {
                    printf(" ");
                    print_slot(results + 16 * i);
                }
                printf("\n");
            }
        } else if (strcmp(cmd, "global") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            char *hexname = strtok_r(NULL, " ", &save);
            if (!ids || !hexname) { printf("error bad global\n"); continue; }
            int id = atoi(ids);
            void *p = NULL;
            if (resolve(id, hexname, "w2l_global_", &p)) { printf("error no global\n"); continue; }
            uint8_t out[16];
            memset(out, 0, sizeof out);
            ((global_fn)p)(out);
            printf("ok ");
            print_slot(out);
            printf("\n");
        } else if (strcmp(cmd, "memhash") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            char *hexname = strtok_r(NULL, " ", &save);
            if (!ids || !hexname) { printf("error bad memhash\n"); continue; }
            int id = atoi(ids);
            uint64_t sz = 0;
            void *base = module_mem(id, hexname, &sz);
            if (!base && sz) { printf("error no memory\n"); continue; }
            uint64_t h = fnv1a64((const uint8_t *)base, (size_t)sz);
            printf("ok %" PRIu64 " %016" PRIx64 "\n", sz, h);
        } else if (strcmp(cmd, "memwrite") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            char *hexname = strtok_r(NULL, " ", &save);
            char *offs = strtok_r(NULL, " ", &save);
            char *hexbytes = strtok_r(NULL, " ", &save);
            if (!ids || !hexname || !offs) { printf("error bad memwrite\n"); continue; }
            int id = atoi(ids);
            uint64_t off = strtoull(offs, NULL, 10);
            uint64_t sz = 0;
            void *base = module_mem(id, hexname, &sz);
            uint8_t data[1 << 20];
            size_t n = 0;
            if (hexbytes && strcmp(hexbytes, "-") != 0 && hex2bytes(hexbytes, data, sizeof data, &n)) {
                printf("error bad hex\n");
                continue;
            }
            if (!base || off + n > sz) { printf("error oob\n"); continue; }
            memcpy((uint8_t *)base + off, data, n);
            printf("ok\n");
        } else if (strcmp(cmd, "memread") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            char *hexname = strtok_r(NULL, " ", &save);
            char *offs = strtok_r(NULL, " ", &save);
            char *lens = strtok_r(NULL, " ", &save);
            if (!ids || !hexname || !offs || !lens) { printf("error bad memread\n"); continue; }
            int id = atoi(ids);
            uint64_t off = strtoull(offs, NULL, 10);
            uint64_t len = strtoull(lens, NULL, 10);
            uint64_t sz = 0;
            void *base = module_mem(id, hexname, &sz);
            if (!base || off + len > sz) { printf("error oob\n"); continue; }
            printf("ok ");
            for (uint64_t i = 0; i < len; i++) printf("%02x", ((uint8_t *)base)[off + i]);
            printf("\n");
        } else if (strcmp(cmd, "memwrite_global") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            char *ghex = strtok_r(NULL, " ", &save);
            char *mhex = strtok_r(NULL, " ", &save);
            char *hexbytes = strtok_r(NULL, " ", &save);
            if (!ids || !ghex || !mhex) { printf("error bad memwrite_global\n"); continue; }
            int id = atoi(ids);
            void *p = NULL;
            if (resolve(id, ghex, "w2l_global_", &p)) { printf("error no global\n"); continue; }
            uint8_t out[16];
            memset(out, 0, sizeof out);
            ((global_fn)p)(out);
            uint64_t off = slot_u32(out);
            uint64_t sz = 0;
            void *base = module_mem(id, mhex, &sz);
            uint8_t data[1 << 20];
            size_t n = 0;
            if (hexbytes && strcmp(hexbytes, "-") != 0 && hex2bytes(hexbytes, data, sizeof data, &n)) {
                printf("error bad hex\n");
                continue;
            }
            if (!base || off + n > sz) { printf("error oob\n"); continue; }
            memcpy((uint8_t *)base + off, data, n);
            printf("ok\n");
        } else if (strcmp(cmd, "output") == 0) {
            char *ids = strtok_r(NULL, " ", &save);
            (void)ids;
            if (outlen == 0) {
                printf("ok -\n");
            } else {
                printf("ok ");
                for (size_t i = 0; i < outlen; i++) printf("%02x", (unsigned char)outbuf[i]);
                printf("\n");
            }
            out_reset();
        } else {
            printf("error unknown command\n");
        }
        fflush(stdout);
    }
    return 0;
}
