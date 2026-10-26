# Acceptance criteria

The submission is this directory as you leave it. It is accepted when:

1. **Builds offline.** `cargo build --release --offline` at the directory root
   succeeds with a clean `CARGO_HOME`, and produces `target/release/wasm2llvm`.
   Rust standard library only, with no dependencies.
2. **Self-contained.** At translation time the binary reads only its input file,
   runs no other programs, needs no environment variables, and needs no network.
3. **CLI.** `wasm2llvm IN.wasm -o OUT.ll` exits 0 and writes LLVM 23 textual IR
   for every in-scope module. It exits non-zero with an error message for
   malformed, invalid or out-of-scope modules.
4. **ABI.** The IR follows `ABI.md` exactly: symbol names, 16-byte slots, trap
   codes, `w2l_init` semantics, call stack exhaustion, and no signal handlers.
   It compiles with `clang -O0` and with `clang -O2`
   (`-shared -fPIC -Wl,-Bsymbolic … -lm`).
5. **Correct.** The generated code matches WebAssembly semantics. Correctness is
   graded on hidden modules of the same kinds as the payloads (the payloads with
   other inputs and builds, more Rust programs, spec tests and randomly
   generated modules), filtered to the feature scope required by the payloads.
6. **Quality (secondary).** SIMD instructions are lowered to LLVM vector
   operations (for example `add <16 x i8>`, `shufflevector`, `@llvm.smin.v8i16`,
   `@llvm.fptosi.sat.v4i32.v4f32`) rather than per-lane scalar code. Overhead
   such as bounds checks and call-depth accounting is kept reasonable, so that
   compute-heavy programs run fast compared with the wasmtime reference.

Scoring: the primary score is the weighted mean, over hidden test suites, of the
per-script pass rate of execution assertions. Validation (rejecting invalid or
malformed modules), SIMD lowering quality and performance are reported as
secondary metrics.

When you are done, make sure the final state of the project builds. Leave a
short `NOTES.md` describing your design, any known limitations, and test results.
