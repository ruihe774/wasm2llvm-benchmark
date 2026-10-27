(module
  (func $count (export "count") (param $n i32) (result i32)
    local.get $n
    if (result i32)
      local.get $n i32.const 1 i32.sub call $count i32.const 1 i32.add
    else
      i32.const 0
    end)
  (func $tail (export "tail") (param $n i32) (result i32)
    local.get $n
    if (result i32)
      local.get $n i32.const 1 i32.sub call $tail
    else
      i32.const 72
    end)
  (func $forever (export "forever") call $forever)
  (func $even (export "mutual") call $odd)
  (func $odd call $even)
  (func (export "trap") unreachable)
)
