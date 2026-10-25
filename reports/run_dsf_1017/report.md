# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.9839**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 0.9996
- SIMD lowering quality (secondary): 222/234 = 0.949
- performance (secondary): geomean speedup vs wasmtime = 1.124

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.35 | 1.0000 | 120/120 | 45888/45888 | 2865/2865 |
| spec3 | 0.1 | 1.0000 | 23/23 | 5874/5874 | 1494/1494 |
| payloads | 0.3 | 0.9481 | 42/45 | 307/314 | - |
| smith | 0.25 | 0.9979 | 470/471 | 8139/8181 | - |

## Failures (first per script)

- `payloads:hidden/numerics-O0/numerics-O0.json`: line 2 assert_return [native-O0]: result 0: got 91b60577a94617830000000000000000, expected {'type': 'i64', 'value': '5939156416821397537'}
- `payloads:hidden/numerics-O3/numerics-O3.json`: line 2 assert_return [native-O0]: result 0: got 148cd5003138979e0000000000000000, expected {'type': 'i64', 'value': '5939156416821397537'}
- `payloads:hidden/numerics-Os/numerics-Os.json`: line 2 assert_return [native-O0]: result 0: got 91b60577a94617830000000000000000, expected {'type': 'i64', 'value': '5939156416821397537'}
- `smith:hidden/smith2000205/smith2000205.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: clang failed: /tmp/w2l-verify-neve1qge/native/cache/b0f26de019f4fa854d8a3ec0.ll:614:32: error: use of undefined value '@w2l.elem0'
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
| compress-O3 | 0.799 | 0.734 | 0.918  |
| crypto-O3 | 0.676 | 0.700 | 1.036  |
| nbody-O3 | 0.788 | 0.756 | 0.959  |
| numerics-O3 | - | 1.629 | - wrong result: ok b73665b22b7d4c550000000000000000 |
| raytrace-O3 | 1.503 | 1.438 | 0.956  |
| simd_image-O3 | 1.672 | 0.854 | 0.511  |
| text-O3 | 0.074 | 0.301 | 4.047  |
| vm-O3 | 1.757 | 2.212 | 1.259  |
