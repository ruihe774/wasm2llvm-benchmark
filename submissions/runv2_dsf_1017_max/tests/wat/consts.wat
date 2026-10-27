(module
  (func (export "f32c") (param i32) (result i32)
    (i32.reinterpret_f32
      (if (result f32) (local.get 0)
        (then (f32.const nan:0x1))
        (else (f32.const nan:0x7fffff)))))
  (func (export "f64c") (param i32) (result i64)
    (i64.reinterpret_f64
      (if (result f64) (local.get 0)
        (then (f64.const nan:0x1))
        (else (f64.const -nan:0x8000000000001)))))
  (func (export "v128c") (param i32) (result i32)
    (i32x4.extract_lane 1
      (v128.const i32x4 0x7f800001 0xffc00001 0x80000000 0x7fffffff)))
  (func (export "f32sub") (param i32 i32) (result i32)
    (i32.reinterpret_f32 (f32.sub (f32.reinterpret_i32 (local.get 0)) (f32.reinterpret_i32 (local.get 1))))))
