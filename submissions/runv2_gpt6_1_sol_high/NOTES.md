# wasm2llvm implementation notes

## Scope and design

The payloads require wasm32 MVP plus **multi-value, sign-extension operations,
nontrapping float-to-integer conversions, bulk memory, and SIMD 1.0**. All
instructions in those proposals are implemented, including all 236 SIMD
opcodes, bulk table operations, passive/declarative segments, start functions,
and multi-value block/loop parameters. Reference-type instructions, relaxed
SIMD, threads, exceptions, tail calls, memory64, multi-memory, GC, extended
constant expressions, and other proposals are rejected.

The translator is Rust/std only, with no Cargo dependencies. It reads its input,
validates it, generates textual LLVM, and writes the requested output; it has no
subprocess, tool-discovery, auxiliary-file, or network code.

* `parse.rs` implements bounded binary readers, exact LEB encodings and section
  structure; `validate.rs` checks types, control-stack polymorphism, indices,
  limits, initializers, memory alignments, and SIMD lanes, including dead code.
* `ir.rs` uses native typed functions and aggregate multi-results internally.
  Slot-ABI wrappers handle imports/exports. Operands are SSA values; entry-block
  allocas implement locals and structured-control merges, and are promoted by
  LLVM at optimization. Indirect calls use structural signature IDs.
* `simd.rs` lowers to vector arithmetic, comparisons, shuffles, extensions,
  narrowing, reductions, and LLVM intrinsics. Dynamic byte swizzle uses a
  portable vector selection network, without target-specific CPU requirements.
* Bounds and trap conditions are explicit. Signed division/remainder overflow,
  shift counts, floating conversion ranges/NaNs, and SIMD saturation are handled
  before potentially undefined LLVM operations. No fast-math/FMA flags are used.
* Linear memory reserves a lazy, anonymous 4 GiB mapping on x86-64 Linux. Logical
  size/max checks implement growth; untouched new pages are already zero. This
  is not a guard-page implementation: all wasm memory bounds are checked.
* A 30,000-call depth budget prevents optimized unbounded recursion from
  becoming an infinite native loop. ABI entries and the common trap path reset
  bookkeeping, so trapping does not poison subsequent calls. Native stack
  exhaustion remains possible for large frames, as permitted by the ABI.

## Limits and tradeoffs

Only function imports have an ABI here; nonfunction imports are rejected with
an explicit diagnostic. Declared locals are limited to 1,000,000 per function
to avoid compressed local declarations exhausting the translator. The generated
instance is single-threaded and has no teardown entry point (none is specified).
The portable swizzle network is less efficient than a CPU-specific byte-shuffle
instruction. There is no dedicated pre-LLVM optimizing pass or bounds-check
elimination pass; LLVM performs ordinary promotion/inlining/optimization.

## Tests run

All execution comparisons below were run against `w2l-ref`, with generated code
compiled by LLVM 23 at **both `-O0` and `-O2`**:

* `cargo build --release --offline` and all **11** Rust unit tests pass.
* `tests/test.py`: **16 differential runs / 1,370 comparisons**, including all
  payloads' codec encode/decode workflows, packet bytes/final ranges, multi-value,
  control flow, bulk memory/table operations, host imports, traps, trap recovery,
  and malformed/invalid/out-of-scope rejection.
* `tests/scalar_matrix.py`: **41,966 commands per optimization level**, covering
  every scalar numeric opcode, NaNs, signed zeros, division overflow, shifts,
  and trapping/saturating conversion boundaries; no mismatches.
* `tests/simd_matrix.py`: **9,291 commands per optimization level**, covering
  every SIMD 1.0 opcode, lane endpoints, memory boundaries, arithmetic extremes,
  NaNs, and random vector bit patterns; no mismatches.
* `tests/smith.py`: **400 deterministic generated modules** translated, compiled,
  and differentially executed at both optimization levels. Unsupported features
  are explicitly disabled and reference loading checks the actual scope.
* `tests/recursion.py`: finite 10,000-frame calls, direct/mutual unbounded
  recursion trapping with code 8, and successful calls after traps all pass.
* `tests/state.py`: exact signaling-NaN/vector global bits, an empty export name,
  growth to the full 4 GiB wasm32 limit, and last-byte/overflow bounds pass.
* `tests/stress.py`: **5,994,351-byte** synthetic module translated to
  20,273,963 bytes of LLVM IR in **0.265 seconds** in this environment.

An additional combined clean-CARGO_HOME/empty-environment/syscall-trace check
was attempted but blocked by the session's command permissions; it was not run.
No hidden acceptance tests were available. See `tests/README.md` for commands.
