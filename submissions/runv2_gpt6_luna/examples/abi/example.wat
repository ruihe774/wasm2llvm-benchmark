;; Example module used to illustrate the w2l ABI. example.ll is a hand-written
;; (unoptimized, deliberately simple) translation of this module that follows
;; the ABI in ABI.md. It is an illustration, not a template you must copy.
(module
  (import "env" "print_i32" (func $print_i32 (param i32)))
  (memory (export "memory") 1 2)
  (global $counter (export "counter") (mut i32) (i32.const 0))

  ;; multi-value result, integer division traps
  (func (export "divmod") (param i32 i32) (result i32 i32)
    (i32.div_u (local.get 0) (local.get 1))
    (i32.rem_u (local.get 0) (local.get 1)))

  ;; host call, global update, bounds-checked store
  (func (export "bump") (param $addr i32) (result i32)
    (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
    (call $print_i32 (global.get $counter))
    (i32.store (local.get $addr) (global.get $counter))
    (global.get $counter))

  ;; unbounded recursion -> call stack exhausted
  (func $recurse (export "recurse") (param i32) (result i32)
    (i32.add (call $recurse (i32.add (local.get 0) (i32.const 1))) (i32.const 1)))

  (data (i32.const 16) "hi"))
