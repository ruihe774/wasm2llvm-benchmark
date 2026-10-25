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
