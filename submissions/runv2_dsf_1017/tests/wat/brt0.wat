(module
  (func (export "f") (param i32) (result i32)
    (block (result i32)
      (i32.const 77)
      (local.get 0)
      (br_table 0)
      (i32.const 0)))
)
