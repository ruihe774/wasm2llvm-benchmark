You are working in an offline sandbox. Your current directory is your workspace.

Your task: implement `wasm2llvm`, a WebAssembly (wasm32) to LLVM 23 IR
translator, written in Rust using only the standard library. The payloads in
`payloads/` are modules that must run correctly after translation; together they
define the required WebAssembly feature scope. Support every proposal they use,
in full, and reject modules outside that scope.

Read `TASK.md` for the task, `ABI.md` for the generated-code contract, and
`ACCEPTANCE.md` for the acceptance criteria. The specifications and LLVM
documentation are in `docs/`.

There are no test suites. Use the tools on `PATH` (LLVM, wasm-tools including `smith`, wabt, and the
`wasmtime`/`w2l-ref` reference engine; the runner protocol is in
`examples/reference/PROTOCOL.md`) to inspect and run the payloads, generate your
own modules, and write your own tests and test harness. Work autonomously until the translator is
complete; then write `NOTES.md` as described in `ACCEPTANCE.md`.
