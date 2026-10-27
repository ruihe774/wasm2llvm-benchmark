(module
  (func $sum (export "sum") (param i32) (result i32)
    local.get 0
    i32.eqz
    if (result i32)
      i32.const 0
    else
      local.get 0
      i32.const 1
      i32.sub
      call $sum
      local.get 0
      i32.add
    end)
  (func $loop_tail (export "loop_tail") (param i32) (result i32)
    local.get 0
    i32.const 1
    i32.add
    call $loop_tail)
  (func $even (export "even") (param i32) (result i32)
    local.get 0
    i32.eqz
    if (result i32)
      i32.const 1
    else
      local.get 0
      i32.const 1
      i32.sub
      call $odd
    end)
  (func $odd (export "odd") (param i32) (result i32)
    local.get 0
    i32.eqz
    if (result i32)
      i32.const 0
    else
      local.get 0
      i32.const 1
      i32.sub
      call $even
    end)
  (func $inf (export "inf") (result i32) call $inf i32.const 1 i32.add)
)
