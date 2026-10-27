// Minimal native driver for the wasm2llvm ABI. Compile with -rdynamic -ldl.
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>
#include <inttypes.h>

static jmp_buf unwind;
static int trapcode;
static unsigned char hostout[65536];
static size_t hostout_len;
void w2l_trap(int32_t code) { trapcode=code; longjmp(unwind,1); }
static void append(const char *s) {size_t n=strlen(s);if(n<=sizeof hostout-hostout_len){memcpy(hostout+hostout_len,s,n);hostout_len+=n;}}
static void pi32(void *a) {int32_t x; char s[80];memcpy(&x,a,4);snprintf(s,sizeof s,"i32:%" PRId32 "\n",x);append(s);}
static void pi64(void *a) {int64_t x; char s[80];memcpy(&x,a,8);snprintf(s,sizeof s,"i64:%" PRId64 "\n",x);append(s);}
static void pf32(void *a) {uint32_t x; char s[80];memcpy(&x,a,4);snprintf(s,sizeof s,"f32:0x%08" PRIx32 "\n",x);append(s);}
static void pf64(void *a) {uint64_t x; char s[80];memcpy(&x,a,8);snprintf(s,sizeof s,"f64:0x%016" PRIx64 "\n",x);append(s);}
void w2l_import_656e76_7072696e745f693332(void*a,void*r){(void)r;pi32(a);}
void w2l_import_656e76_7072696e745f693634(void*a,void*r){(void)r;pi64(a);}
void w2l_import_656e76_7072696e745f663332(void*a,void*r){(void)r;pf32(a);}
void w2l_import_656e76_7072696e745f663634(void*a,void*r){(void)r;pf64(a);}
void w2l_import_656e76_70757463686172(void*a,void*r){(void)r;if(hostout_len<sizeof hostout)hostout[hostout_len++]=*(unsigned char*)a;}
void w2l_import_656e76_7072696e745f76313238(void*a,void*r){(void)r; char s[40]="v128:";for(int i=0;i<16;i++)sprintf(s+5+2*i,"%02x",((unsigned char*)a)[i]);strcat(s,"\n");append(s);}
void w2l_import_7370656374657374_7072696e74(void*a,void*r){(void)a;(void)r;append("print\n");}
void w2l_import_7370656374657374_7072696e745f693332(void*a,void*r){(void)r;pi32(a);}
void w2l_import_7370656374657374_7072696e745f693634(void*a,void*r){(void)r;pi64(a);}
void w2l_import_7370656374657374_7072696e745f663332(void*a,void*r){(void)r;pf32(a);}
void w2l_import_7370656374657374_7072696e745f663634(void*a,void*r){(void)r;pf64(a);}
void w2l_import_7370656374657374_7072696e745f6933325f663332(void*a,void*r){(void)r;pi32(a);pf32((char*)a+16);}
void w2l_import_7370656374657374_7072696e745f6636345f663634(void*a,void*r){(void)r;pf64(a);pf64((char*)a+16);}
static int unhex(const char*s,unsigned char*d,size_t cap){size_t n=strlen(s);if(n%2||n/2>cap)return -1;for(size_t i=0;i<n/2;i++){unsigned int v;if(sscanf(s+2*i,"%2x",&v)!=1)return -1;d[i]=(unsigned char)v;}return (int)(n/2);}
static void printhex(const unsigned char*s,size_t n){for(size_t i=0;i<n;i++)printf("%02x",s[i]);}
static void *symbol(void *lib,const char*prefix,const char*name){char b[1024];snprintf(b,sizeof b,"%s%s",prefix,!strcmp(name,"-")?"":name);return dlsym(lib,b);}
int main(int argc,char **argv){
    setvbuf(stdout,NULL,_IOLBF,0);
    if(argc!=2){fprintf(stderr,"usage: driver module.so\n");return 2;}
    void *lib=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);if(!lib){fprintf(stderr,"dlopen: %s\n",dlerror());return 2;}
    char *line=NULL;size_t capacity=0;
    while(getline(&line,&capacity,stdin)>0){char *cmd=strtok(line," \t\r\n"),*name;if(!cmd)continue;
        if(!strcmp(cmd,"quit"))break;
        if(!strcmp(cmd,"ping")){puts("ok");continue;}
        if(!strcmp(cmd,"init")){void(*f)(void)=dlsym(lib,"w2l_init");if(!f){puts("error init");continue;}if(setjmp(unwind)){printf("trap %d\n",trapcode);}else{f();puts("ok");}continue;}
        if(!strcmp(cmd,"output")){printf("ok ");if(!hostout_len)printf("-");else printhex(hostout,hostout_len);hostout_len=0;putchar('\n');continue;}
        name=strtok(NULL," \t\r\n");if(!name){puts("error missing-name");continue;}
        if(!strcmp(cmd,"invoke")){char *nstr=strtok(NULL," \t\r\n");if(!nstr){puts("error missing-results");continue;}int n=atoi(nstr);if(n<0||n>256){puts("error results");continue;}_Alignas(16) unsigned char args[4096]={0},results[4096]={0};int i=0;char *a;while((a=strtok(NULL," \t\r\n"))){if(i==256||unhex(a,args+16*i,16)!=16){i=-1;break;}i++;}if(i<0){puts("error args");continue;}void(*f)(void*,void*)=symbol(lib,"w2l_export_",name);if(!f){puts("error export");continue;}if(setjmp(unwind)){printf("trap %d\n",trapcode);}else{f(args,results);printf("ok");for(i=0;i<n;i++){putchar(' ');printhex(results+16*i,16);}putchar('\n');}continue;}
        if(!strcmp(cmd,"global")){_Alignas(16) unsigned char v[16]={0};void(*f)(void*)=symbol(lib,"w2l_global_",name);if(!f){puts("error global");continue;}f(v);printf("ok ");printhex(v,16);putchar('\n');continue;}
        if(!strcmp(cmd,"memread")||!strcmp(cmd,"memwrite")||!strcmp(cmd,"memhash")){uint64_t size=0;void*(*f)(void*)=symbol(lib,"w2l_memory_",name);if(!f){puts("error memory");continue;}unsigned char *mem=f(&size);if(!strcmp(cmd,"memhash")){uint64_t h=14695981039346656037ull;for(uint64_t i=0;i<size;i++)h=(h^mem[i])*1099511628211ull;printf("ok %" PRIu64 " %016" PRIx64 "\n",size,h);continue;}char *offstr=strtok(NULL," \t\r\n"),*data=strtok(NULL," \t\r\n");if(!offstr||!data){puts("error args");continue;}uint64_t off=strtoull(offstr,NULL,0);if(!strcmp(cmd,"memread")){uint64_t n=strtoull(data,NULL,0);if(off>size||n>size-off){puts("error bounds");continue;}printf("ok ");printhex(mem+off,n);putchar('\n');}else{size_t n=strlen(data)/2;if(off>size||n>size-off||unhex(data,mem+off,n)<0){puts("error bounds");continue;}puts("ok");}continue;}
        puts("error command");
    }free(line);dlclose(lib);return 0;
}
