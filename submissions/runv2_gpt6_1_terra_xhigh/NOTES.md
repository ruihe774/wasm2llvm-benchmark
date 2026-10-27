# Notes

Build with `cargo build --release --offline`, then run
`target/release/wasm2llvm INPUT.wasm -o OUTPUT.ll`.

`wasm2llvm` decodes and validates the binary, then lowers structured control
flow to LLVM blocks with fixed 128-bit operand slots. Numeric and fixed-width
SIMD operations use internal LLVM helpers; SIMD helpers use vector operations.
Linear-memory and table accesses are explicitly bounds checked. Memory normally
uses a zero-filled, read/write anonymous mapping reserved up to its maximum;
a `calloc`/`realloc` fallback is used if the mapping fails. Neither path relies
on signals or guard pages. ABI entry points reset the call-depth counter so a
trap does not poison subsequent calls.

The discovered scope is core wasm32, multi-value, sign extension, saturating
float-to-int conversion, bulk memory, and fixed-width SIMD. Reference types,
relaxed SIMD, multiple memories/tables, exceptions and tail calls are rejected.

Known limitations:
- Only function imports are implemented. Valid modules importing a memory,
  table, or global are currently rejected, even though these are core wasm;
  `ABI.md` does not specify host symbols for those import kinds.
- A clean-`CARGO_HOME` build and empty-environment invocation were not verified:
  the sandbox permission layer denied the combined verification command.

Tests (offline tools required):
- `cargo build --release --offline` succeeds with the session's Cargo home.
- `python3 -I tests/run.py`: multi-value, control/indirect calls, bulk operations,
  numerical edges, every SIMD memory form, 4 GiB growth, 10,000-frame recursion,
  stack exhaustion and post-trap recovery, at both `-O0` and `-O2`.
- `python3 -I tests/payloads.py`: QOA encode/decode plus CELT and SILK encoding
  compared with `w2l-ref` at both optimization levels.
- `python3 tests/smith.py 500`: 500 in-scope terminating generated modules
  compared against `w2l-ref` at both optimization levels.
- `python3 tests/validation.py`, `python3 tests/reserved.py`, and
  `python3 tests/large_locals.py`: malformed/out-of-scope rejection, reserved
  immediates, and sparse run-length-encoded local declarations.
