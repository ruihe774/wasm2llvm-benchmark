(module
  (type (;0;) (func (param i32 i32) (result i32)))
  (type (;1;) (func (param i64 i64) (result i64)))
  (type (;2;) (func (param f32 f32) (result f32)))
  (type (;3;) (func (param f64 f64) (result f64)))
  (type (;4;) (func (param i32 i64) (result i32 i64)))
  (table (;0;) 1 1 funcref)
  (memory (;0;) 1 4)
  (global (;0;) (mut v128) v128.const i32x4 0x00000000 0x00000000 0x00000000 0x00000000)
  (global (;1;) (mut i64) i64.const 0)
  (global (;2;) (mut i32) i32.const 0)
  (global (;3;) (mut i32) i32.const 0)
  (global (;4;) (mut i64) i64.const 0)
  (global (;5;) (mut i32) i32.const 300)
  (export "i32" (func 0))
  (export "i64" (func 1))
  (export "f32" (func 2))
  (export "f64" (func 3))
  (export "pair" (func 4))
  (export "memory" (memory 0))
  (func (;0;) (type 0) (param i32 i32) (result i32)
    (local i32 v128)
    global.get 5
    i32.eqz
    if ;; label = @1
      unreachable
    end
    global.get 5
    i32.const 1
    i32.sub
    global.set 5
    data.drop 0
    data.drop 0
    nop
    local.get 2
    f64.convert_i32_u
    f32.demote_f64
    local.get 1
    v128.load16x4_u offset=47537 align=4
    i32x4.trunc_sat_f32x4_s
    i16x8.extend_high_i8x16_s
    i16x8.extend_low_i8x16_s
    i64x2.bitmask
    i64.load8_u offset=26408
    i32.const -524288
    v128.load64_zero offset=24952 align=4
    f32.const 0x1.fffcp-135 (;=0.000000000000000000000000000000000000000045916;)
    i32.trunc_f32_u
    i8x16.splat
    i32x4.max_s
    i32x4.neg
    i32x4.extend_high_i16x8_s
    i16x8.extend_low_i8x16_s
    f64x2.neg
    f32x4.sqrt
    local.tee 3
    v128.const i32x4 0x7fc00000 0x7fc00000 0x7fc00000 0x7fc00000
    local.get 3
    local.get 3
    f32x4.eq
    v128.bitselect
    global.get 0
    v128.xor
    global.set 0
    global.get 1
    i64.xor
    global.set 1
    i32.reinterpret_f32
    global.get 2
    i32.xor
    global.set 2
    i32.const -1324650180
  )
  (func (;1;) (type 1) (param i64 i64) (result i64)
    (local i32 f64 f32 v128 v128 v128)
    global.get 5
    i32.eqz
    if ;; label = @1
      unreachable
    end
    global.get 5
    i32.const 1
    i32.sub
    global.set 5
    loop (result i32) ;; label = @1
      global.get 5
      i32.eqz
      if ;; label = @2
        unreachable
      end
      global.get 5
      i32.const 1
      i32.sub
      global.set 5
      data.drop 0
      memory.size
      f64.convert_i32_u
      loop (result f64) ;; label = @2
        global.get 5
        i32.eqz
        if ;; label = @3
          unreachable
        end
        global.get 5
        i32.const 1
        i32.sub
        global.set 5
        f32.const -0x1.e9ba2p+47 (;=-269230300000000;)
        i32.const -641456342
        if (result i32) ;; label = @3
          br 1 (;@2;)
          data.drop 0
          loop ;; label = @4
            global.get 5
            i32.eqz
            if ;; label = @5
              unreachable
            end
            global.get 5
            i32.const 1
            i32.sub
            global.set 5
            loop ;; label = @5
              global.get 5
              i32.eqz
              if ;; label = @6
                unreachable
              end
              global.get 5
              i32.const 1
              i32.sub
              global.set 5
              br 4 (;@1;)
              loop ;; label = @6
                global.get 5
                i32.eqz
                if ;; label = @7
                  unreachable
                end
                global.get 5
                i32.const 1
                i32.sub
                global.set 5
                br 5 (;@1;)
                local.get 1
                f64.const 0x1.p-1068 (;=0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000316;)
                loop (result f64) ;; label = @7
                  global.get 5
                  i32.eqz
                  if ;; label = @8
                    unreachable
                  end
                  global.get 5
                  i32.const 1
                  i32.sub
                  global.set 5
                  loop (result i32) ;; label = @8
                    global.get 5
                    i32.eqz
                    if ;; label = @9
                      unreachable
                    end
                    global.get 5
                    i32.const 1
                    i32.sub
                    global.set 5
                    memory.size
                    i32.load8_s offset=94737
                    f64.convert_i32_s
                    data.drop 0
                    f64.nearest
                    local.tee 3
                    f64.const nan (;=NaN;)
                    local.get 3
                    local.get 3
                    f64.eq
                    select
                    loop (result i32) ;; label = @9
                      global.get 5
                      i32.eqz
                      if ;; label = @10
                        unreachable
                      end
                      global.get 5
                      i32.const 1
                      i32.sub
                      global.set 5
                      v128.const i32x4 0x7a18cd57 0xc9975f08 0x4ad52c14 0x4eb0d59a
                      i16x8.extract_lane_u 0
                      nop
                      memory.grow
                      i64.extend_i32_u
                      i64.clz
                      loop (result i32) ;; label = @10
                        global.get 5
                        i32.eqz
                        if ;; label = @11
                          unreachable
                        end
                        global.get 5
                        i32.const 1
                        i32.sub
                        global.set 5
                        br 3 (;@7;)
                        memory.size
                        br 9 (;@1;)
                        i8x16.splat
                        i16x8.extend_high_i8x16_u
                        memory.size
                        i64.load32_u offset=170896
                        i64.eqz
                        f32.convert_i32_s
                        i64.trunc_f32_u
                        f64.reinterpret_i64
                        f32.demote_f64
                        f32.nearest
                        local.tee 4
                        f32.const nan (;=NaN;)
                        local.get 4
                        local.get 4
                        f32.eq
                        select
                        memory.size
                        i8x16.splat
                        v128.const i32x4 0x2bc83f0b 0x7e4b0fd6 0xcdb39c6c 0x3cb49b45
                        v128.any_true
                        f64.const -nan:0xbffffffffffff (;=NaN;)
                        f64x2.splat
                        f64x2.floor
                        local.tee 5
                        v128.const i32x4 0x00000000 0x7ff80000 0x00000000 0x7ff80000
                        local.get 5
                        local.get 5
                        f64x2.eq
                        v128.bitselect
                        f32x4.trunc
                        local.tee 6
                        v128.const i32x4 0x7fc00000 0x7fc00000 0x7fc00000 0x7fc00000
                        local.get 6
                        local.get 6
                        f32x4.eq
                        v128.bitselect
                        f64x2.sqrt
                        local.tee 7
                        v128.const i32x4 0x00000000 0x7ff80000 0x00000000 0x7ff80000
                        local.get 7
                        local.get 7
                        f64x2.eq
                        v128.bitselect
                        global.get 0
                        v128.xor
                        global.set 0
                        global.get 3
                        i32.xor
                        global.set 3
                        global.get 0
                        v128.xor
                        global.set 0
                        i32.reinterpret_f32
                        global.get 2
                        i32.xor
                        global.set 2
                        global.get 0
                        v128.xor
                        global.set 0
                        i32.const -1851743160
                      end
                      global.get 3
                      i32.xor
                      global.set 3
                      global.get 1
                      i64.xor
                      global.set 1
                      i32.const 1222443917
                    end
                    global.get 3
                    i32.xor
                    global.set 3
                    i64.reinterpret_f64
                    global.get 4
                    i64.xor
                    global.set 4
                    i32.const -88877634
                  end
                  global.get 3
                  i32.xor
                  global.set 3
                  f64.const 0x1.fffffffep-1043 (;=0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000021219957905;)
                end
                i64.reinterpret_f64
                global.get 4
                i64.xor
                global.set 4
                i64.reinterpret_f64
                global.get 4
                i64.xor
                global.set 4
                global.get 1
                i64.xor
                global.set 1
              end
            end
          end
          i32.const -2
        else
          i32.const -242184424
        end
        global.get 3
        i32.xor
        global.set 3
        i32.reinterpret_f32
        global.get 2
        i32.xor
        global.set 2
        f64.const -0x1.44dae8b06eaaep+764 (;=-123130722752603230000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000;)
      end
      i64.reinterpret_f64
      global.get 4
      i64.xor
      global.set 4
      i64.reinterpret_f64
      global.get 4
      i64.xor
      global.set 4
      i32.const 1073741823
    end
    global.get 3
    i32.xor
    global.set 3
    i64.const 597500189649430532
  )
  (func (;2;) (type 2) (param f32 f32) (result f32)
    (local v128 v128 v128 v128 v128)
    global.get 5
    i32.eqz
    if ;; label = @1
      unreachable
    end
    global.get 5
    i32.const 1
    i32.sub
    global.set 5
    i64.const -4852991918847655901
    memory.size
    i32.extend16_s
    loop ;; label = @1
      global.get 5
      i32.eqz
      if ;; label = @2
        unreachable
      end
      global.get 5
      i32.const 1
      i32.sub
      global.set 5
      block ;; label = @2
        memory.size
        br_table 0 (;@2;) 1 (;@1;) 1 (;@1;)
        nop
        f64.const -nan:0xffffffffdffff (;=NaN;)
        v128.const i32x4 0xe253e24b 0xaa1f3e9b 0x5cb73a03 0xea1e2c16
        f64x2.sqrt
        local.tee 2
        v128.const i32x4 0x00000000 0x7ff80000 0x00000000 0x7ff80000
        local.get 2
        local.get 2
        f64x2.eq
        v128.bitselect
        v128.not
        i32x4.neg
        i32x4.extend_low_i16x8_s
        i32x4.all_true
        i64.load32_u offset=15728 align=2
        local.get 1
        memory.size
        i16x8.splat
        i32x4.extend_low_i16x8_s
        i8x16.neg
        f32x4.ceil
        local.tee 3
        v128.const i32x4 0x7fc00000 0x7fc00000 0x7fc00000 0x7fc00000
        local.get 3
        local.get 3
        f32x4.eq
        v128.bitselect
        f64x2.abs
        i16x8.bitmask
        i32.clz
        i32x4.splat
        f32x4.trunc
        local.tee 4
        v128.const i32x4 0x7fc00000 0x7fc00000 0x7fc00000 0x7fc00000
        local.get 4
        local.get 4
        f32x4.eq
        v128.bitselect
        f32x4.demote_f64x2_zero
        f64x2.sqrt
        local.tee 5
        v128.const i32x4 0x00000000 0x7ff80000 0x00000000 0x7ff80000
        local.get 5
        local.get 5
        f64x2.eq
        v128.bitselect
        i32x4.trunc_sat_f64x2_s_zero
        f32x4.abs
        loop (result i64) ;; label = @3
          global.get 5
          i32.eqz
          if ;; label = @4
            unreachable
          end
          global.get 5
          i32.const 1
          i32.sub
          global.set 5
          f64.const 0x1.ffffffp-1050 (;=0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000165780916;)
          memory.size
          v128.load32_zero offset=33601
          i32x4.extadd_pairwise_i16x8_s
          f32x4.ceil
          local.tee 6
          v128.const i32x4 0x7fc00000 0x7fc00000 0x7fc00000 0x7fc00000
          local.get 6
          local.get 6
          f32x4.eq
          v128.bitselect
          i32x4.neg
          i32x4.abs
          br 2 (;@1;)
          i32x4.trunc_sat_f32x4_s
          f32x4.neg
          f32x4.convert_i32x4_u
          f32x4.extract_lane 2
          loop ;; label = @4
            global.get 5
            i32.eqz
            if ;; label = @5
              unreachable
            end
            global.get 5
            i32.const 1
            i32.sub
            global.set 5
            local.get 0
            f32x4.splat
            i8x16.extract_lane_u 10
            f32.convert_i32_s
            i32.trunc_sat_f32_u
            global.get 3
            i32.xor
            global.set 3
          end
          i32.reinterpret_f32
          global.get 2
          i32.xor
          global.set 2
          i64.reinterpret_f64
          global.get 4
          i64.xor
          global.set 4
          i64.const 262142
        end
        global.get 1
        i64.xor
        global.set 1
        global.get 0
        v128.xor
        global.set 0
        i32.reinterpret_f32
        global.get 2
        i32.xor
        global.set 2
        global.get 1
        i64.xor
        global.set 1
        i64.reinterpret_f64
        global.get 4
        i64.xor
        global.set 4
      end
    end
    global.get 3
    i32.xor
    global.set 3
    global.get 1
    i64.xor
    global.set 1
    f32.const 0x1.fe0384p-118 (;=0.0000000000000000000000000000000000059951826;)
  )
  (func (;3;) (type 3) (param f64 f64) (result f64)
    (local v128 v128 v128)
    global.get 5
    i32.eqz
    if ;; label = @1
      unreachable
    end
    global.get 5
    i32.const 1
    i32.sub
    global.set 5
    i64.const 2199672435167218794
    i64.ctz
    i64.clz
    i64.extend32_s
    i64.ctz
    f64.reinterpret_i64
    f32.const 0x1.fp-145 (;=0.000000000000000000000000000000000000000000043;)
    local.get 0
    f64.const 0x1.d806f80980337p-112 (;=0.00000000000000000000000000000000035511381323806977;)
    block ;; label = @1
      i32.const 8192
      v128.load offset=15171
      i16x8.extadd_pairwise_i8x16_u
      i16x8.extadd_pairwise_i8x16_s
      i32x4.bitmask
      local.get 0
      f64.neg
      i64.const 6428294840403087726
      f32.const 0x1p-126 (;=0.000000000000000000000000000000000000011754944;)
      data.drop 0
      f32.abs
      i64.trunc_sat_f32_u
      br 0 (;@1;)
      i64.ctz
      f64.convert_i64_s
      i32.const 262140
      i16x8.splat
      f32x4.trunc
      local.tee 3
      v128.const i32x4 0x7fc00000 0x7fc00000 0x7fc00000 0x7fc00000
      local.get 3
      local.get 3
      f32x4.eq
      v128.bitselect
      f64x2.sqrt
      local.tee 4
      v128.const i32x4 0x00000000 0x7ff80000 0x00000000 0x7ff80000
      local.get 4
      local.get 4
      f64x2.eq
      v128.bitselect
      f64x2.promote_low_f32x4
      br 0 (;@1;)
      i16x8.extend_high_i8x16_s
      global.get 0
      v128.xor
      global.set 0
      i64.reinterpret_f64
      global.get 4
      i64.xor
      global.set 4
      global.get 1
      i64.xor
      global.set 1
      i64.reinterpret_f64
      global.get 4
      i64.xor
      global.set 4
      global.get 3
      i32.xor
      global.set 3
    end
    i64.reinterpret_f64
    global.get 4
    i64.xor
    global.set 4
    i64.reinterpret_f64
    global.get 4
    i64.xor
    global.set 4
    i32.reinterpret_f32
    global.get 2
    i32.xor
    global.set 2
  )
  (func (;4;) (type 4) (param i32 i64) (result i32 i64)
    (local i32 f64 f32 f32 f32 v128 f32)
    global.get 5
    i32.eqz
    if ;; label = @1
      unreachable
    end
    global.get 5
    i32.const 1
    i32.sub
    global.set 5
    f64.const -nan:0xffffffff00000 (;=NaN;)
    block ;; label = @1
      memory.size
      f64.convert_i32_u
      f64.floor
      local.tee 3
      f64.const nan (;=NaN;)
      local.get 3
      local.get 3
      f64.eq
      select
      data.drop 0
      br 0 (;@1;)
      v128.const i32x4 0x30cc5e09 0xf20b07b2 0xb011483e 0x1d8a581a
      f64x2.convert_low_i32x4_u
      i32x4.trunc_sat_f64x2_u_zero
      f64x2.abs
      i32x4.trunc_sat_f64x2_s_zero
      i16x8.bitmask
      f32.reinterpret_i32
      f32.sqrt
      local.tee 4
      f32.const nan (;=NaN;)
      local.get 4
      local.get 4
      f32.eq
      select
      i32.trunc_sat_f32_s
      i32.load8_u offset=44276
      f32.reinterpret_i32
      local.get 2
      f32.convert_i32_s
      f32.add
      local.tee 5
      f32.const nan (;=NaN;)
      local.get 5
      local.get 5
      f32.eq
      select
      f32.sqrt
      local.tee 6
      f32.const nan (;=NaN;)
      local.get 6
      local.get 6
      f32.eq
      select
      memory.size
      f32.convert_i32_s
      br 0 (;@1;)
      i32.const 4194303
      v128.load32_zero offset=14053 align=2
      i32x4.extend_low_i16x8_u
      v128.const i32x4 0xb4250eb6 0x9fc56e4d 0xf778a667 0x9940a70b
      f64x2.lt
      br 0 (;@1;)
      i64x2.extend_low_i32x4_u
      i32x4.trunc_sat_f64x2_s_zero
      f32x4.abs
      i16x8.extend_high_i8x16_s
      i16x8.bitmask
      i32.load8_s offset=25662
      i32.extend8_s
      v128.load32x2_s offset=52698
      f32x4.trunc
      local.tee 7
      v128.const i32x4 0x7fc00000 0x7fc00000 0x7fc00000 0x7fc00000
      local.get 7
      local.get 7
      f32x4.eq
      v128.bitselect
      i32x4.trunc_sat_f64x2_u_zero
      i16x8.bitmask
      select
      nop
      f32.neg
      f32.const 0x1.155e5ap+2 (;=4.333884;)
      i32.reinterpret_f32
      i32.const -530549691
      i32.store16 offset=4523 align=1
      f32.nearest
      local.tee 8
      f32.const nan (;=NaN;)
      local.get 8
      local.get 8
      f32.eq
      select
      i32.reinterpret_f32
      global.get 2
      i32.xor
      global.set 2
      i64.reinterpret_f64
      global.get 4
      i64.xor
      global.set 4
    end
    i64.reinterpret_f64
    global.get 4
    i64.xor
    global.set 4
    i32.const -2108581912
    i64.const 2097151
  )
  (data (;0;) (i32.const 592) "\f1+S")
)
