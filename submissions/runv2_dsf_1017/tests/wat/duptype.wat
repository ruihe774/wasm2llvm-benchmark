(module
  (type $a (func (result i32)))
  (type $b (func (result i32)))
  (table 1 funcref)
  (func $f (type $b) (result i32) (i32.const 42))
  (elem (i32.const 0) $f)
  (func (export "go") (result i32)
    (call_indirect (type $a) (i32.const 0))))
