# Local tests

All test programs are standard-library Python or C; external tools are used only
by tests, never by the translator.

```sh
cargo build --release --offline
cargo test --offline
python3 -I tests/test.py --no-build

mkdir -p tests/out
clang -std=c11 -O2 -rdynamic tests/harness.c -ldl -o tests/out/harness
python3 -I tests/scalar_matrix.py
python3 -I tests/simd_matrix.py tests/out/harness
python3 -I tests/recursion.py
python3 -I tests/state.py
python3 -I tests/smith.py --count 100
python3 -I tests/stress.py
```

`test.py` builds its own native driver and uses a temporary directory. Use
`--only control`, `--only bulk`, `--opt 0`, `--opt 2`, `--quick`, or `--keep`
to select work and preserve generated artifacts. The complete default run
includes all four payloads.

The other scripts preserve artifacts in ignored `tests/out/`. `smith.py` accepts
`--start N` and `--count N`; the recorded 400-module run used seeds 0 through 399.
Smith's unsupported features are explicitly disabled, and each generated module
is checked against the reference engine's scope before translation.

The C driver accepts the reference line protocol, with `.so` paths instead of
`.wasm` paths for `load`. It implements the provided imports and explicit trap
recovery. It does not install a native stack-overflow handler; recursion tests
use small frames and the translator's explicit depth trap.

Comparisons ignore unused ABI-slot bytes and allow NaN variation only for
arithmetic floating-point results. Integer results, bitwise operations, encoded
packets, and memory byte comparisons remain exact.
