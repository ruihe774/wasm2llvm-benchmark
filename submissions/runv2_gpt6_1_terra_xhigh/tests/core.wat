(module
  (import "env" "print_i32" (func $print (param i32)))
  (type $u (func (param i32) (result i32)))
  (type $equal (func (param i32) (result i32)))
  (type $bad (func (result i64)))
  (memory (export "memory") 1 3)
  (table 4 10 funcref)
  (global (export "g") (mut i32) (i32.const 11))
  (data (i32.const 4) "active")
  (data $data "passive!")
  (func $twice (type $u) (param i32) (result i32) local.get 0 i32.const 2 i32.mul)
  (func $other (type $bad) (result i64) i64.const 2)
  (elem (i32.const 0) $twice $other)
  (elem $passive func $twice)
  (func (export "indirect") (param i32 i32) (result i32)
    local.get 1 local.get 0 call_indirect (type $u))
  (func (export "indirect_duplicate_type") (param i32 i32) (result i32)
    local.get 1 local.get 0 call_indirect (type $equal))
  (func (export "table_init")
    i32.const 2 i32.const 0 i32.const 1 table.init $passive)
  (func (export "table_drop") elem.drop $passive)
  (func (export "table_init_range") (param i32 i32 i32)
    local.get 0 local.get 1 local.get 2 table.init $passive)
  (func (export "table_copy") (param i32 i32 i32)
    local.get 0 local.get 1 local.get 2 table.copy)
  (func (export "select") (param i32 i32 i32) (result i32)
    local.get 0 local.get 1 local.get 2 select)
  (func (export "if") (param i32 i32) (result i32)
    local.get 0 if (result i32) local.get 1 else i32.const 77 end)
  (func (export "loop") (param i32) (result i32) (local i32)
    block $exit
      loop $loop
        local.get 0 i32.eqz br_if $exit
        local.get 1 local.get 0 i32.add local.set 1
        local.get 0 i32.const 1 i32.sub local.set 0
        br $loop
      end
    end
    local.get 1)
  (func (export "loop_params") (param i32) (result i32) (local i32 i32)
    i32.const 0 local.get 0
    loop $again (param i32 i32) (result i32)
      local.set 1 local.set 2
      local.get 1
      if (result i32)
        local.get 2 local.get 1 i32.add
        local.get 1 i32.const 1 i32.sub
        br $again
      else
        local.get 2
      end
    end)
  (func (export "branch_table") (param i32) (result i32)
    block $outer (result i32)
      block $inner (result i32)
        i32.const 55 local.get 0 br_table $inner $outer
      end
      i32.const 1 i32.add
    end)
  (func (export "branch") (param i32) (result i32)
    block $b (result i32)
      i32.const 99
      local.get 0 br_if $b
      drop i32.const 3
    end)
  (func (export "load") (param i32) (result i32) local.get 0 i32.load)
  (func (export "load8s") (param i32) (result i32) local.get 0 i32.load8_s)
  (func (export "store") (param i32 i32) local.get 0 local.get 1 i32.store)
  (func (export "grow") (param i32) (result i32) local.get 0 memory.grow)
  (func (export "fill") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 memory.fill)
  (func (export "copy") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 memory.copy)
  (func (export "data_init") (param i32 i32 i32) local.get 0 local.get 1 local.get 2 memory.init $data)
  (func (export "data_drop") data.drop $data)
  (func (export "increment_global") global.get 0 i32.const 1 i32.add global.set 0)
  (func (export "polymorphic_select") unreachable select f32.neg drop)
  (func (export "say") (param i32) local.get 0 call $print)
  (func $unbounded (export "unbounded") call $unbounded)
  (func (export "") (result i32) i32.const 81)
  (func (export "名前😀") (param i32) (result i32) local.get 0)
  (func (export "write_then_trap") i32.const 500 i32.const 73 i32.store8 unreachable)
  (func (export "boom") unreachable)
  (func (export "after") (result i32) i32.const 23)
  (func $rec (export "recurse") (param i32) (result i32)
    local.get 0 i32.eqz if (result i32) i32.const 0 else
      local.get 0 i32.const 1 i32.sub call $rec i32.const 1 i32.add
    end)
)
