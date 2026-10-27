/* Test harness implementing the w2l ABI (ABI.md) for generated .so modules.
 * Reads a line protocol on stdin, like w2l-ref.
 * Build: clang -O1 -rdynamic -o harness harness.c -ldl
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
#include <unistd.h>

static sigjmp_buf jb;
static int trap_code;

static void segv_handler(int sig, siginfo_t *si, void *ctx) {
    (void)ctx;
    if (getenv("W2L_DEBUG_SIG")) {
        char buf[128];
        int n = snprintf(buf, sizeof buf, "HARNESS-SIG %d addr=%p\n", sig, si ? si->si_addr : 0);
        write(2, buf, n);
    }
    trap_code = 8;
    siglongjmp(jb, 2);
}

static void setup_signals(void) {
    static char altstack[256 * 1024];
    stack_t ss;
    memset(&ss, 0, sizeof ss);
    ss.ss_sp = altstack;
    ss.ss_size = sizeof altstack;
    ss.ss_flags = 0;
    sigaltstack(&ss, NULL);
    struct sigaction sa;
    memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = segv_handler;
    sa.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigaction(SIGSEGV, &sa, NULL);
    sigaction(SIGBUS, &sa, NULL);
    sigaction(SIGILL, &sa, NULL);
    sigaction(SIGFPE, &sa, NULL);
}

__attribute__((noreturn)) void w2l_trap(int32_t code) {
    trap_code = code;
    siglongjmp(jb, 1);
}

/* ---------------- host imports ---------------- */

static char *g_out;
static size_t g_outlen, g_outcap;

static void out_reset(void) { g_outlen = 0; }
static void out_byte(int b) {
    if (g_outlen + 1 > g_outcap) {
        g_outcap = g_outcap ? g_outcap * 2 : 256;
        g_out = realloc(g_out, g_outcap);
    }
    g_out[g_outlen++] = (char)b;
}
static void out_str(const char *s) { while (*s) out_byte(*s++); }
static void out_i64(int64_t v) { char b[32]; snprintf(b, sizeof b, "%lld", (long long)v); out_str(b); }
static void out_f32(uint32_t bits) { char b[32]; snprintf(b, sizeof b, "f32:0x%08x\n", bits); out_str(b); }
static void out_f64(uint64_t bits) { char b[64]; snprintf(b, sizeof b, "f64:0x%016llx\n", (unsigned long long)bits); out_str(b); }

#define SLOT(p, i) ((uint8_t *)(p) + 16 * (size_t)(i))

/* spectest / env imports: name hex encodings */
#define IMP(n) void w2l_import_##n(uint8_t *args, uint8_t *results)
IMP(7370656374657374_7072696e74) { (void)args; (void)results; out_str("print\n"); }
IMP(7370656374657374_7072696e745f693332) { int32_t v; memcpy(&v, SLOT(args,0), 4); out_str("i32:"); out_i64(v); out_byte('\n'); }
IMP(7370656374657374_7072696e745f693634) { int64_t v; memcpy(&v, SLOT(args,0), 8); out_str("i64:"); out_i64(v); out_byte('\n'); }
IMP(7370656374657374_7072696e745f663332) { uint32_t v; memcpy(&v, SLOT(args,0), 4); out_f32(v); }
IMP(7370656374657374_7072696e745f663634) { uint64_t v; memcpy(&v, SLOT(args,0), 8); out_f64(v); }
IMP(7370656374657374_7072696e745f6933325f663332) { int32_t a; uint32_t b; memcpy(&a, SLOT(args,0),4); memcpy(&b, SLOT(args,1),4); out_str("i32:"); out_i64(a); out_byte('\n'); out_f32(b); }
IMP(7370656374657374_7072696e745f6636345f663634) { uint64_t a,b; memcpy(&a, SLOT(args,0),8); memcpy(&b, SLOT(args,1),8); out_f64(a); out_f64(b); }
IMP(656e76_7072696e745f693332) { int32_t v; memcpy(&v, SLOT(args,0), 4); out_str("i32:"); out_i64(v); out_byte('\n'); }
IMP(656e76_7072696e745f693634) { int64_t v; memcpy(&v, SLOT(args,0), 8); out_str("i64:"); out_i64(v); out_byte('\n'); }
IMP(656e76_7072696e745f663332) { uint32_t v; memcpy(&v, SLOT(args,0), 4); out_f32(v); }
IMP(656e76_7072696e745f663634) { uint64_t v; memcpy(&v, SLOT(args,0), 8); out_f64(v); }
IMP(656e76_7072696e745f76313238) { out_str("v128:"); for (int i = 15; i >= 0; i--) { char b[4]; snprintf(b, sizeof b, "%02x", SLOT(args,0)[i]); out_str(b); } out_byte('\n'); }
IMP(656e76_70757463686172) { int32_t v; memcpy(&v, SLOT(args,0), 4); out_byte((int)v); }

IMP(656e76_6563686f) {
    /* (i32 i64 f32 f64 v128) -> (i32 i64 f32 f64 v128) */
    for (int i = 0; i < 5; i++) memcpy(SLOT(results, i), SLOT(args, i), 16);
}
IMP(656e76_73756d) {
    int32_t a, b;
    memcpy(&a, SLOT(args,0), 4);
    memcpy(&b, SLOT(args,1), 4);
    int32_t r = (int32_t)((uint32_t)a + (uint32_t)b);
    memcpy(SLOT(results,0), &r, 4);
}
IMP(656e76_76616464) {
    for (int i = 0; i < 16; i++) SLOT(results,0)[i] = SLOT(args,0)[i] ^ SLOT(args,1)[i];
}
IMP(656e76_7265745f6d756c7469) {
    int32_t a = 7; int64_t b = 8;
    memcpy(SLOT(results,0), &a, 4);
    memcpy(SLOT(results,1), &b, 8);
}

/* ---------------- module registry ---------------- */

typedef struct {
    int used;
    void *handle;
    void (*init)(void);
} Mod;
static Mod mods[128];

static Mod *getmod(const char *ids) {
    int id = atoi(ids);
    if (id < 0 || id >= 128) return NULL;
    return &mods[id];
}

static void *sym(Mod *m, const char *name) {
    return dlsym(m->handle, name);
}

static int hexval(int c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static int parse_hex(const char *s, uint8_t *out, size_t maxlen, size_t *outlen) {
    size_t n = 0;
    while (s[0] && s[1]) {
        int hi = hexval(s[0]), lo = hexval(s[1]);
        if (hi < 0 || lo < 0) return -1;
        if (n >= maxlen) return -1;
        out[n++] = (uint8_t)((hi << 4) | lo);
        s += 2;
    }
    if (s[0]) return -1;
    *outlen = n;
    return 0;
}

static void print_hex(const uint8_t *p, size_t n) {
    for (size_t i = 0; i < n; i++) printf("%02x", p[i]);
}

static uint64_t fnv1a(const uint8_t *p, size_t n) {
    uint64_t h = 14695981039346656037ULL;
    for (size_t i = 0; i < n; i++) {
        h ^= p[i];
        h *= 1099511628211ULL;
    }
    return h;
}

/* name "-" -> empty */
static void namehex(const char *name, char *out) {
    if (strcmp(name, "-") == 0) { out[0] = 0; return; }
    strcpy(out, name);
}

static int call_export(Mod *m, const char *hexname, uint8_t *args, uint8_t *res,
                       int *trap) {
    char symname[512];
    namehex(hexname, symname);
    char full[600];
    snprintf(full, sizeof full, "w2l_export_%s", symname);
    void (*fn)(uint8_t *, uint8_t *) = (void (*)(uint8_t *, uint8_t *))sym(m, full);
    if (!fn) return -1;
    trap_code = -1;
    if (sigsetjmp(jb, 1) == 0) {
        fn(args, res);
        *trap = -1;
    } else {
        *trap = trap_code;
    }
    return 0;
}


int main(void) {
    setvbuf(stdout, NULL, _IONBF, 0);
    setup_signals();
    char *line = NULL;
    size_t cap = 0;
    ssize_t len;
    while ((len = getline(&line, &cap, stdin)) >= 0) {
        while (len > 0 && (line[len-1] == '\n' || line[len-1] == '\r')) line[--len] = 0;
        char *argv[80];
        int argc = 0;
        char *save = NULL;
        char *t = strtok_r(line, " ", &save);
        while (t && argc < 80) { argv[argc++] = t; t = strtok_r(NULL, " ", &save); }
        if (argc == 0) continue;
        const char *cmd = argv[0];
        if (!strcmp(cmd, "quit")) break;
        if (!strcmp(cmd, "ping")) { printf("ok\n"); continue; }
        if (!strcmp(cmd, "load")) {
            if (argc < 3) { printf("error bad args\n"); continue; }
            Mod *m = getmod(argv[1]);
            if (!m) { printf("error bad id\n"); continue; }
            m->handle = dlopen(argv[2], RTLD_NOW | RTLD_LOCAL);
            if (!m->handle) { printf("error %s\n", dlerror()); continue; }
            m->init = (void (*)(void))sym(m, "w2l_init");
            if (!m->init) { printf("error no w2l_init\n"); continue; }
            m->used = 1;
            out_reset();
            printf("ok\n");
            continue;
        }
        Mod *m = argc >= 2 ? getmod(argv[1]) : NULL;
        if (!strcmp(cmd, "init")) {
            if (!m || !m->used) { printf("error unknown module\n"); continue; }
            trap_code = -1;
            if (sigsetjmp(jb, 1) == 0) { m->init(); printf("ok\n"); }
            else printf("trap %d\n", trap_code);
            out_reset();
        } else if (!strcmp(cmd, "invoke")) {
            if (!m || !m->used || argc < 4) { printf("error bad args\n"); continue; }
            const char *hexname = argv[2];
            int nres = atoi(argv[3]);
            static uint8_t args[1 << 16], res[1 << 16];
            memset(args, 0, sizeof args);
            memset(res, 0, sizeof res);
            int nargs = 0;
            for (int i = 4; i < argc; i++) {
                uint8_t buf[16]; size_t bl;
                if (parse_hex(argv[i], buf, 16, &bl) < 0 || bl != 16) break;
                if (nargs >= 4096) break;
                memcpy(SLOT(args, nargs), buf, 16);
                nargs++;
            }
            int trap;
            if (call_export(m, hexname, args, res, &trap) < 0) { printf("error no such export\n"); continue; }
            if (trap >= 0) { printf("trap %d\n", trap); continue; }
            printf("ok");
            for (int i = 0; i < nres && i < 4096; i++) {
                printf(" ");
                print_hex(SLOT(res, i), 16);
            }
            printf("\n");
        } else if (!strcmp(cmd, "global")) {
            if (!m || argc < 3) { printf("error bad args\n"); continue; }
            char symname[512]; namehex(argv[2], symname);
            char full[600]; snprintf(full, sizeof full, "w2l_global_%s", symname);
            void (*fn)(uint8_t *) = (void (*)(uint8_t *))sym(m, full);
            if (!fn) { printf("error no such global\n"); continue; }
            uint8_t res[16]; memset(res, 0, 16);
            trap_code = -1;
            if (sigsetjmp(jb, 1) == 0) { fn(res); printf("ok "); print_hex(res, 16); printf("\n"); }
            else printf("trap %d\n", trap_code);
        } else if (!strcmp(cmd, "memhash")) {
            if (!m || argc < 3) { printf("error bad args\n"); continue; }
            char symname[512]; namehex(argv[2], symname);
            char full[600]; snprintf(full, sizeof full, "w2l_memory_%s", symname);
            void *(*fn)(uint64_t *) = (void *(*)(uint64_t *))sym(m, full);
            if (!fn) { printf("error no such memory\n"); continue; }
            uint64_t sz = 0;
            trap_code = -1;
            if (sigsetjmp(jb, 1) == 0) {
                uint8_t *p = (uint8_t *)fn(&sz);
                printf("ok %llu %016llx\n", (unsigned long long)sz,
                       (unsigned long long)fnv1a(p, sz));
            } else printf("trap %d\n", trap_code);
        } else if (!strcmp(cmd, "memwrite") || !strcmp(cmd, "memread")) {
            if (!m || argc < 4) { printf("error bad args\n"); continue; }
            char symname[512]; namehex(argv[2], symname);
            char full[600]; snprintf(full, sizeof full, "w2l_memory_%s", symname);
            void *(*fn)(uint64_t *) = (void *(*)(uint64_t *))sym(m, full);
            if (!fn) { printf("error no such memory\n"); continue; }
            uint64_t off = strtoull(argv[3], NULL, 10);
            uint64_t sz = 0;
            uint8_t *p = (uint8_t *)fn(&sz);
            if (!strcmp(cmd, "memwrite")) {
                if (argc < 5) { printf("error bad args\n"); continue; }
                static uint8_t buf[1 << 20]; size_t bl;
                if (parse_hex(argv[4], buf, sizeof buf, &bl) < 0) { printf("error bad hex\n"); continue; }
                if (off + bl > sz) { printf("error out of bounds\n"); continue; }
                memcpy(p + off, buf, bl);
                printf("ok\n");
            } else {
                if (argc < 5) { printf("error bad args\n"); continue; }
                uint64_t l = strtoull(argv[4], NULL, 10);
                if (off + l > sz) { printf("error out of bounds\n"); continue; }
                printf("ok ");
                print_hex(p + off, l);
                printf("\n");
            }
        } else if (!strcmp(cmd, "memwrite_global")) {
            if (!m || argc < 5) { printf("error bad args\n"); continue; }
            char symname[512]; namehex(argv[2], symname);
            char full[600]; snprintf(full, sizeof full, "w2l_global_%s", symname);
            void (*gf)(uint8_t *) = (void (*)(uint8_t *))sym(m, full);
            if (!gf) { printf("error no such global\n"); continue; }
            uint8_t gv[16]; memset(gv, 0, 16); gf(gv);
            uint32_t off; memcpy(&off, gv, 4);
            namehex(argv[3], symname);
            snprintf(full, sizeof full, "w2l_memory_%s", symname);
            void *(*mf)(uint64_t *) = (void *(*)(uint64_t *))sym(m, full);
            if (!mf) { printf("error no such memory\n"); continue; }
            uint64_t sz = 0; uint8_t *p = (uint8_t *)mf(&sz);
            static uint8_t buf[1 << 20]; size_t bl;
            if (parse_hex(argv[4], buf, sizeof buf, &bl) < 0) { printf("error bad hex\n"); continue; }
            if ((uint64_t)off + bl > sz) { printf("error out of bounds\n"); continue; }
            memcpy(p + off, buf, bl);
            printf("ok\n");
        } else if (!strcmp(cmd, "output")) {
            if (g_outlen == 0) printf("ok -\n");
            else { printf("ok "); print_hex((uint8_t *)g_out, g_outlen); printf("\n"); }
            out_reset();
        } else {
            printf("error unknown command\n");
        }
    }
    return 0;
}
