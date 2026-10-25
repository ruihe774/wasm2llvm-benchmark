# wasm2llvm verification report

- build: **ok**
- **correctness score (primary): 1.0000**
- validation score (secondary; invalid-rejection rate x valid-module acceptance rate): 1.0000
- SIMD lowering quality (secondary): 233/234 = 0.996
- performance (secondary): geomean speedup vs wasmtime = 0.803

| suite | weight | exec score | scripts fully passing | assertions | validation |
|---|---|---|---|---|---|
| spec | 0.35 | 1.0000 | 120/120 | 45888/45888 | 2865/2865 |
| spec3 | 0.1 | 1.0000 | 23/23 | 5874/5874 | 1494/1494 |
| payloads | 0.3 | 1.0000 | 45/45 | 314/314 | - |
| smith | 0.25 | 1.0000 | 471/471 | 8181/8181 | - |

## Failures (first per script)


## SIMD ops not passing the lowering check

- i32x4.bitmask: scalarized (lane ops 0, allowed 2)

## Performance

| bench | native -O2 (s) | wasmtime (s) | speedup |
|---|---|---|---|
| compress-O3 | 0.999 | 0.731 | 0.732  |
| crypto-O3 | 0.674 | 0.702 | 1.042  |
| nbody-O3 | 2.225 | 0.756 | 0.340  |
| numerics-O3 | 1.891 | 1.629 | 0.861  |
| raytrace-O3 | 4.798 | 1.437 | 0.299  |
| simd_image-O3 | 1.571 | 0.848 | 0.540  |
| text-O3 | 0.080 | 0.300 | 3.762  |
| vm-O3 | 1.738 | 2.208 | 1.270  |
