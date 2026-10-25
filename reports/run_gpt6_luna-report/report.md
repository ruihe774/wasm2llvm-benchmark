# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 1.0000**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 1.0000
- SIMD lowering quality (secondary): 217/234 = 0.927
- performance (secondary): geomean speedup vs wasmtime = 0.797

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.35 | 1.0000 | 120/120 | 45888/45888 | 2865/2865 |
| spec3 | 0.1 | 1.0000 | 23/23 | 5874/5874 | 1494/1494 |
| payloads | 0.3 | 1.0000 | 45/45 | 314/314 | - |
| smith | 0.25 | 1.0000 | 471/471 | 8181/8181 | - |

## Failures (first per script)


## SIMD ops not passing the lowering check

- v128.any_true: scalarized (lane ops 0, allowed 2)
- i32x4.bitmask: scalarized (lane ops 0, allowed 2)
- i64x2.bitmask: scalarized (lane ops 0, allowed 2)
- i8x16.extract_lane_s: scalarized (lane ops 0, allowed 3)
- i8x16.extract_lane_u: scalarized (lane ops 0, allowed 3)
- i16x8.extract_lane_s: scalarized (lane ops 0, allowed 3)
- i16x8.extract_lane_u: scalarized (lane ops 0, allowed 3)
- i32x4.extract_lane: scalarized (lane ops 0, allowed 3)
- i64x2.extract_lane: scalarized (lane ops 0, allowed 3)
- f32x4.extract_lane: scalarized (lane ops 0, allowed 3)
- f64x2.extract_lane: scalarized (lane ops 0, allowed 3)
- v128.load32_zero: scalarized (lane ops 0, allowed 3)
- v128.load64_zero: scalarized (lane ops 0, allowed 3)
- v128.store8_lane: scalarized (lane ops 0, allowed 3)
- v128.store16_lane: scalarized (lane ops 0, allowed 3)
- v128.store32_lane: scalarized (lane ops 0, allowed 3)
- v128.store64_lane: scalarized (lane ops 0, allowed 3)

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 1.005 | 0.731 | 0.728  |
| crypto-O3 | 0.688 | 0.702 | 1.020  |
| nbody-O3 | 2.223 | 0.755 | 0.340  |
| numerics-O3 | 1.892 | 1.627 | 0.860  |
| raytrace-O3 | 4.794 | 1.439 | 0.300  |
| simd_image-O3 | 1.571 | 0.851 | 0.542  |
| text-O3 | 0.085 | 0.307 | 3.613  |
| vm-O3 | 1.731 | 2.212 | 1.278  |
