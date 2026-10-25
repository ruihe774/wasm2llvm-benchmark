You are working in an offline sandbox. Your current directory is your workspace.

Your task: implement `wasm2llvm`, a WebAssembly (wasm32, core spec + bulk-memory,
multi-value, nontrapping-fptoint, sign-ext, simd128) to LLVM 23 IR translator,
written in Rust using only the standard library.

Read `TASK.md` for the full specification (scope, CLI, the ABI between the
generated code and the test harness, and the available tools and documentation)
and `ACCEPTANCE.md` for the acceptance criteria. Specifications are in `docs/`,
tests and the test driver are in `tests/` and `harness/`.

Correctness is the primary goal: never mistranslate. Generated-code quality
(for example real vector code for SIMD, low overhead) is secondary. Work
autonomously until the translator is complete and the visible test suites pass;
then write `NOTES.md` as described in ACCEPTANCE.md.
