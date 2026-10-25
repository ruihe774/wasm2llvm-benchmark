(module
  (memory (export "memory") 0 2)
  (func (export "size") (result i32) (memory.size))
  (func (export "grow") (param i32) (result i32) (memory.grow (local.get 0)))
  (func (export "sl") (param i32 i32) (result i32) (i32.store (local.get 0) (local.get 1)) (i32.load (local.get 0))))
