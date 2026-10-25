(module
  (table 4 funcref)
  (type $mv (func (result i32 i64)))
  (type $vv (func (param v128 v128) (result v128)))
  (func $mv1 (type $mv) (result i32 i64) (i32.const 7) (i64.const 8))
  (func $mv2 (type $mv) (result i32 i64) (i32.const 9) (i64.const 10))
  (func $vv1 (type $vv) (param v128 v128) (result v128) (i8x16.add (local.get 0) (local.get 1)))
  (elem (i32.const 0) $mv1 $mv2 $vv1)

  (func (export "ci_mv") (param i32) (result i32 i64)
    (call_indirect (type $mv) (local.get 0)))

  (func (export "sel_const") (param i32) (result i32)
    (i32x4.extract_lane 0
      (select
        (v128.const i32x4 0x11111111 0x22222222 0x33333333 0x44444444)
        (v128.const i32x4 0x55555555 0x66666666 0x77777777 0x88888888)
        (local.get 0))))

  (func (export "if_const") (param i32) (result v128)
    (if (result v128) (local.get 0)
      (then (v128.const i32x4 1 2 3 4))
      (else (v128.const i32x4 5 6 7 8))))

  (func (export "phi_const") (param i32) (result i32)
    (block (result i32)
      (block (result i32)
        (local.get 0)
        (br_if 0 (i32.const 0x1234))
        (drop)
        (i32.const 0x5678))))

  (func (export "brt_func") (param i32 i32 i32) (result i32 i32)
    (local.get 0)
    (local.get 1)
    (local.get 2)
    (br_table 0 0 (i32.const 0)))

  (func (export "ci_vv") (param i32 i32 i32) (result i32)
    (i32x4.extract_lane 1
      (call_indirect (type $vv)
        (i32x4.splat (local.get 1))
        (i32x4.splat (local.get 2))
        (local.get 0))))
)
