# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.9923**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 0.9983
- SIMD lowering quality (secondary): 226/234 = 0.966
- performance (secondary): geomean speedup vs wasmtime = 0.896

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.3 | 0.9887 | 114/120 | 45413/45888 | 2863/2865 |
| spec3 | 0.1 | 0.9565 | 22/23 | 5873/5874 | 1492/1494 |
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
- `spec:unreached-invalid/unreached-invalid.json`: line 676 assert_invalid [native-O0]: module was accepted
- `spec:unreached-valid/unreached-valid.json`: line 63 assert_trap [native-O0]: module at line 49 unavailable: translation failed: translator exit status 1: wasm2llvm: function 0: br_table label types differ

- `spec3:unreached-invalid/unreached-invalid.json`: line 676 assert_invalid [native-O0]: module was accepted
- `spec3:unreached-valid/unreached-valid.json`: line 77 assert_trap [native-O0]: module at line 63 unavailable: translation failed: translator exit status 1: wasm2llvm: function 0: br_table label types differ


## SIMD ops not passing the lowering check

- v128.and: scalarized (lane ops 0, allowed 2)
- v128.andnot: scalarized (lane ops 0, allowed 2)
- v128.or: scalarized (lane ops 0, allowed 2)
- v128.xor: scalarized (lane ops 0, allowed 2)
- v128.bitselect: scalarized (lane ops 0, allowed 2)
- v128.not: scalarized (lane ops 0, allowed 2)
- v128.load: scalarized (lane ops 0, allowed 2)
- v128.store: scalarized (lane ops 0, allowed 2)

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 1.097 | 0.738 | 0.673  |
| crypto-O3 | 0.738 | 0.701 | 0.950  |
| nbody-O3 | 1.276 | 0.756 | 0.592  |
| numerics-O3 | 1.983 | 1.628 | 0.821  |
| raytrace-O3 | 1.615 | 1.438 | 0.891  |
| simd_image-O3 | 1.736 | 0.849 | 0.489  |
| text-O3 | 0.104 | 0.307 | 2.944  |
| vm-O3 | 2.123 | 2.218 | 1.045  |
