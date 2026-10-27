(module
  (memory (export "memory") 1)
  (global (export "g") (mut i32) (i32.const 0))
  (table 2 funcref)
  (func $f (result i32) (i32.const 42))
  (elem (i32.const 0) $f)
  (data (i32.const 100) "AB")
  (func $s
    (global.set 0 (i32.add
      (i32.load8_u (i32.const 100))
      (call_indirect (type 0) (i32.const 0)))))
  (type (func (result i32)))
  (start $s))
