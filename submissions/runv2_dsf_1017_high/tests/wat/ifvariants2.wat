(module
  (func $a (export "a") (param i32) (result i32)
    local.get 0
    local.get 0
    (if (param i32) (result i32) (then)))
  (func $b (export "b") (param i32) (result i32)
    local.get 0
    local.get 0
    (if (param i32) (result i32) (then drop i32.const 3) (else)))
  (func $c (export "c") (param i32) (result i32)
    local.get 0
    (if (result i32) (then i32.const 2) (else i32.const 4)))
)
