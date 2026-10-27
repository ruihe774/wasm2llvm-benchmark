# Task: a WebAssembly → LLVM IR translator

Implement **`wasm2llvm`**, a translator from WebAssembly binary modules to
textual **LLVM 23** IR, in Rust using only the standard library.

```
wasm2llvm INPUT.wasm -o OUTPUT.ll
```

The payloads in `payloads/` are modules that must run correctly after
translation. Together they define the required WebAssembly feature scope:
support every proposal they use, in full (every instruction of each proposal,
not only the instructions that happen to occur in these binaries), and reject
modules that use anything outside that scope. There is no list of proposals;
discover them from the binaries and the specifications in `docs/`.

`ABI.md` is the contract for the generated code (symbols, slots, traps, memory,
stack). `ACCEPTANCE.md` lists the acceptance criteria. Read both.

## Deliverable

* A Cargo project at the root of this directory that builds the binary
  `wasm2llvm` with `cargo build --release --offline`
  (output: `target/release/wasm2llvm`).
* **Rust, standard library only.** There is no network and no crate registry,
  so the project must have no dependencies other than `std`.
* The translator must be self-contained. At translation time it must not
  execute other programs (no `clang`, `wasm2c`, `wasm-tools`, `llc`, ...) and
  must not read any file other than its input. It is graded inside a jail with
  an empty environment, no network and nothing but its input file visible.
* Exit status 0 means `OUTPUT.ll` was written. On malformed or invalid input
  (or input outside the supported scope), exit with a non-zero status and a
  message on stderr.
* Translation must take reasonable time (well under a minute even for
  multi-megabyte modules).

## Environment

Everything is offline. Some tools on `PATH`:

* LLVM 23.1.4: `clang`, `opt`, `llc`, `llvm-as`, `llvm-dis`, `llvm-link`, `lli`, `wasm-ld`, ...
* `wasm-tools` 1.262 (`parse` wat→wasm, `print`, `dump`, `validate`, `smith`,
  `json-from-wast`, ...)
* wabt 1.0.42: `wat2wasm`, `wasm2wat`, `wasm-objdump`, `wast2json`,
  `spectest-interp`, `wasm-interp`, ... (no `wasm2c`)
* `wasmtime` 50 CLI (a reference WebAssembly engine) and `w2l-ref`, a
  wasmtime-based reference runner; its line protocol is in
  `examples/reference/PROTOCOL.md`. Use them to run the payloads and your own
  modules and to obtain reference results.
* Rust 1.99.1 with targets `x86_64-unknown-linux-gnu`, `wasm32v1-none`,
  `wasm32-unknown-unknown`.

Documentation is in `docs/`: the WebAssembly 2.0 core specification, overviews
of the proposals, the OCaml reference interpreter sources, and the LLVM 23
documentation (including `LangRef.rst`).

There are **no test suites**. Use `wasm-tools smith` to generate modules, write
your own `.wat`/`.wasm` tests with `wasm-tools parse` / `wat2wasm`, compile
your IR with `clang`, and write your own harness for the ABI in `ABI.md`.

Do not modify `payloads/` or `docs/`. Leave a short `NOTES.md` describing your
design, known limitations, and test results.
