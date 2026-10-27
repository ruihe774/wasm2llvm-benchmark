#define _POSIX_C_SOURCE 200809L
#include <dlfcn.h>
#include <inttypes.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Test-only line protocol driver. Build with -rdynamic -ldl. */
typedef struct {
    char *id;
    void *handle;
    unsigned char *output;
    size_t used, capacity;
    int initialized;
} Instance;
static Instance instances[128];
static Instance *active;
static jmp_buf trap_target;
static volatile int armed, trap_code;

__attribute__((noreturn)) void w2l_trap(int32_t code) {
    if (!armed) { fprintf(stderr, "unexpected trap %d\n", code); exit(2); }
    trap_code = code;
    longjmp(trap_target, 1);
}
static void append(const void *bytes, size_t n) {
    if (!active) return;
    if (n > SIZE_MAX - active->used) abort();
    size_t need = active->used + n;
    if (need > active->capacity) {
        size_t cap = need > 4096 ? need * 2 : 4096;
        void *p = realloc(active->output, cap);
        if (!p) abort();
        active->output = p; active->capacity = cap;
    }
    memcpy(active->output + active->used, bytes, n); active->used += n;
}
static void format(const char *fmt, ...) {
    char buffer[256]; va_list args; va_start(args, fmt);
    int n = vsnprintf(buffer, sizeof(buffer), fmt, args); va_end(args);
    if (n < 0 || (size_t)n >= sizeof(buffer)) abort();
    append(buffer, (size_t)n);
}
static void print_i32(const unsigned char *a) { int32_t x; memcpy(&x,a,4); format("i32:%" PRId32 "\n",x); }
static void print_i64(const unsigned char *a) { int64_t x; memcpy(&x,a,8); format("i64:%" PRId64 "\n",x); }
static void print_f32(const unsigned char *a) { uint32_t x; memcpy(&x,a,4); format("f32:0x%08" PRIx32 "\n",x); }
static void print_f64(const unsigned char *a) { uint64_t x; memcpy(&x,a,8); format("f64:0x%016" PRIx64 "\n",x); }
static void print_v128(const unsigned char *a) {
    static const char h[] = "0123456789abcdef"; char text[38] = "v128:";
    for (int i=0; i<16; ++i) { text[5+i*2]=h[a[i]>>4]; text[6+i*2]=h[a[i]&15]; }
    text[37]='\n'; append(text,38);
}
#define HOST(cname, symbol, body) \
    void cname(const unsigned char *a, unsigned char *r) __asm__(symbol); \
    void cname(const unsigned char *a, unsigned char *r) { (void)a; (void)r; body; }
HOST(env_i32,"w2l_import_656e76_7072696e745f693332", print_i32(a))
HOST(env_i64,"w2l_import_656e76_7072696e745f693634", print_i64(a))
HOST(env_f32,"w2l_import_656e76_7072696e745f663332", print_f32(a))
HOST(env_f64,"w2l_import_656e76_7072696e745f663634", print_f64(a))
HOST(env_v128,"w2l_import_656e76_7072696e745f76313238", print_v128(a))
HOST(env_putchar,"w2l_import_656e76_70757463686172", append(a,1))
HOST(spec_print,"w2l_import_7370656374657374_7072696e74", append("print\n",6))
HOST(spec_i32,"w2l_import_7370656374657374_7072696e745f693332", print_i32(a))
HOST(spec_i64,"w2l_import_7370656374657374_7072696e745f693634", print_i64(a))
HOST(spec_f32,"w2l_import_7370656374657374_7072696e745f663332", print_f32(a))
HOST(spec_f64,"w2l_import_7370656374657374_7072696e745f663634", print_f64(a))
HOST(spec_i32_f32,"w2l_import_7370656374657374_7072696e745f6933325f663332", print_i32(a); print_f32(a+16))
HOST(spec_f64_f64,"w2l_import_7370656374657374_7072696e745f6636345f663634", print_f64(a); print_f64(a+16))

static int digit(char c) {
    if (c>='0' && c<='9') return c-'0';
    if (c>='a' && c<='f') return c-'a'+10;
    if (c>='A' && c<='F') return c-'A'+10;
    return -1;
}
static int unhex(const char *s, unsigned char *out, size_t n) {
    if (strlen(s)!=n*2) return 0;
    for (size_t i=0;i<n;++i) {
        int a=digit(s[i*2]), b=digit(s[i*2+1]); if (a<0 || b<0) return 0;
        out[i]=(unsigned char)((a<<4)|b);
    }
    return 1;
}
static void hex(const unsigned char *p, size_t n) {
    static const char h[]="0123456789abcdef";
    for (size_t i=0;i<n;++i) { putchar(h[p[i]>>4]); putchar(h[p[i]&15]); }
}
static int number(const char *s, uint64_t *n) {
    if (!s || !*s || *s=='-') return 0;
    char *end; *n=strtoull(s,&end,10); return !*end;
}
static Instance *find(const char *id) {
    if (!id) return NULL;
    for (size_t i=0;i<128;++i) if (instances[i].id && !strcmp(id,instances[i].id)) return &instances[i];
    return NULL;
}
static void *symbol(Instance *i, const char *prefix, const char *name) {
    if (!name) return NULL;
    if (!strcmp(name,"-")) name="";
    size_t n=strlen(name);
    for (size_t j=0;j<n;++j) if (digit(name[j])<0) return NULL;
    if (n%2) return NULL;
    char *full=malloc(strlen(prefix)+n+1); if (!full) abort();
    strcpy(full,prefix); strcat(full,name); void *p=dlsym(i->handle,full); free(full); return p;
}
static int exported_memory(Instance *i, const char *name, unsigned char **base, uint64_t *size) {
    void *(*get)(uint64_t *)=symbol(i,"w2l_memory_",name);
    if (!get) return 0;
    *base=get(size); return 1;
}
static int exported_global(Instance *i, const char *name, unsigned char *slot) {
    void (*get)(void *)=symbol(i,"w2l_global_",name);
    if (!get) return 0;
    get(slot); return 1;
}
static void command(char **tokens, size_t count) {
#define ARG(n) ((n)<count ? tokens[n] : NULL)
    const char *cmd=ARG(0);
    if (!cmd) return;
    if (!strcmp(cmd,"ping")) { puts("ok"); return; }
    if (!strcmp(cmd,"load")) {
        if (count!=3) { puts("error expected load id path"); return; }
        Instance *i=find(ARG(1));
        if (!i) for (size_t k=0;k<128;++k) if (!instances[k].id) { i=&instances[k]; break; }
        if (!i) { puts("error too many instances"); return; }
        if (i->handle) dlclose(i->handle);
        free(i->id); free(i->output); memset(i,0,sizeof(*i));
        i->handle=dlopen(ARG(2),RTLD_NOW|RTLD_LOCAL);
        if (!i->handle) { printf("error %s\n",dlerror()); return; }
        i->id=strdup(ARG(1)); puts("ok"); return;
    }
    Instance *i=find(ARG(1));
    if (!i) { puts("error unknown instance"); return; }
    if (!strcmp(cmd,"init")) {
        void (*init)(void)=dlsym(i->handle,"w2l_init");
        if (!init || i->initialized) { puts("error missing init or instance already initialized"); return; }
        active=i; armed=1;
        if (!setjmp(trap_target)) { init(); puts("ok"); } else printf("trap %d\n",trap_code);
        armed=0; active=NULL; i->initialized=1; return;
    }
    if (!i->initialized) { puts("error instance is not initialized"); return; }
    if (!strcmp(cmd,"invoke")) {
        uint64_t results;
        if (count<4 || !number(ARG(3),&results) || results>4096 || count-4>4096) { puts("error invalid invoke arguments"); return; }
        void (*invoke)(void *,void *)=symbol(i,"w2l_export_",ARG(2));
        if (!invoke) { puts("error missing export"); return; }
        size_t na=count-4, nr=(size_t)results;
        unsigned char *args=NULL,*out=NULL;
        if (posix_memalign((void **)&args,16,(na+1)*16) || posix_memalign((void **)&out,16,(nr+1)*16)) abort();
        memset(args,0,(na+1)*16); memset(out,0,(nr+1)*16);
        for (size_t k=0;k<na;++k) if (!unhex(ARG(k+4),args+k*16,16)) { free(args); free(out); puts("error invalid slot"); return; }
        active=i; armed=1;
        if (!setjmp(trap_target)) {
            invoke(args,out); fputs("ok",stdout);
            for (size_t k=0;k<nr;++k) { putchar(' '); hex(out+k*16,16); }
            putchar('\n');
        } else printf("trap %d\n",trap_code);
        armed=0; active=NULL; free(args); free(out); return;
    }
    if (!strcmp(cmd,"global")) {
        _Alignas(16) unsigned char slot[16]={0};
        if (!exported_global(i,ARG(2),slot)) { puts("error missing global"); return; }
        fputs("ok ",stdout); hex(slot,16); putchar('\n'); return;
    }
    if (!strcmp(cmd,"output")) {
        fputs("ok ",stdout);
        if (!i->used) putchar('-'); else hex(i->output,i->used);
        putchar('\n'); i->used=0; return;
    }
    if (!strcmp(cmd,"memread") || !strcmp(cmd,"memwrite") || !strcmp(cmd,"memhash") || !strcmp(cmd,"memwrite_global")) {
        unsigned char *base; uint64_t size,offset=0,length=0;
        int from_global=!strcmp(cmd,"memwrite_global");
        if (!exported_memory(i,ARG(from_global?3:2),&base,&size)) { puts("error missing memory"); return; }
        if (!strcmp(cmd,"memhash")) {
            uint64_t hash=UINT64_C(14695981039346656037);
            for (uint64_t k=0;k<size;++k) { hash^=base[k]; hash*=UINT64_C(1099511628211); }
            printf("ok %" PRIu64 " %016" PRIx64 "\n",size,hash); return;
        }
        if (from_global) {
            _Alignas(16) unsigned char slot[16]={0}; uint32_t value;
            if (!exported_global(i,ARG(2),slot)) { puts("error missing global"); return; }
            memcpy(&value,slot,4); offset=value;
        } else if (!number(ARG(3),&offset)) { puts("error invalid offset"); return; }
        if (!strcmp(cmd,"memread")) {
            if (!number(ARG(4),&length) || offset>size || length>size-offset) { puts("error out of bounds read"); return; }
            fputs("ok ",stdout); hex(base+offset,(size_t)length); putchar('\n'); return;
        }
        const char *bytes=ARG(4);
        if (!bytes) { puts("error missing bytes"); return; }
        length=strlen(bytes)/2;
        if (offset>size || length>size-offset || !unhex(bytes,base+offset,(size_t)length)) { puts("error invalid or out of bounds write"); return; }
        puts("ok"); return;
    }
    puts("error unknown command");
#undef ARG
}
int main(void) {
    char *line=NULL; size_t capacity=0;
    while (getline(&line,&capacity,stdin)>=0) {
        char **tokens=NULL; size_t count=0, allocated=0; char *state;
        for (char *p=strtok_r(line," \t\r\n",&state);p;p=strtok_r(NULL," \t\r\n",&state)) {
            if (count==allocated) { allocated=allocated?allocated*2:16; tokens=realloc(tokens,allocated*sizeof(*tokens)); if (!tokens) abort(); }
            tokens[count++]=p;
        }
        if (count && !strcmp(tokens[0],"quit")) { free(tokens); break; }
        command(tokens,count); free(tokens); fflush(stdout);
    }
    free(line);
    for (size_t i=0;i<128;++i) { if (instances[i].handle) dlclose(instances[i].handle); free(instances[i].id); free(instances[i].output); }
    return 0;
}
