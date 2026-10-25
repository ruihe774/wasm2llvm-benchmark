# run_dsf_1017 (DeepSeek-V4.1-Flash-1017) — failure analysis

Correctness 0.9839, validation 0.9996, SIMD quality 222/234, perf 1.124x wasmtime.

## Transcript audit
All 440 tool calls reviewed: no prohibited or gaming behavior (no wasm2c, no access to
verification/ or deps/src, tests/ and harness/ unchanged vs. the initial commit, translator
reads only its input and has no process spawning). 15 Bash calls were denied by dontAsk mode;
the agent did not try to bypass them.

## Failures
1. `smith:hidden/smith2000205` — **agent bug.** The module is valid and in scope
   (`(elem declare func)` + `table.init 0` / `elem.drop 0`). `codegen.rs` only emits
   `@w2l.elemN` when the segment has >=1 function, but `table.init` always references it,
   so clang fails with "use of undefined value '@w2l.elem0'". Fix: emit an empty global
   (or skip the memcpy) for empty/declarative segments.
2. `payloads:hidden/numerics-{O0,O3,Os}` (and the numerics perf bench) — **mostly a
   benchmark-test issue, with an agent deviation from hardware behavior.** Bisected by
   exporting `conv`/`int_ops`/`i128_ops` and comparing against wasmtime: only `conv` fails,
   exactly for inputs where the f32 `g` is a negative-sign NaN. `conv` computes
   `f.copysign(g as f64)`, which leaks the sign of the NaN produced by `f64.promote_f32`.
   wasmtime/x86 preserve sign and payload (0xffc00000 -> 0xfff8000000000000); the agent's
   translator canonicalises promote/demote results to the positive canonical NaN. The wasm
   spec allows any arithmetic NaN (either sign), so the agent is spec-legal, yet the payload
   makes a NaN sign observable, contradicting the README's claim that payloads never depend on
   NaN bits. Suggested harness fix: remove NaN-sign dependence from `numerics/lib.rs`
   (e.g. mask `g` to non-NaN before copysign) and rebuild the hidden payload.

## Why the agent canonicalises NaN in promote/demote (transcript, ~03:41-03:43)
* Trigger: visible spec test `float_exprs.wast` (lines 638 and 2415): `f32.demote_f64(f64.promote_f32(sNaN))`
  must yield an *arithmetic* NaN (quiet bit set; line 2415 masks the result with 0x7fc00000).
  At -O2 LLVM folds `fptrunc(fpext x)` -> `x`, so the sNaN came back unquieted (0x7fa00000).
  This is a legitimate spec requirement, not a benchmark bug. (The agent briefly misread line 2415 as
  requiring the exact canonical NaN; it didn't notice the mask.) The same test class drove the
  volatile-constant trick for `x-0.0` / `x*1.0`, which preserves hardware NaN behavior.
* Decision (event ~612): barrier tricks seemed too costly, so it chose "if the input is NaN, return the
  positive canonical NaN" for promote/demote. It never weighed that this drops the sign and payload that
  wasmtime/x86 preserve. It is spec-legal but a blunt over-fix; a faithful fix would only set the quiet bit
  (e.g. `select isnan, or(bits, quiet), fpext`) or use an opaque barrier.
* Benchmark share of the blame: only that the hidden `numerics` payload observes a NaN's sign through
  `copysign`, which the README says payloads avoid. Agent share: the unnecessary sign/payload loss.
