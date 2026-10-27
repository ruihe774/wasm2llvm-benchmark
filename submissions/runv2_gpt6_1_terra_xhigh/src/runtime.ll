; The generated instance owns one unshared wasm32 linear memory.
declare void @w2l_trap(i32) noreturn
declare ptr @calloc(i64, i64)
declare ptr @realloc(ptr, i64)
declare ptr @mmap(ptr, i64, i32, i32, i32, i64)
declare i32 @munmap(ptr, i64)
declare void @free(ptr)
declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)
declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)

@w2l_mem_base = internal global ptr null
@w2l_mem_bytes = internal global i64 0
@w2l_mem_reserved = internal global i1 false
@w2l_depth = internal global i32 0

define internal ptr @w2l_mem_ptr(i32 %addr, i64 %offset, i64 %len) {
entry:
  %a = zext i32 %addr to i64
  %start = add i64 %a, %offset
  %end = add i64 %start, %len
  %size = load i64, ptr @w2l_mem_bytes
  %bad = icmp ugt i64 %end, %size
  br i1 %bad, label %trap, label %good
trap:
  call void @w2l_trap(i32 4)
  unreachable
good:
  %base = load ptr, ptr @w2l_mem_base
  %p = getelementptr i8, ptr %base, i64 %start
  ret ptr %p
}

define internal i32 @w2l_mem_grow(i32 %delta) {
entry:
  %oldbytes = load i64, ptr @w2l_mem_bytes
  %oldpages = lshr i64 %oldbytes, 16
  %d = zext i32 %delta to i64
  %newpages = add i64 %oldpages, %d
  %too_large = icmp ugt i64 %newpages, @W2L_MEMORY_MAX@
  br i1 %too_large, label %fail, label %size_ok
size_ok:
  %empty = icmp eq i32 %delta, 0
  br i1 %empty, label %done, label %grow
grow:
  %reserved = load i1, ptr @w2l_mem_reserved
  br i1 %reserved, label %mapped, label %allocate
mapped:
  %mappedbytes = shl i64 %newpages, 16
  store i64 %mappedbytes, ptr @w2l_mem_bytes
  br label %done
allocate:
  %newbytes = shl i64 %newpages, 16
  %oldbase = load ptr, ptr @w2l_mem_base
  %base = call ptr @realloc(ptr %oldbase, i64 %newbytes)
  %oom = icmp eq ptr %base, null
  br i1 %oom, label %fail, label %clear
clear:
  %newstart = getelementptr i8, ptr %base, i64 %oldbytes
  %amount = sub i64 %newbytes, %oldbytes
  call void @llvm.memset.p0.i64(ptr %newstart, i8 0, i64 %amount, i1 false)
  store ptr %base, ptr @w2l_mem_base
  store i64 %newbytes, ptr @w2l_mem_bytes
  br label %done
done:
  %answer = trunc i64 %oldpages to i32
  ret i32 %answer
fail:
  ret i32 -1
}

define internal void @w2l_mem_copy(i32 %dest, i32 %src, i32 %n) {
entry:
  %len = zext i32 %n to i64
  %d = call ptr @w2l_mem_ptr(i32 %dest, i64 0, i64 %len)
  %s = call ptr @w2l_mem_ptr(i32 %src, i64 0, i64 %len)
  call void @llvm.memmove.p0.p0.i64(ptr %d, ptr %s, i64 %len, i1 false)
  ret void
}

define internal void @w2l_mem_fill(i32 %dest, i32 %byte, i32 %n) {
entry:
  %len = zext i32 %n to i64
  %d = call ptr @w2l_mem_ptr(i32 %dest, i64 0, i64 %len)
  %v = trunc i32 %byte to i8
  call void @llvm.memset.p0.i64(ptr %d, i8 %v, i64 %len, i1 false)
  ret void
}
