# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.9995**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 0.9996
- SIMD lowering quality (secondary): 222/234 = 0.949
- performance (secondary): geomean speedup vs wasmtime = 1.125

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.35 | 1.0000 | 120/120 | 45888/45888 | 2865/2865 |
| spec3 | 0.1 | 1.0000 | 23/23 | 5874/5874 | 1494/1494 |
| payloads | 0.3 | 1.0000 | 45/45 | 314/314 | - |
| smith | 0.25 | 0.9979 | 470/471 | 8139/8181 | - |

## Failures (first per script)

- `smith:hidden/smith2000205/smith2000205.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: clang failed: /tmp/w2l-verify-1_jv4z_g/native/cache/b0f26de019f4fa854d8a3ec0.ll:614:32: error: use of undefined value '@w2l.elem0'
  614 |   %t36 = getelementptr i8, ptr 

## SIMD ops not passing the lowering check

- i8x16.extract_lane_s: scalarized (lane ops 0, allowed 3)
- i8x16.extract_lane_u: scalarized (lane ops 0, allowed 3)
- i16x8.extract_lane_s: scalarized (lane ops 0, allowed 3)
- i16x8.extract_lane_u: scalarized (lane ops 0, allowed 3)
- i32x4.extract_lane: scalarized (lane ops 0, allowed 3)
- i64x2.extract_lane: scalarized (lane ops 0, allowed 3)
- f32x4.extract_lane: scalarized (lane ops 0, allowed 3)
- f64x2.extract_lane: scalarized (lane ops 0, allowed 3)
- v128.store8_lane: scalarized (lane ops 0, allowed 3)
- v128.store16_lane: scalarized (lane ops 0, allowed 3)
- v128.store32_lane: scalarized (lane ops 0, allowed 3)
- v128.store64_lane: scalarized (lane ops 0, allowed 3)

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 0.799 | 0.731 | 0.916  |
| crypto-O3 | 0.685 | 0.702 | 1.025  |
| nbody-O3 | 0.787 | 0.755 | 0.959  |
| numerics-O3 | 1.453 | 1.627 | 1.120  |
| raytrace-O3 | 1.503 | 1.436 | 0.956  |
| simd_image-O3 | 1.647 | 0.849 | 0.515  |
| text-O3 | 0.073 | 0.302 | 4.107  |
| vm-O3 | 1.761 | 2.211 | 1.256  |
