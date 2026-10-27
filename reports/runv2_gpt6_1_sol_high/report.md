# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.9991**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 1.0000
- SIMD lowering quality (secondary): 234/234 = 1.000
- performance (secondary): geomean speedup vs wasmtime = 1.144

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.3 | 0.9971 | 115/120 | 45414/45888 | 2865/2865 |
| spec3 | 0.1 | 1.0000 | 23/23 | 5874/5874 | 1494/1494 |
| payloads | 0.2 | 1.0000 | 45/45 | 314/314 | - |
| real | 0.2 | 1.0000 | 11/11 | 74/74 | - |
| smith | 0.2 | 1.0000 | 471/471 | 8181/8181 | - |

The `real` suite is scored all-or-nothing per script; its assertion totals are diagnostic.

## Failures (first per script)

- `spec:f32/f32.json`: line 1652 assert_return [native-O0]: result 0: got 0000a0ff000000000000000000000000, expected {'type': 'f32', 'value': 'nan:arithmetic'}
- `spec:f64/f64.json`: line 1652 assert_return [native-O0]: result 0: got 000000000000f4ff0000000000000000, expected {'type': 'f64', 'value': 'nan:arithmetic'}
- `spec:float_exprs/float_exprs.json`: line 75 assert_return [native-O2]: result 0: got 0000a07f000000000000000000000000, expected {'type': 'f32', 'value': 'nan:arithmetic'}
- `spec:simd_f32x4/simd_f32x4.json`: line 1060 assert_return [native-O0]: result 0: got 0000a07f0000a07f0000a07f0000a07f, expected {'type': 'v128', 'lane_type': 'f32', 'value': ['nan:arithmetic', 'nan:arithmetic', 'nan:arithmetic', 'nan:arithmetic']}
- `spec:simd_f64x2/simd_f64x2.json`: line 1043 assert_return [native-O0]: result 0: got 000000000000f47f000000000000f47f, expected {'type': 'v128', 'lane_type': 'f64', 'value': ['nan:arithmetic', 'nan:arithmetic']}

## SIMD ops not passing the lowering check

- none

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 0.750 | 0.734 | 0.978  |
| crypto-O3 | 0.675 | 0.703 | 1.042  |
| nbody-O3 | 0.780 | 0.755 | 0.969  |
| numerics-O3 | 1.521 | 1.627 | 1.070  |
| raytrace-O3 | 1.485 | 1.434 | 0.966  |
| simd_image-O3 | 1.549 | 0.889 | 0.574  |
| text-O3 | 0.073 | 0.307 | 4.226  |
| vm-O3 | 1.866 | 2.212 | 1.185  |
