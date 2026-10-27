(module
  (memory (export "memory") 1)

  ;; ---------- integer shift/rotate edge cases ----------
  (func (export "i32_shl") (param i32 i32) (result i32) (i32.shl (local.get 0) (local.get 1)))
  (func (export "i32_shr_s") (param i32 i32) (result i32) (i32.shr_s (local.get 0) (local.get 1)))
  (func (export "i32_shr_u") (param i32 i32) (result i32) (i32.shr_u (local.get 0) (local.get 1)))
  (func (export "i32_rotl") (param i32 i32) (result i32) (i32.rotl (local.get 0) (local.get 1)))
  (func (export "i32_rotr") (param i32 i32) (result i32) (i32.rotr (local.get 0) (local.get 1)))
  (func (export "i64_shl") (param i64 i64) (result i64) (i64.shl (local.get 0) (local.get 1)))
  (func (export "i64_shr_s") (param i64 i64) (result i64) (i64.shr_s (local.get 0) (local.get 1)))
  (func (export "i64_shr_u") (param i64 i64) (result i64) (i64.shr_u (local.get 0) (local.get 1)))
  (func (export "i64_rotl") (param i64 i64) (result i64) (i64.rotl (local.get 0) (local.get 1)))
  (func (export "i64_rotr") (param i64 i64) (result i64) (i64.rotr (local.get 0) (local.get 1)))
  (func (export "i32_clz") (param i32) (result i32) (i32.clz (local.get 0)))
  (func (export "i32_ctz") (param i32) (result i32) (i32.ctz (local.get 0)))
  (func (export "i32_popcnt") (param i32) (result i32) (i32.popcnt (local.get 0)))
  (func (export "i64_clz") (param i64) (result i64) (i64.clz (local.get 0)))
  (func (export "i64_ctz") (param i64) (result i64) (i64.ctz (local.get 0)))
  (func (export "i64_popcnt") (param i64) (result i64) (i64.popcnt (local.get 0)))
  (func (export "i32_ext8") (param i32) (result i32) (i32.extend8_s (local.get 0)))
  (func (export "i32_ext16") (param i32) (result i32) (i32.extend16_s (local.get 0)))
  (func (export "i64_ext8") (param i64) (result i64) (i64.extend8_s (local.get 0)))
  (func (export "i64_ext16") (param i64) (result i64) (i64.extend16_s (local.get 0)))
  (func (export "i64_ext32") (param i64) (result i64) (i64.extend32_s (local.get 0)))
  (func (export "i32_add") (param i32 i32) (result i32) (i32.add (local.get 0) (local.get 1)))
  (func (export "i32_mul") (param i32 i32) (result i32) (i32.mul (local.get 0) (local.get 1)))
  (func (export "i64_mul") (param i64 i64) (result i64) (i64.mul (local.get 0) (local.get 1)))

  ;; ---------- conversion edges (trapping) ----------
  (func (export "i32_trunc_f32_s") (param f32) (result i32) (i32.trunc_f32_s (local.get 0)))
  (func (export "i32_trunc_f64_s") (param f64) (result i32) (i32.trunc_f64_s (local.get 0)))
  (func (export "i32_trunc_f32_u") (param f32) (result i32) (i32.trunc_f32_u (local.get 0)))
  (func (export "i32_trunc_f64_u") (param f64) (result i32) (i32.trunc_f64_u (local.get 0)))
  (func (export "i64_trunc_f32_s") (param f32) (result i64) (i64.trunc_f32_s (local.get 0)))
  (func (export "i64_trunc_f64_s") (param f64) (result i64) (i64.trunc_f64_s (local.get 0)))
  (func (export "i64_trunc_f32_u") (param f32) (result i64) (i64.trunc_f32_u (local.get 0)))
  (func (export "i64_trunc_f64_u") (param f64) (result i64) (i64.trunc_f64_u (local.get 0)))

  (func (export "i32_sat_f32_s") (param f32) (result i32) (i32.trunc_sat_f32_s (local.get 0)))
  (func (export "i32_sat_f64_s") (param f64) (result i32) (i32.trunc_sat_f64_s (local.get 0)))
  (func (export "i32_sat_f32_u") (param f32) (result i32) (i32.trunc_sat_f32_u (local.get 0)))
  (func (export "i32_sat_f64_u") (param f64) (result i32) (i32.trunc_sat_f64_u (local.get 0)))
  (func (export "i64_sat_f32_s") (param f32) (result i64) (i64.trunc_sat_f32_s (local.get 0)))
  (func (export "i64_sat_f64_s") (param f64) (result i64) (i64.trunc_sat_f64_s (local.get 0)))
  (func (export "i64_sat_f32_u") (param f32) (result i64) (i64.trunc_sat_f32_u (local.get 0)))
  (func (export "i64_sat_f64_u") (param f64) (result i64) (i64.trunc_sat_f64_u (local.get 0)))

  ;; ---------- float -> bits ----------
  (func (export "f32_add") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.add (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
  (func (export "f32_mul") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.mul (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
  (func (export "f32_min") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.min (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
  (func (export "f32_max") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.max (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
  (func (export "f32_copysign") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.copysign (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
  (func (export "f32_abs") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.abs (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_neg") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.neg (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_sqrt") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.sqrt (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_ceil") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.ceil (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_floor") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.floor (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_trunc") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.trunc (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_nearest") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.nearest (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_demote") (param i64) (result i32)
    (i32.reinterpret_f32 (f32.demote_f64 (f64.reinterpret_i64 (local.get 0)))))
  (func (export "f64_promote") (param i32) (result i64)
    (i64.reinterpret_f64 (f64.promote_f32 (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f64_add") (param i64 i64) (result i64)
    (i64.reinterpret_f64 (f64.add (f64.reinterpret_i64 (local.get 0)) (f64.reinterpret_i64 (local.get 1)))))
  (func (export "f64_min") (param i64 i64) (result i64)
    (i64.reinterpret_f64 (f64.min (f64.reinterpret_i64 (local.get 0)) (f64.reinterpret_i64 (local.get 1)))))
  (func (export "f64_max") (param i64 i64) (result i64)
    (i64.reinterpret_f64 (f64.max (f64.reinterpret_i64 (local.get 0)) (f64.reinterpret_i64 (local.get 1)))))
  (func (export "f64_sqrt") (param i64) (result i64)
    (i64.reinterpret_f64 (f64.sqrt (f64.reinterpret_i64 (local.get 0)))))
  (func (export "f64_nearest") (param i64) (result i64)
    (i64.reinterpret_f64 (f64.nearest (f64.reinterpret_i64 (local.get 0)))))
  (func (export "f32_convert_s") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.convert_i32_s (local.get 0))))
  (func (export "f32_convert_u") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.convert_i32_u (local.get 0))))
  (func (export "f32_convert_l_s") (param i64) (result i32)
    (i32.reinterpret_f32 (f32.convert_i64_s (local.get 0))))
  (func (export "f32_convert_l_u") (param i64) (result i32)
    (i32.reinterpret_f32 (f32.convert_i64_u (local.get 0))))
  (func (export "f64_convert_l_u") (param i64) (result i64)
    (i64.reinterpret_f64 (f64.convert_i64_u (local.get 0))))
  (func (export "f64_convert_l_s") (param i64) (result i64)
    (i64.reinterpret_f64 (f64.convert_i64_s (local.get 0))))

  ;; ---------- NaN behaviour predicates ----------
  ;; returns bit0: result is NaN, bit1: quiet bit set
  (func $naninfo (param i32) (result i32) (i32.or (i32.and (local.get 0) (i32.const 0x00400000)) (i32.const 0)))
  (func (export "min_naninfo") (param i32 i32) (result i32)
    (local $r i32)
    (local.set $r (i32.reinterpret_f32
      (f32.min (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "max_naninfo") (param i32 i32) (result i32)
    (local $r i32)
    (local.set $r (i32.reinterpret_f32
      (f32.max (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "add_naninfo") (param i32 i32) (result i32)
    (local $r i32)
    (local.set $r (i32.reinterpret_f32
      (f32.add (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "sqrt_naninfo") (param i32) (result i32)
    (local $r i32)
    (local.set $r (i32.reinterpret_f32 (f32.sqrt (f32.reinterpret_i32 (local.get 0)))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "demote_naninfo") (param i64) (result i32)
    (local $r i32)
    (local.set $r (i32.reinterpret_f32 (f32.demote_f64 (f64.reinterpret_i64 (local.get 0)))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "promote_naninfo") (param i32) (result i64)
    (local $r i64)
    (local.set $r (i64.reinterpret_f64 (f64.promote_f32 (f32.reinterpret_i32 (local.get 0)))))
    (i64.or
      (i64.extend_i32_u (i64.ne (i64.and (local.get $r) (i64.const 0x7ff0000000000000)) (i64.const 0x7ff0000000000000)))
      (i64.and
        (i64.shr_u (i64.and (local.get $r) (i64.const 0x0008000000000000)) (i64.const 51))
        (i64.const 1))))
  ;; pmin/pmax NaN selection: pmin(a,b) = b if b<a else a
  (func (export "pmin_bits") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.min (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
  (func (export "fmin_sel") (param i32 i32) (result i32)
    (local $a f32) (local $b f32)
    (local.set $a (f32.reinterpret_i32 (local.get 0)))
    (local.set $b (f32.reinterpret_i32 (local.get 1)))
    (i32.reinterpret_f32 (select (f32.min (local.get $a) (local.get $b)) (local.get $b) (f32.lt (local.get $b) (local.get $a)))))

  ;; ---------- comparison sanity ----------
  (func (export "f32_eq") (param i32 i32) (result i32)
    (f32.eq (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1))))
  (func (export "f32_ne") (param i32 i32) (result i32)
    (f32.ne (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1))))
  (func (export "f32_lt") (param i32 i32) (result i32)
    (f32.lt (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1))))
  (func (export "f64_le") (param i64 i64) (result i32)
    (f64.le (f64.reinterpret_i64 (local.get 0)) (f64.reinterpret_i64 (local.get 1))))

  ;; ---------- SIMD edge cases ----------
  (func (export "v_min_naninfo") (param i32 i32) (result i32)
    (i32x4.extract_lane 0
      (i32x4.min_s
        (i32x4.splat (local.get 0))
        (i32x4.splat (local.get 1)))))
  (func (export "v_f32min_naninfo") (param i32 i32) (result i32)
    (local $v v128) (local $r i32)
    (local.set $v (f32x4.min
      (f32x4.splat (f32.reinterpret_i32 (local.get 0)))
      (f32x4.splat (f32.reinterpret_i32 (local.get 1)))))
    (local.set $r (i32.reinterpret_f32 (f32x4.extract_lane 0 (local.get $v))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "v_pmin_bits") (param i32 i32) (result i32)
    (i32.reinterpret_f32
      (f32x4.extract_lane 1
        (f32x4.pmin
          (f32x4.splat (f32.reinterpret_i32 (local.get 0)))
          (f32x4.splat (f32.reinterpret_i32 (local.get 1)))))))
  (func (export "v_pmax_bits") (param i32 i32) (result i32)
    (i32.reinterpret_f32
      (f32x4.extract_lane 2
        (f32x4.pmax
          (f32x4.splat (f32.reinterpret_i32 (local.get 0)))
          (f32x4.splat (f32.reinterpret_i32 (local.get 1)))))))
  (func (export "v_narrow_s") (param i32) (result i32)
    (local $a v128)
    (local.set $a (i32x4.splat (local.get 0)))
    (i16x8.extract_lane_u 3 (i16x8.narrow_i32x4_s (local.get $a) (local.get $a))))
  (func (export "v_narrow_u") (param i32) (result i32)
    (local $a v128)
    (local.set $a (i32x4.splat (local.get 0)))
    (i16x8.extract_lane_u 3 (i16x8.narrow_i32x4_u (local.get $a) (local.get $a))))
  (func (export "v_q15") (param i32 i32) (result i32)
    (i16x8.extract_lane_s 0
      (i16x8.q15mulr_sat_s
        (i16x8.splat (local.get 0))
        (i16x8.splat (local.get 1)))))
  (func (export "v_avgr") (param i32 i32) (result i32)
    (i8x16.extract_lane_u 0
      (i8x16.avgr_u
        (i8x16.splat (local.get 0))
        (i8x16.splat (local.get 1)))))
  (func (export "v_dot") (param i32 i32) (result i32)
    (i32x4.extract_lane 2
      (i32x4.dot_i16x8_s
        (i16x8.splat (local.get 0))
        (i16x8.splat (local.get 1)))))
  (func (export "v_bitmask") (param i32) (result i32)
    (i8x16.bitmask
      (i8x16.replace_lane 0 (i8x16.splat (local.get 0)) (i32.const -1))))
  (func (export "v_alltrue") (param i32) (result i32)
    (i16x8.all_true (i16x8.splat (local.get 0))))
  (func (export "v_swizzle") (param i32 i32) (result i32)
    (i8x16.extract_lane_u 5
      (i8x16.swizzle
        (i8x16.splat (local.get 0))
        (i8x16.splat (local.get 1)))))
  (func (export "v_shuffle") (param i32) (result i32)
    (i8x16.extract_lane_u 9
      (i8x16.shuffle 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15
        (i8x16.splat (local.get 0))
        (i8x16.splat (i32.const 0x55)))))
  (func (export "v_shl") (param i32 i32) (result i32)
    (i8x16.extract_lane_u 2
      (i8x16.shl (i8x16.splat (local.get 0)) (local.get 1))))
  (func (export "v_shr_s") (param i32 i32) (result i32)
    (i8x16.extract_lane_s 2
      (i8x16.shr_s (i8x16.splat (local.get 0)) (local.get 1))))
  (func (export "v_extmul") (param i32 i32) (result i32)
    (i16x8.extract_lane_s 4
      (i16x8.extmul_high_i8x16_s
        (i8x16.splat (local.get 0))
        (i8x16.splat (local.get 1)))))
  (func (export "v_extadd") (param i32) (result i32)
    (i16x8.extract_lane_u 6
      (i16x8.extadd_pairwise_i8x16_u (i8x16.splat (local.get 0)))))
  (func (export "v_sat_trunc") (param i32) (result i32)
    (i32x4.extract_lane 3
      (i32x4.trunc_sat_f32x4_s (f32x4.splat (f32.reinterpret_i32 (local.get 0))))))
  (func (export "v_sat_trunc_u") (param i32) (result i32)
    (i32x4.extract_lane 3
      (i32x4.trunc_sat_f32x4_u (f32x4.splat (f32.reinterpret_i32 (local.get 0))))))
  (func (export "v_convert") (param i32) (result i32)
    (i32.reinterpret_f32
      (f32x4.extract_lane 3 (f32x4.convert_i32x4_s (i32x4.splat (local.get 0))))))
  (func (export "v_load_lane") (param i32) (result i32)
    (i32x4.extract_lane 1
      (v128.load32_lane 0
        (local.get 0)
        (v128.const i32x4 0 0 0 0))))
  (func (export "v_anytrue") (param i64) (result i32)
    (v128.any_true (i64x2.splat (local.get 0))))

  (func (export "f64_min_naninfo") (param i64 i64) (result i32)
    (local $r i64)
    (local.set $r (i64.reinterpret_f64
      (f64.min (f64.reinterpret_i64 (local.get 0)) (f64.reinterpret_i64 (local.get 1)))))
    (i32.or
      (i32.wrap_i64 (i64.extend_i32_u (i64.ne (i64.and (local.get $r) (i64.const 0x7ff0000000000000)) (i64.const 0x7ff0000000000000))))
      (i32.wrap_i64 (i64.shr_u (i64.and (local.get $r) (i64.const 0x0008000000000000)) (i64.const 51)))))
  (func (export "f64_max_naninfo") (param i64 i64) (result i32)
    (local $r i64)
    (local.set $r (i64.reinterpret_f64
      (f64.max (f64.reinterpret_i64 (local.get 0)) (f64.reinterpret_i64 (local.get 1)))))
    (i32.or
      (i32.wrap_i64 (i64.extend_i32_u (i64.ne (i64.and (local.get $r) (i64.const 0x7ff0000000000000)) (i64.const 0x7ff0000000000000))))
      (i32.wrap_i64 (i64.shr_u (i64.and (local.get $r) (i64.const 0x0008000000000000)) (i64.const 51)))))
  (func (export "v_f32max_naninfo") (param i32 i32) (result i32)
    (local $v v128) (local $r i32)
    (local.set $v (f32x4.max
      (f32x4.splat (f32.reinterpret_i32 (local.get 0)))
      (f32x4.splat (f32.reinterpret_i32 (local.get 1)))))
    (local.set $r (i32.reinterpret_f32 (f32x4.extract_lane 3 (local.get $v))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "v_f32add_naninfo") (param i32 i32) (result i32)
    (local $v v128) (local $r i32)
    (local.set $v (f32x4.add
      (f32x4.splat (f32.reinterpret_i32 (local.get 0)))
      (f32x4.splat (f32.reinterpret_i32 (local.get 1)))))
    (local.set $r (i32.reinterpret_f32 (f32x4.extract_lane 2 (local.get $v))))
    (i32.or
      (i32.and (i32.ne (i32.and (local.get $r) (i32.const 0x7f800000)) (i32.const 0x7f800000)) (i32.const 1))
      (i32.and (local.get $r) (i32.const 0x00400000))))
  (func (export "f32_abs_nan") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.abs (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_neg_nan") (param i32) (result i32)
    (i32.reinterpret_f32 (f32.neg (f32.reinterpret_i32 (local.get 0)))))
  (func (export "f32_copysign_nan") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.copysign (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1)))))
)
