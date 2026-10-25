# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.9999**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 0.9998
- SIMD lowering quality (secondary): 234/234 = 1.000
- performance (secondary): geomean speedup vs wasmtime = 1.096

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.3 | 0.9997 | 119/120 | 45862/45888 | 2864/2865 |
| spec3 | 0.1 | 1.0000 | 23/23 | 5874/5874 | 1494/1494 |
| payloads | 0.2 | 1.0000 | 45/45 | 314/314 | - |
| real | 0.2 | 1.0000 | 11/11 | 74/74 | - |
| smith | 0.2 | 1.0000 | 471/471 | 8181/8181 | - |

The `real` suite is scored all-or-nothing per script; its assertion totals are diagnostic.

## Failures (first per script)

- `spec:custom/custom.json`: line 77 assert_malformed [native-O0]: module was accepted
- `spec:float_exprs/float_exprs.json`: line 75 assert_return [native-O2]: result 0: got 0000a07f000000000000000000000000, expected {'type': 'f32', 'value': 'nan:arithmetic'}

## SIMD ops not passing the lowering check

- none

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 0.825 | 0.733 | 0.888  |
| crypto-O3 | 0.661 | 0.702 | 1.062  |
| nbody-O3 | 0.789 | 0.758 | 0.961  |
| numerics-O3 | 1.536 | 1.627 | 1.059  |
| raytrace-O3 | 1.550 | 1.436 | 0.927  |
| simd_image-O3 | 1.649 | 0.850 | 0.515  |
| text-O3 | 0.077 | 0.308 | 4.010  |
| vm-O3 | 1.961 | 2.214 | 1.129  |
