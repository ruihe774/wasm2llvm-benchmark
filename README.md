# wasm2llvm benchmark (operator documentation — not visible to agents)

Benchmark task for AI coding agents running in Claude Code: implement a
WebAssembly (wasm32; core + bulk-memory, multi-value, nontrapping-fptoint,
sign-ext, simd128) → LLVM 23 IR translator in std-only Rust. Correctness is the
primary metric; SIMD lowering quality, validation and run-time performance are
secondary.

## Layout

| path | visible to agent | content |
|---|---|---|
| `agent/` | yes (copied into each run) | the agent workspace template |
| `agent/PROMPT.md` | yes | the prompt to give the agent |
| `agent/TASK.md` | yes | full specification incl. the ABI (generated code ↔ harness) |
| `agent/ACCEPTANCE.md` | yes | acceptance criteria |
| `agent/docs/` | yes | WebAssembly 2.0 core spec (rst), 5 proposal overviews, OCaml reference interpreter, LLVM 23 docs |
| `agent/harness/` | yes | native C harness, Python test driver `w2l.py`, protocol doc, hand-written ABI example |
| `agent/tests/spec/` | yes | 120 spec scripts (WebAssembly 2.0 testsuite incl. SIMD, filtered to scope; ~45.9k exec + 2.9k validation assertions) |
| `agent/tests/payloads/` | yes | 6 Rust programs × {O0,O3,Os} (sources in `_src/`) |
| `agent/tests/smith/` | yes | 51 wasm-smith differential tests |
| `agent/tools/w2l-ref/` | yes | source of the wasmtime reference runner (binary in `deps/bin`) |
| `deps/bin/` | yes (read) | symlinks: LLVM 23.1.4, wasm-tools 1.262.0, wabt 1.0.42 (minus `wasm2c`), wasmtime 50.0.0, `w2l-ref` |
| `deps/toolchains/` | yes (read) | extracted toolchains (`wasm2c` + its runtime are denied) |
| `deps/downloads/` | – | original archives + `SHA256SUMS` |
| `deps/src/` | **no** | spec repo at `wg-2.0` and `wg-3.0` (the latter is the source of hidden tests), LLVM docs checkout |
| `deps/build/` | – | cargo target dir of `w2l-ref` |
| `verification/` | **no** | hidden tests, verifier, generators, reference baseline |
| `sandbox/` | **no** | Claude Code settings template + run preparation script |

Rust toolchain: the system rustup `stable` = 1.99.1 (LLVM 23.1.1), targets
`wasm32v1-none`, `wasm32-unknown-unknown`, `x86_64-unknown-linux-gnu`.

## Running an agent

```
sandbox/prepare_run.sh run01                  # creates /mnt/workspace/w2l-runs/run01 + settings
```
It prints the exact commands. Headless:
```
cd /mnt/workspace/w2l-runs/run01 && claude -p "$(cat PROMPT.md)" \
  --settings /mnt/workspace/wasm2llvm/sandbox/generated/run01.settings.json \
  --setting-sources project --safe-mode --strict-mcp-config \
  --permission-mode dontAsk --output-format stream-json --verbose \
  > /mnt/workspace/w2l-runs/run01.transcript.jsonl 2> /mnt/workspace/w2l-runs/run01.stderr.log
```
(add `--model ...` / `--max-turns ...` as desired; for interactive runs drop
`-p`/`--output-format`/`--verbose` and paste PROMPT.md).

* Runs must live **outside** `/mnt/workspace/wasm2llvm` (the script enforces it);
  the default root is `/mnt/workspace/w2l-runs`.
* The settings file is generated into `sandbox/generated/` — not a location
  Claude Code reads by itself — and passed with `--settings`.
* `--setting-sources project` keeps your user settings (~/.claude/settings.json)
  out; `--safe-mode` disables CLAUDE.md, skills, plugins, hooks, MCP servers and
  custom agents from your configuration; `--strict-mcp-config` loads no MCP servers.

What the sandbox settings enforce (`sandbox/settings.template.json`):
* **Network:** sandboxed Bash runs with an empty domain allowlist (no egress);
  `WebFetch`/`WebSearch` are denied; unsandboxed fallback is disabled
  (`allowUnsandboxedCommands: false`, `failIfUnavailable: true`).
* **Writes:** Bash may only write inside the run directory (plus nothing else:
  `TMPDIR` and `CARGO_HOME` point into the run dir); Edit/Write tools are only
  allowed inside the run dir (`dontAsk` mode denies everything not allowed).
* **Reads:** Bash reads are denied for `/mnt/workspace` and `~` except the run
  dir, `deps/bin`, `deps/toolchains` (minus wasm2c), `~/.rustup`, `~/.cargo/bin`;
  file tools are blocked outside the working directory
  (`blockReadsOutsideWorkingDirectories`) and explicitly denied for
  `verification/`, `deps/src/`, `sandbox/`, this README, `~/.claude*` and `~/.ssh`.

The sandbox needs bubblewrap and socat (both installed). Before the first real
run, it is worth doing a 1-minute sanity run with a prompt like:
"Run these and report each output verbatim: `curl -sS -m5 https://example.com`;
`ls /mnt/workspace/wasm2llvm/verification`; `cat ~/.claude.json | head -1`;
`touch /mnt/workspace/x`; `clang --version | head -1`; `cargo --version`;
`wasm-tools --version`; also try to Read /mnt/workspace/wasm2llvm/README.md and
to Write /mnt/workspace/escape.txt" — all but the last three version commands
must fail. (Settings keys follow the current Claude Code docs; if your version
reports an unknown key, adjust the template.)

## Grading

```
python3 verification/verify.py /mnt/workspace/w2l-runs/run01 --out /mnt/workspace/w2l-runs/run01-report
```
(~3–5 min on 20 cores; `--quick` runs every 10th script, `--skip-perf` skips timing.)

1. Copies the submission (without `target/`), builds `cargo build --release
   --offline` in bubblewrap with a fresh `CARGO_HOME` and no network.
2. Runs the translator in a bubblewrap jail: empty environment, no network,
   `/mnt/workspace`, `/home` and system LLVM (`/usr/lib/llvm-*`) hidden; the
   input is copied into a private dir. (So shelling out to clang/wasm2c/rustc is
   impossible.)
3. Correctness suites; every module is compiled with LLVM 23 `clang -O0` and
   `-O2` and an assertion passes only if it passes in both:

   | suite | weight | content |
   |---|---|---|
   | spec | 0.35 | visible spec 2.0 scripts (pristine copy) |
   | spec3 | 0.10 | hidden: files new/changed in the spec `wg-3.0` testsuite, filtered to scope (~5.9k exec + 1.5k validation assertions) |
   | payloads | 0.30 | 6 visible + 9 hidden Rust programs × 3 opt levels (hidden: compress, crypto, nbody, numerics, raytrace, simd_image, text, traps, vm) |
   | smith | 0.25 | 51 visible + 420 hidden wasm-smith differential tests vs wasmtime |

   Suite score = mean over scripts of the pass fraction of execution assertions
   (`assert_return/trap/exhaustion/uninstantiable/output/memhash`). The
   **primary correctness score** is the weighted mean of the suite scores.
4. Secondary metrics:
   * **validation** = (binary `assert_invalid`/`assert_malformed` rejected) ×
     (fraction of valid modules that translated and instantiated);
   * **SIMD lowering quality** = fraction of 234 SIMD instructions (one module
     each, `verification/tests/simd_quality`) whose random-input test passes and
     whose `opt -O2` IR contains vector types and at most 2–3
     `extractelement`/`insertelement` (a scalarized lowering needs ≥ 6);
     `i8x16.swizzle` is informational only;
   * **performance** = geomean over 8 hidden benchmarks of wasmtime time /
     native (-O2) time for one invocation (result must be correct).
5. Writes `report.json` (everything, incl. per-script results) and `report.md`.

### Calibration: reference baseline

`verification/reference/wasm2llvm` is a *baseline* "translator" built from
wasm2c + clang (exactly what agents are forbidden to do), used to validate the
whole pipeline. Run with `--translator verification/reference/wasm2llvm --no-jail`.
Its result: correctness 0.980, validation 0.997, SIMD quality 231/234
(`trunc_sat_f64x2_*_zero` scalarized, `i64x2.all_true` wrong in simde),
performance ≈ 1.02× wasmtime. All its failures are known wasm2c deviations
(table OOB reported as memory OOB, null vs. signature-mismatch `call_indirect`
merged, a NaN-quieting difference, lax validation), which
confirms that the expectations are sound and the ABI is sufficient. A stub
submission that emits nothing scores 0 on every metric.

## Regenerating test data

All generators are deterministic; they need `deps/bin` on `PATH`.

```
export PATH=/mnt/workspace/wasm2llvm/deps/bin:$PATH
python3 verification/tools/prepare_spec.py agent/tests/spec deps/src/spec-wg-2.0/test/core
python3 verification/tools/prepare_spec.py verification/tests/spec3 <dir with wg-3.0 .wast files new/changed vs 2.0>
python3 verification/tools/build_payloads.py            # all Rust payloads (visible + hidden)
python3 verification/tools/gen_smith.py agent/tests/smith 1000000 60
python3 verification/tools/gen_smith.py verification/tests/smith 2000000 500
python3 verification/tools/gen_simd_quality.py verification/tests/simd_quality
```

Notes:
* `prepare_spec.py` keeps a command only if the module validates with exactly the
  benchmark feature set and imports only supported host functions; it then runs
  every assertion against wasmtime and drops disagreements (15 in 2.0, all due to
  cross-module linking that the benchmark doesn't support). Drop reasons are in
  `_prepare_report.json`.
* Payloads are built with stable Rust + `RUSTC_BOOTSTRAP=1` (for
  `wasm_numeric_instr`, `float_minimum_maximum`, `core_intrinsics`) and
  `-C target-feature=+bulk-memory,+multivalue,+nontrapping-fptoint,+sign-ext,+simd128`.
  Each `payload.json` lists invocations (and `bench` invocations for timing);
  traps are rejected unless `"may_trap": true`. Payload code never prints or
  stores NaN bit patterns (wasm NaN payloads are nondeterministic).
* Deep recursion that spills to the Rust shadow stack (O0 builds) makes
  "memory OOB vs. stack exhausted" implementation-defined, so such invocations
  are restricted to O3/Os; smith scripts are truncated at the first
  stack-exhaustion trap for the same reason.
* `w2l-ref` is rebuilt with
  `CARGO_TARGET_DIR=deps/build/w2l-ref cargo build --release` in
  `agent/tools/w2l-ref` and copied to `deps/bin/`.

## Scope decisions (also stated in TASK.md)

* In scope = validates under `wasm-tools validate --features=-all,mutable-global,bulk-memory,multi-value,saturating-float-to-int,sign-extension,simd,floats`.
  In wasmparser's model, expression-encoded element segments (flags 4–7) belong
  to reference-types, so they are out of scope.
* Only function imports (host functions `spectest.*`, `env.*`); imported
  memories/tables/globals and cross-module linking (`register`) are not tested.
* Trap codes are checked exactly (9 categories).
