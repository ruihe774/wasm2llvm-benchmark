(module
  (memory (export "memory") 0)
  (data (i32.const 0) "")
  (func (export "grow") (param i32) (result i32) local.get 0 memory.grow)
  (func (export "size") (result i32) memory.size)
  (func (export "store8") (param i32 i32) local.get 0 local.get 1 i32.store8)
  (func (export "load8") (param i32) (result i32) local.get 0 i32.load8_u)
  (func (export "load32") (param i32) (result i32) local.get 0 i32.load)
)
