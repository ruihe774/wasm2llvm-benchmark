# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 0.9995**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 0.9996
- SIMD lowering quality (secondary): 234/234 = 1.000
- performance (secondary): geomean speedup vs wasmtime = 1.124

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.35 | 1.0000 | 120/120 | 45888/45888 | 2865/2865 |
| spec3 | 0.1 | 1.0000 | 23/23 | 5874/5874 | 1494/1494 |
| payloads | 0.3 | 1.0000 | 45/45 | 314/314 | - |
| smith | 0.25 | 0.9979 | 470/471 | 8139/8181 | - |

## Failures (first per script)

- `smith:hidden/smith2000205/smith2000205.json`: line 2 assert_return [native-O0]: module at line 1 unavailable: clang failed: /tmp/w2l-verify-xqy1pain/native/cache/b0f26de019f4fa854d8a3ec0.ll:614:32: error: use of undefined value '@w2l.elem0'
  614 |   %t36 = getelementptr i8, ptr 

## SIMD ops not passing the lowering check

- none

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 0.801 | 0.732 | 0.915  |
| crypto-O3 | 0.675 | 0.702 | 1.040  |
| nbody-O3 | 0.787 | 0.756 | 0.960  |
| numerics-O3 | 1.456 | 1.628 | 1.118  |
| raytrace-O3 | 1.504 | 1.436 | 0.955  |
| simd_image-O3 | 1.653 | 0.852 | 0.516  |
| text-O3 | 0.075 | 0.300 | 4.003  |
| vm-O3 | 1.756 | 2.220 | 1.264  |
