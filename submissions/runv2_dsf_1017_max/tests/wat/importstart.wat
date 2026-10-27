(module
  (import "spectest" "print" (func $p))
  (import "env" "print_i32" (func $q (param i32)))
  (func $f (call $q (i32.const 7)))
  (export "f" (func $f))
  (export "p_alias" (func $p))
  (start $p))
