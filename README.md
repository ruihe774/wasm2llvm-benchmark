# wasm2llvm benchmark 2.0 (operator documentation — not visible to agents)

Benchmark task for AI coding agents running in Claude Code: implement a
WebAssembly (wasm32) → LLVM 23 IR translator in std-only Rust. The agent is
given a set of real-world payloads and must discover from them which WebAssembly
feature proposals the translator has to support. All test suites are hidden. A
reference engine (`wasmtime` and the wasmtime-based `w2l-ref` runner) is
available so the agent can run the visible payloads and obtain reference
results; it still has to write its own tests and harness. Correctness is the primary
metric; SIMD lowering quality, validation and run-time performance are
secondary.

**TLDR: You may *not* need a "Pro"-level model, actually.**
A *good* "Flash"-tier model can well accomplish complex software projects
if the task is well-defined and the workflow is well-established.
See the comparison of the four runs I've performed (columns sorted by
ascending correctness):

| metric                    | GPT-6 Luna (max) | GPT-6.1 Terra (xhigh) | DeepSeek-V4.1-Flash-1017 (high) | DeepSeek-V4.1-Flash-1017 (max) |
|---------------------------|------------------|-----------------------|---------------------------------|--------------------------------|
| **correctness (primary)** | 0.8101           | 0.9923                | 0.9931                          | 0.9999                         |
| validation                | 0.8620           | 0.9983                | 0.9969                          | 0.9998                         |
| SIMD quality              | 221/234          | 226/234               | 234/234                         | 234/234                        |
| performance               | 1.116×           | 0.896×                | 1.089×                          | 1.096×                         |
| spec                      | 0.9044           | 0.9887                | 0.9914                          | 0.9997                         |
| spec3                     | 0.9544           | 0.9565                | 0.9565                          | 1.0000                         |
| payloads                  | 0.8667           | 1.0000                | 1.0000                          | 1.0000                         |
| real                      | 1.0000           | 1.0000                | 1.0000                          | 1.0000                         |
| smith                     | 0.3503           | 1.0000                | 1.0000                          | 1.0000                         |
| wall time                 | 69 min           | 39 min                | 53 min                          | 57 min                         |

(All runs were done in Claude Code. For DeepSeek, the official Anthropic-compatible endpoint was used.
For GPT, cc-switch was used to convert official OpenAI Responses endpoint.
The auto compact window of DeepSeek was set to 786432, while that of GPT models was set to 300000,
which was a similar setting to 272k used in Codex.
DeepSeek runs used no subagents, while GPT runs used at most one concurrent subagent.
Wall time is the session span from its first to its last transcript entry, rounded to minutes.
This comparison is illustrative.)

## Layout

| path | visible to agent | content |
|---|---|---|
| `agent/` | yes (copied into each run) | the agent workspace template |
| `agent/PROMPT.md` | yes | the prompt to give the agent |
| `agent/TASK.md` | yes | task description (the payloads define the feature scope) |
| `agent/ABI.md` | yes | the generated-code ABI contract (generated code ↔ harness) |
| `agent/ACCEPTANCE.md` | yes | acceptance criteria |
| `agent/payloads/` | yes | the real-world payloads and their interface descriptions: QOA, CELT, SILK, multi-value example |
| `agent/examples/abi/` | yes | hand-written ABI example (`example.wat`, `example.ll`) |
| `agent/examples/reference/` | yes | line protocol of the `w2l-ref` reference runner |
| `agent/docs/` | yes | WebAssembly 2.0 core spec (rst), 5 proposal overviews, OCaml reference interpreter, LLVM 23 docs |
| `verification/harness/` | **no** | native C harness, Python test driver `w2l.py`, protocol doc |
| `verification/tests/spec/` | **no** | 120 spec scripts (WebAssembly 2.0 testsuite incl. SIMD, filtered to scope; ~45.9k exec + 2.9k validation assertions) |
| `verification/tests/spec3/` | **no** | files new/changed in the spec 3.0 testsuite, filtered to scope (~5.9k exec + 1.5k validation assertions) |
| `verification/tests/payloads/` | **no** | 15 Rust programs × {O0,O3,Os} (sources in `verification/payloads/{visible,hidden}`) |
| `verification/tests/real/` | **no** | 11 real-payload tests (QOA encode/decode, CELT/SILK f32/i16, release/checked builds) with inputs that are not in the workspace |
| `verification/tests/smith/` | **no** | 471 wasm-smith differential tests |
| `verification/tests/simd_quality/` | **no** | one module per SIMD instruction for the lowering-quality metric |
| `verification/tools/w2l-ref/` | **no** | source of the wasmtime reference runner (binary in `deps/bin`) |
| `verification/payloads/` | **no** | payload sources (Rust crates) and the hidden real-payload binaries, including `-checked` builds |
| `deps/bin/` | yes (read) | symlinks: LLVM 23.1.4, wasm-tools 1.262.0, wabt 1.0.42 (minus `wasm2c`), wasmtime 50.0.0, `w2l-ref` |
| `deps/toolchains/` | yes (read) | extracted toolchains (`wasm2c` + its runtime are denied) |
| `deps/downloads/` | – | original archives + `SHA256SUMS` |
| `deps/src/` | **no** | spec repo at `wg-2.0` and `wg-3.0` (the latter is the source of hidden tests), LLVM docs checkout |
| `deps/build/` | – | cargo target dir of `w2l-ref` |
| `verification/` | **no** | hidden tests, verifier, generators, reference baseline |
| `sandbox/` | **no** | Claude Code settings template + run preparation script |

Rust toolchain: the system rustup `stable` = 1.99.1 (LLVM 23.1.1), targets
`wasm32v1-none`, `wasm32-unknown-unknown`, `x86_64-unknown-linux-gnu`.

The agent gets the payload binaries and their interface descriptions, plus a
reference engine (`wasmtime` and the `w2l-ref` runner, protocol in
`agent/examples/reference/PROTOCOL.md`), but no test scripts and no test
harness. `wasm-tools` (including `smith`, `parse`, `print`, `dump`, `validate`)
and the LLVM tools remain available, as do the Rust and C compilers and the wabt
interpreters.

## Setup (fresh clone)

```
./setup.sh                      # or ./setup.sh --skip-ref-build
```
Fetches everything git ignores: the LLVM/wabt/wasm-tools/wasmtime archives
(verified against `deps/downloads/SHA256SUMS`) and their extraction into
`deps/toolchains/`, the spec repos and LLVM docs in `deps/src/`, the wabt/simde
checkouts under `verification/reference/third_party/`, the rustup targets, and a
build of `deps/bin/w2l-ref` (from `verification/tools/w2l-ref`). It is
idempotent. It needs network access, git, curl, tar, zstd, xz, bubblewrap, socat
and rustup/cargo, and downloads ~1.2 GB (LLVM is extracted with
`zstd --long=31`). `deps/src/wasm-core-2.html` is fetched from w3.org on a
best-effort basis. `sandbox/generated/` and benchmark results are not
recreated.

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
  dir, `deps/bin`, `deps/toolchains`, `~/.rustup`, `~/.cargo/bin`; file tools are
  blocked outside the working directory (`blockReadsOutsideWorkingDirectories`)
  and explicitly denied for `verification/`, `deps/src/`, `sandbox/`, this
  README, `~/.claude*` and `~/.ssh`.
* **Reference engines:** `wasmtime`, `w2l-ref` and the wabt interpreters remain
  available so the agent can run the visible payloads and its own modules. The
  hidden test data, harness and verifier are denied.

The sandbox needs bubblewrap and socat (both installed). Before the first real
run, it is worth doing a 1-minute sanity run with a prompt like:
"Run these and report each output verbatim: `curl -sS -m5 https://example.com`;
`ls /mnt/workspace/wasm2llvm/verification`; `cat ~/.claude.json | head -1`;
`touch /mnt/workspace/x`; `clang --version | head -1`; `cargo --version`;
`wasm-tools --version`; `wasmtime --version`; also try to Read
/mnt/workspace/wasm2llvm/README.md and to Write /mnt/workspace/escape.txt" —
`clang`, `cargo`, `wasm-tools` and `wasmtime` must succeed; the rest must fail. (Settings keys follow the
current Claude Code docs; if your version reports an unknown key, adjust the
template.)

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
   `-O2` and an assertion passes only if it passes in both. All suites are
   hidden:

   | suite | weight | content |
   |---|---|---|
   | spec | 0.30 | spec 2.0 scripts (pristine copy; ~45.9k exec + 2.9k validation assertions) |
   | spec3 | 0.10 | files new/changed in the spec `wg-3.0` testsuite, filtered to scope |
   | payloads | 0.20 | 15 Rust programs × 3 opt levels (6 previously visible + 9 hidden: compress, crypto, nbody, numerics, raytrace, simd_image, text, traps, vm) |
   | real | 0.20 | 11 real-payload tests: QOA mono/stereo roundtrips, CELT mono/stereo/f32/i16/checked, SILK f32/i16/checked |
   | smith | 0.20 | 471 wasm-smith differential tests vs wasmtime |

   Suite score = mean over scripts of the pass fraction of execution assertions
   (`assert_return/trap/exhaustion/uninstantiable/output/memhash`). The `real`
   suite is different: it is scored **all-or-nothing per script**, so one wrong
   assertion (for example a wrong memory hash) scores that script 0; its
   assertion totals in the report are diagnostic. The **primary correctness
   score** is the weighted mean of the suite scores.
4. Secondary metrics:
   * **validation** = (binary `assert_invalid`/`assert_malformed` rejected) ×
     (fraction of valid modules that translated and instantiated);
   * **SIMD lowering quality** = fraction of 234 SIMD instructions (one module
     each, `verification/tests/simd_quality`) whose random-input test passes and
     whose `opt -O2` IR contains vector types and at most 2–3
     `extractelement`/`insertelement` (a scalarized lowering needs ≥ 6);
     ops with a scalar operand or result (`extract_lane`, `store*_lane`,
     `load*_zero`, `any_true`/`all_true`/`bitmask`; `scalar_ok` in the manifest)
     may instead have no vector types if the wasm code does ≤ 3 non-global
     loads/stores, since `opt` legitimately narrows the ABI slot accesses there;
     `i8x16.swizzle` is informational only;
   * **performance** = geomean over hidden benchmarks of wasmtime time /
     native (-O2) time for one invocation (result must be correct).
5. Writes `report.json` (everything, incl. per-script results) and `report.md`.

The real-payload tests use the same visible binaries plus hidden `-checked`
builds and inputs that are not in the agent workspace. They drive the codecs
through the ABI (memory writes, init, encode/decode) and hash the whole linear
memory after the calls; see `verification/tools/gen_real_payloads.py` and
`verification/harness/PROTOCOL.md`.

### Calibration: reference baseline

`verification/reference/wasm2llvm` is a *baseline* "translator" built from
wasm2c + clang (exactly what agents are forbidden to do), used to validate the
whole pipeline. Run with `--translator verification/reference/wasm2llvm --no-jail`.
Its result on the revised (hidden) suites, run with `--skip-perf`, is
correctness 0.9813, validation 0.9966, SIMD quality 231/234
(`trunc_sat_f64x2_*_zero` scalarized, `i64x2.all_true` wrong in simde), with
spec 0.9823, spec3 0.8781, payloads 0.9975, real 1.0000 and smith 0.9967. Its
remaining failures are explainable by known wasm2c deviations (table OOB
reported as memory OOB, null vs. signature-mismatch `call_indirect` merged, lax
validation of some malformed binaries, a NaN-quieting difference). Performance
was ≈1.02× wasmtime in the previous revision and was not re-measured for this
revision. A stub submission that emits nothing scores 0 on every metric.

## Regenerating test data

All generators are deterministic; they need `deps/bin` on `PATH`.

```
export PATH=/mnt/workspace/wasm2llvm/deps/bin:$PATH
python3 verification/tools/prepare_spec.py verification/tests/spec deps/src/spec-wg-2.0/test/core
python3 verification/tools/prepare_spec.py verification/tests/spec3 <dir with wg-3.0 .wast files new/changed vs 2.0>
python3 verification/tools/build_payloads.py            # all Rust payloads, visible and hidden sources
python3 verification/tools/gen_smith.py verification/tests/smith 1000000 60
python3 verification/tools/gen_smith.py verification/tests/smith 2000000 500
python3 verification/tools/gen_simd_quality.py verification/tests/simd_quality
python3 verification/tools/gen_real_payloads.py         # QOA/CELT/SILK hidden tests
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
* The real-payload binaries live in `verification/payloads/real/`; the visible
  copies in `agent/payloads/` are what the agent sees. `gen_real_payloads.py`
  reads the hidden copies and records expectations with `w2l-ref`.
* `w2l-ref` is rebuilt with
  `CARGO_TARGET_DIR=deps/build/w2l-ref cargo build --release` in
  `verification/tools/w2l-ref` and copied to `deps/bin/`.

## Scope decisions (operator only; not stated to the agent)

* In scope = validates under `wasm-tools validate --features=-all,mutable-global,bulk-memory,multi-value,saturating-float-to-int,sign-extension,simd,floats`.
  The visible payloads exercise every proposal in this set:
  `qoa-simd` (SIMD, bulk memory, sign-ext, saturating conversions, mutable
  globals), `minicelt`/`minisilk` (SIMD, bulk memory, sign-ext, saturating
  conversions), and the `multi-value` example (multiple results and block
  parameters). Relaxed SIMD, reference types, GC, function references, tail
  calls, exceptions, threads, memory64 and multi-memory are out of scope and are
  not used by any visible payload. Do not add these proposals to the visible set
  without also adding a payload that justifies them.
* Only function imports (host functions `spectest.*`, `env.*`); imported
  memories/tables/globals and cross-module linking (`register`) are not tested.
* Trap codes are checked exactly (9 categories).
