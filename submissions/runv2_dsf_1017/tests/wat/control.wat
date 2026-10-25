(module
  (memory (export "memory") 1)
  (table 8 funcref)
  (type $mv3 (func (result i32 i32 i32)))
  (type $vi (func (param v128) (result v128)))
  (type $i2v (func (param i32) (result v128)))

  (func $m3 (type $mv3) (result i32 i32 i32) (i32.const 1) (i32.const 2) (i32.const 3))
  (func $vadd (type $vi) (param v128) (result v128)
    (i8x16.add (local.get 0) (i8x16.splat (i32.const 1))))
  (func $isplat (type $i2v) (param i32) (result v128) (i32x4.splat (local.get 0)))

  (elem (i32.const 0) $m3 $vadd $isplat)

  ;; multi-value call results routed straight into a block's results
  (func (export "mv_block") (result i32 i32 i32)
    (block (result i32 i32 i32)
      (call $m3)))

  ;; block with params, changing values
  (func (export "block_params") (param i32 i32) (result i32 i32)
    (local.get 0) (local.get 1)
    (block (param i32 i32) (result i32 i32)
      (i32.add)
      (local.get 0)))

  ;; loop with results: sum of 0..n-1 and count
  (func (export "loop_sum") (param i32) (result i32 i32)
    (local $sum i32) (local $i i32)
    (local.set $sum (i32.const 0))
    (local.set $i (i32.const 0))
    (loop $l (result i32 i32)
      (local.set $sum (i32.add (local.get $sum) (local.get $i)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (local.get $i)
      (local.get 0)
      (i32.lt_u)
      (br_if $l)
      (local.get $sum)
      (local.get $i)))

  ;; br_table with values
  (func (export "brt") (param i32) (result i32)
    (block $b2 (result i32)
      (block $b1 (result i32)
        (block $b0 (result i32)
          (i32.const 10)
          (local.get 0)
          (br_table $b0 $b1 $b2)
          (i32.const -1)
        )
        (i32.add (i32.const 1))
      )
      (i32.add (i32.const 2))
    )
    (i32.add (i32.const 4)))

  ;; br_if carrying a value
  (func (export "brif_val") (param i32) (result i32)
    (block $b (result i32)
      (i32.const 7)
      (local.get 0)
      (br_if $b)
      (drop)
      (i32.const 9))
    (i32.add (i32.const 100)))

  ;; br 0 at function level with multi results
  (func (export "br_func") (param i32) (result i32 i32)
    (i32.const 5)
    (i32.const 6)
    (local.get 0)
    (br_if 0)
    (drop)
    (drop)
    (i32.const 7)
    (i32.const 8)
    (return)
  )

  ;; select on v128
  (func (export "sel_v128") (param i32 i32 i32) (result i32)
    (i32x4.extract_lane 2
      (select
        (i32x4.splat (local.get 0))
        (i32x4.splat (local.get 1))
        (local.get 2))))

  ;; call_indirect with v128 arg and result
  (func (export "ci_v128") (param i32 i32) (result i32)
    (i32x4.extract_lane 0
      (call_indirect (type $vi) (i32x4.splat (local.get 1)) (local.get 0))))

  ;; call_indirect returning v128
  (func (export "ci_i2v") (param i32 i32) (result i32)
    (i32x4.extract_lane 1
      (call_indirect (type $i2v) (local.get 1) (local.get 0))))

  ;; many params
  (func (export "many") (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i32)
    (local.get 0) (local.get 1) (i32.add)
    (local.get 2) (i32.add) (local.get 3) (i32.add)
    (local.get 4) (i32.add) (local.get 5) (i32.add)
    (local.get 6) (i32.add) (local.get 7) (i32.add)
    (local.get 8) (i32.add) (local.get 9) (i32.add)
    (local.get 10) (i32.add) (local.get 11) (i32.add)
    (local.get 12) (i32.add) (local.get 13) (i32.add)
    (local.get 14) (i32.add) (local.get 15) (i32.add))

  ;; deeply nested blocks with branches to various depths
  (func (export "nested") (param i32) (result i32)
    (block $a (result i32)
      (block $b (result i32)
        (block $c (result i32)
          (block $d (result i32)
            (i32.const 1)
            (local.get 0)
            (br_table $d $c $b $a)
            (i32.const 0))
          (i32.add (i32.const 10)))
        (i32.add (i32.const 20)))
      (i32.add (i32.const 40))))

  ;; loop with br_if targeting loop params
  (func (export "loop_brif") (param i32 i32) (result i32)
    (local $n i32)
    (local.set $n (local.get 0))
    (i32.const 0)
    (loop $l (param i32) (result i32)
      (local.get 1)
      (i32.add)
      (local.get $n)
      (i32.const 1)
      (i32.sub)
      (local.tee $n)
      (br_if $l)))

  ;; if with params and results, both arms
  (func (export "if_params") (param i32 i32 i32) (result i32 i32)
    (local.get 0) (local.get 1) (local.get 2)
    (if (param i32 i32) (result i32 i32)
      (then (i32.add) (i32.const 1))
      (else (i32.sub) (i32.const 2))))

  ;; if without else with matching params/results
  (func (export "if_params_noelse") (param i32 i32 i32) (result i32 i32)
    (local.get 0) (local.get 1) (local.get 2)
    (if (param i32 i32) (result i32 i32)
      (then (i32.add) (i32.const 5))))

  ;; loop with two params and two results
  (func (export "loop2") (param i32 i32) (result i32 i32)
    (local $x i32) (local $y i32)
    (local.get 0) (local.get 1)
    (loop $l (param i32 i32) (result i32 i32)
      (local.set $y)
      (local.set $x)
      (local.get $x) (i32.const 1) (i32.add)
      (local.get $y) (i32.const 1) (i32.sub)
      (local.tee $y)
      (local.get $y)
      (br_if $l)))

  ;; br_table targeting a loop with a parameter, and duplicate targets
  (func (export "brt_loop") (param i32) (result i32)
    (local $i i32)
    (local.set $i (local.get 0))
    (block $out (result i32)
      (i32.const 0)
      (loop $l (param i32) (result i32)
        (i32.add (i32.const 1))
        (local.get $i)
        (local.tee $i (i32.sub (local.get $i) (i32.const 1)))
        (br_table $out $l $l $out)
        (i32.const 0))))
)