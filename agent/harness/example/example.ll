; Hand-written translation of example.wat following the w2l ABI (see TASK.md).
; Build & run:
;   clang -O2 -shared -fPIC -Wl,-Bsymbolic -o example.so example.ll -lm
;   printf 'load m ./example.so\ninit m\ninvoke m 6469766d6f64 2 07000000000000000000000000000000 02000000000000000000000000000000\n' | ../w2l_harness

target triple = "x86_64-unknown-linux-gnu"

; ---- runtime services provided by the harness ----
declare void @w2l_trap(i32) noreturn nounwind
declare void @w2l_import_656e76_7072696e745f693332(ptr, ptr)   ; env.print_i32

; libc
declare ptr @calloc(i64, i64)

; ---- module state (internal) ----
@mem.base  = internal global ptr null
@mem.size  = internal global i64 0          ; bytes
@g.counter = internal global i32 0
@data.0    = private constant [2 x i8] c"hi"

; ---- wasm functions (internal, native signatures) ----
define internal { i32, i32 } @f.divmod(i32 %a, i32 %b) {
entry:
  %z = icmp eq i32 %b, 0
  br i1 %z, label %trap, label %ok
trap:
  call void @w2l_trap(i32 1)                ; integer divide by zero
  unreachable
ok:
  %q = udiv i32 %a, %b
  %r = urem i32 %a, %b
  %t0 = insertvalue { i32, i32 } poison, i32 %q, 0
  %t1 = insertvalue { i32, i32 } %t0, i32 %r, 1
  ret { i32, i32 } %t1
}

define internal i32 @f.bump(i32 %addr) {
entry:
  %c0 = load i32, ptr @g.counter
  %c1 = add i32 %c0, 1
  store i32 %c1, ptr @g.counter
  ; host call through the uniform slot ABI
  %args = alloca [1 x <16 x i8>], align 16
  %res = alloca [1 x <16 x i8>], align 16
  store <16 x i8> zeroinitializer, ptr %args
  store i32 %c1, ptr %args
  call void @w2l_import_656e76_7072696e745f693332(ptr %args, ptr %res)
  ; i32.store with offset 0: check addr + 4 <= size
  %c2 = load i32, ptr @g.counter
  %ea = zext i32 %addr to i64
  %end = add i64 %ea, 4
  %size = load i64, ptr @mem.size
  %oob = icmp ugt i64 %end, %size
  br i1 %oob, label %trap, label %ok
trap:
  call void @w2l_trap(i32 4)                ; out of bounds memory access
  unreachable
ok:
  %base = load ptr, ptr @mem.base
  %p = getelementptr i8, ptr %base, i64 %ea
  store i32 %c2, ptr %p, align 1
  %c3 = load i32, ptr @g.counter
  ret i32 %c3
}

; NOTE: relying on the native stack overflowing is not enough: LLVM may turn
; recursion into a loop (tail-recursion elimination), which would hang instead
; of trapping. This example uses an explicit call-depth counter.
@depth = internal global i32 0

define internal i32 @f.recurse(i32 %x) {
entry:
  %d0 = load i32, ptr @depth
  %d1 = add i32 %d0, 1
  store i32 %d1, ptr @depth
  %deep = icmp ugt i32 %d1, 50000
  br i1 %deep, label %exhausted, label %body
exhausted:
  call void @w2l_trap(i32 8)                ; call stack exhausted
  unreachable
body:
  %x1 = add i32 %x, 1
  %r = call i32 @f.recurse(i32 %x1)
  %r1 = add i32 %r, 1
  %d2 = load i32, ptr @depth
  %d3 = sub i32 %d2, 1
  store i32 %d3, ptr @depth
  ret i32 %r1
}

; ---- ABI entry points ----
define void @w2l_init() {
entry:
  %m = call ptr @calloc(i64 65536, i64 1)
  store ptr %m, ptr @mem.base
  store i64 65536, ptr @mem.size
  store i32 0, ptr @g.counter
  ; active data segment at offset 16 (bounds already known to be fine here)
  %dst = getelementptr i8, ptr %m, i64 16
  call void @llvm.memcpy.p0.p0.i64(ptr %dst, ptr @data.0, i64 2, i1 false)
  ret void
}

define void @w2l_export_6469766d6f64(ptr %args, ptr %results) {     ; "divmod"
entry:
  %a = load i32, ptr %args
  %pb = getelementptr i8, ptr %args, i64 16
  %b = load i32, ptr %pb
  %r = call { i32, i32 } @f.divmod(i32 %a, i32 %b)
  %r0 = extractvalue { i32, i32 } %r, 0
  %r1 = extractvalue { i32, i32 } %r, 1
  store i32 %r0, ptr %results
  %pr1 = getelementptr i8, ptr %results, i64 16
  store i32 %r1, ptr %pr1
  ret void
}

define void @w2l_export_62756d70(ptr %args, ptr %results) {         ; "bump"
entry:
  %a = load i32, ptr %args
  %r = call i32 @f.bump(i32 %a)
  store i32 %r, ptr %results
  ret void
}

define void @w2l_export_72656375727365(ptr %args, ptr %results) {   ; "recurse"
entry:
  ; a previous call may have trapped (longjmp) with a non-zero depth
  store i32 0, ptr @depth
  %a = load i32, ptr %args
  %r = call i32 @f.recurse(i32 %a)
  store i32 %r, ptr %results
  ret void
}

define void @w2l_global_636f756e746572(ptr %out) {                   ; "counter"
entry:
  %v = load i32, ptr @g.counter
  store i32 %v, ptr %out
  ret void
}

define ptr @w2l_memory_6d656d6f7279(ptr %size_out) {                 ; "memory"
entry:
  %s = load i64, ptr @mem.size
  store i64 %s, ptr %size_out
  %b = load ptr, ptr @mem.base
  ret ptr %b
}

declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)
