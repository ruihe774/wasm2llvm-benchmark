# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.9931**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 0.9969
- SIMD lowering quality (secondary): 234/234 = 1.000
- performance (secondary): geomean speedup vs wasmtime = 1.089

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.3 | 0.9914 | 118/120 | 45861/45888 | 2856/2865 |
| spec3 | 0.1 | 0.9565 | 22/23 | 5873/5874 | 1493/1494 |
| payloads | 0.2 | 1.0000 | 45/45 | 314/314 | - |
| real | 0.2 | 1.0000 | 11/11 | 74/74 | - |
| smith | 0.2 | 1.0000 | 471/471 | 8181/8181 | - |

The `real` suite is scored all-or-nothing per script; its assertion totals are diagnostic.

## Failures (first per script)

- `spec:binary/binary.json`: line 146 assert_malformed [native-O0]: module was accepted
- `spec:float_exprs/float_exprs.json`: line 75 assert_return [native-O2]: result 0: got 0000a07f000000000000000000000000, expected {'type': 'f32', 'value': 'nan:arithmetic'}
- `spec:unreached-invalid/unreached-invalid.json`: line 687 assert_invalid [native-O0]: module was accepted
- `spec:unreached-valid/unreached-valid.json`: line 63 assert_trap [native-O0]: module at line 49 unavailable: translation failed: translator exit status 1: wasm2llvm: invalid module: func 0: instr 4 BrTable([0, 1], 1): type mismatch: br_table target types
   [0] Block(Val(F64)) 
- `spec3:unreached-invalid/unreached-invalid.json`: line 687 assert_invalid [native-O0]: module was accepted
- `spec3:unreached-valid/unreached-valid.json`: line 77 assert_trap [native-O0]: module at line 63 unavailable: translation failed: translator exit status 1: wasm2llvm: invalid module: func 0: instr 4 BrTable([0, 1], 1): type mismatch: br_table target types
   [0] Block(Val(F64)) 

## SIMD ops not passing the lowering check

- none

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 0.791 | 0.735 | 0.928  |
| crypto-O3 | 0.656 | 0.701 | 1.068  |
| nbody-O3 | 0.781 | 0.755 | 0.967  |
| numerics-O3 | 1.548 | 1.627 | 1.051  |
| raytrace-O3 | 1.764 | 1.437 | 0.815  |
| simd_image-O3 | 1.644 | 0.849 | 0.516  |
| text-O3 | 0.080 | 0.307 | 3.821  |
| vm-O3 | 1.809 | 2.208 | 1.220  |
