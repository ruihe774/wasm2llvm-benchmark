(module
  (memory (export "memory") 1 2)
  (table 8 funcref)
  (type $sig (func (result i32)))
  (type $sig2 (func (param i32) (result i32)))
  (global $counter (export "counter") (mut i32) (i32.const 0))
  (global $vg (export "vg") (mut v128) (v128.const i32x4 1 2 3 4))
  (global $lastch (export "lastch") i32 (i32.const 0))
  (func $t0 (result i32) (i32.const 10))
  (func $t1 (result i32) (i32.const 20))
  (func $start (global.set $counter (i32.const 77)))
  (start $start)

  (elem (i32.const 0) $t0 $t1)
  (elem func $t0 $t1)
  (elem declare func $t0)

  (data "hello")
  (data (i32.const 100) "world")

  (func (export "call_ind") (param i32) (result i32)
    (call_indirect (type $sig) (local.get 0)))
  (func (export "call_ind_ty") (param i32) (result i32)
    (call_indirect (type $sig2) (i32.const 0) (local.get 0)))
  (func (export "tinit") (param i32 i32 i32)
    (table.init 1 (local.get 0) (local.get 1) (local.get 2)))
  (func (export "tcopy") (param i32 i32 i32)
    (table.copy (local.get 0) (local.get 1) (local.get 2)))
  (func (export "edrop") (elem.drop 1))
  (func (export "minit") (param i32 i32 i32)
    (memory.init 0 (local.get 0) (local.get 1) (local.get 2)))
  (func (export "ddrop") (data.drop 0))
  (func (export "mcopy") (param i32 i32 i32)
    (memory.copy (local.get 0) (local.get 1) (local.get 2)))
  (func (export "mfill") (param i32 i32 i32)
    (memory.fill (local.get 0) (local.get 1) (local.get 2)))
  (func (export "grow") (param i32) (result i32) (memory.grow (local.get 0)))
  (func (export "msize") (result i32) (memory.size))
  (func (export "ld") (param i32) (result i32) (i32.load (local.get 0)))
  (func (export "ld_off") (param i32) (result i32) (i32.load offset=4294967295 (local.get 0)))
  (func (export "ld16_off") (param i32) (result i32) (i32.load16_u offset=65534 (local.get 0)))
  (func (export "st") (param i32 i32) (i32.store (local.get 0) (local.get 1)))
  (func (export "st8") (param i32 i32) (i32.store8 (local.get 0) (local.get 1)))
  (func (export "ld64") (param i32) (result i64) (i64.load (local.get 0)))
  (func (export "i64st32") (param i32 i64) (i64.store32 (local.get 0) (local.get 1)))
  (func (export "unreach") (unreachable))
  (func (export "getcount") (result i32) (global.get $counter))
  (func (export "setcount") (param i32) (global.set $counter (local.get 0)))

  (func (export "loopparam") (param i32) (result i32)
    (local i32)
    local.get 0
    loop $l (param i32) (result i32)
      local.tee 1
      i32.const 1
      i32.sub
      local.tee 1
      i32.const 0
      i32.gt_s
      if (result i32)
        local.get 1
        br $l
      else
        local.get 1
        i32.const 42
        i32.add
      end
    end)
  (func (export "brtable") (param i32) (result i32)
    block $a (result i32)
      block $b (result i32)
        block $c (result i32)
          i32.const 11
          local.get 0
          br_table $c $b $a
        end
        i32.const 1
        i32.add
      end
      i32.const 100
      i32.add
    end)
  (func (export "blockmulti") (param i32 i32) (result i32 i32)
    local.get 0
    local.get 1
    block (param i32 i32) (result i32 i32)
    end)
  (func (export "ifparam") (param i32 i32) (result i32)
    local.get 0
    local.get 1
    (if (param i32) (result i32) (then) (else i32.const 5 i32.add)))
  (func (export "brifvals") (param i32 i32) (result i32)
    block $b (result i32)
      local.get 0
      local.get 1
      br_if $b
      drop
      i32.const 99
    end)
  (func (export "sel") (param i32 i32 i32) (result i32)
    (select (local.get 0) (local.get 1) (local.get 2)))
  (func (export "selv") (param v128 v128 i32) (result v128)
    (select (local.get 0) (local.get 1) (local.get 2)))
)
