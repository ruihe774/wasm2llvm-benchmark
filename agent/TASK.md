# Task: a WebAssembly → LLVM IR translator

Implement **`wasm2llvm`**, a translator from WebAssembly binary modules
(`wasm32-unknown-unknown` style core modules) to textual **LLVM IR** accepted by
**LLVM 23** (the toolchain in this environment is LLVM 23.1.4).

```
wasm2llvm INPUT.wasm -o OUTPUT.ll
```

Correctness is the primary metric: a module that is translated with wrong
semantics is a failure. The quality of the generated IR is the secondary
metric. For example, lowering WebAssembly SIMD to scalar operations instead of
LLVM vector operations counts as poor quality, and so does wasting time at runtime.

No "lifting" is required. You do not have to recover structure that WebAssembly
does not express and LLVM IR does not need, such as loops, types or variables beyond
what the binary gives you. You only need a faithful, efficient translation.

## 1. Scope

The translator must support the WebAssembly **core specification** (the
WebAssembly 1.0 instruction set, including import/export of mutable globals)
plus exactly these proposals:

| proposal | notes |
|---|---|
| bulk-memory | `memory.copy/fill/init`, `data.drop`, `table.copy/init`, `elem.drop`, passive/declarative segments |
| multi-value | multiple results for functions and blocks, block parameters |
| nontrapping-fptoint | `*.trunc_sat_*` |
| sign-ext | `i32.extend8_s` etc. |
| simd128 | all 128-bit SIMD instructions (no relaxed-simd) |

**Exact scope.** A module is in scope iff it validates under:
```
wasm-tools validate --features=-all,mutable-global,bulk-memory,multi-value,saturating-float-to-int,sign-extension,simd,floats
```
Everything else, such as reference types (externref, `table.get/set/grow/size/fill`,
typed `select`, multiple tables), element segments in the expression encoding
(segment flags 4–7), multi-memory, memory64, threads, tail calls, exceptions, GC and
relaxed SIMD, is out of scope. The translator should reject such modules with an error.

At most one memory and one table exist (either may be absent). Tables hold
`funcref`.

**Imports.** Only *function* imports are supported. A module that imports a
memory, table or global may be rejected.

The specs are in `docs/`. See section 7.

## 2. Deliverable

* A Cargo project at the root of this directory that builds the binary
  `wasm2llvm` with `cargo build --release --offline`
  (output: `target/release/wasm2llvm`).
* **Rust, standard library only.** There is no network and no crate registry,
  so the project must have no dependencies other than `std`.
* The translator must be self-contained. At translation time it must not
  execute other programs (no `clang`, `wasm2c`, `wasm-tools`, `llc`, ...) and must
  not read any file other than its input. It is graded inside a jail with an
  empty environment, no network and nothing but its input file visible.
* Exit status 0 means `OUTPUT.ll` was written. On malformed or invalid input (or
  out-of-scope features), exit with a non-zero status and a message on stderr.
* Translation must take reasonable time (well under a minute even for
  multi-megabyte modules).

## 3. Output requirements

The emitted `.ll` is compiled by the grader with LLVM 23 exactly like this,
**at both `-O0` and `-O2`**:

```
clang -O0|-O2 -shared -fPIC -Wl,-Bsymbolic -o module.so OUTPUT.ll -lm
```

The result is loaded with `dlopen` into a harness process, which drives it
through the ABI below. Requirements:

* The IR must be valid LLVM 23 IR and target `x86_64-unknown-linux-gnu`. You may
  omit the target triple or data layout, or set them to the usual x86-64 Linux values.
* The IR must have well-defined behavior for **every** input the original wasm
  module can receive. It must not rely on LLVM undefined behavior, `poison`, or
  `undef` for observable results. The code is compiled with optimizations, so
  any UB will be exploited. Typical traps: `udiv`/`sdiv` by zero or `INT_MIN/-1`,
  over-wide shifts, `fptosi`/`fptoui` out of range, out-of-bounds pointers, and
  infinite recursion that LLVM may turn into an infinite loop.
* Floating point must follow WebAssembly semantics exactly. In particular, do not
  use fast-math flags (`fast`, `contract`, `nnan`, `nsz`, ...) that change results.
  `fma` contraction is not allowed. NaN *payloads* follow the wasm rules, and the
  tests accept any NaN wherever wasm allows any NaN.
* Each `.so` is one module instance. Use whatever global state you like, but give
  everything except the ABI symbols `internal`/`private` linkage.
* You may call functions from the C library and libm (for example `malloc`, `calloc`,
  `realloc`, `free`, `mmap`, `munmap`, `mremap`, `memcpy`, `memmove`, `memset`,
  `abort`, `ceilf`, ...) and any LLVM intrinsic.
* Do **not** install signal handlers or depend on hardware faults (guard pages,
  `SIGFPE`, ...) for wasm semantics. The harness owns signal handling. Bounds and
  trap conditions must be checked explicitly in the generated code. The one exception
  is native stack overflow, described in 4.4.

## 4. ABI between generated code and the harness

All values cross the ABI in **16-byte slots**. A slot holds one wasm value,
little-endian, in its low bytes: i32/f32 in bytes 0..3 (f32 as its IEEE bit
pattern), i64/f64 in bytes 0..7, v128 in bytes 0..15 (byte *i* is lane byte
*i*). Unused bytes are don't-care on input and may be anything on output. Slot
arrays are 16-byte aligned.

Names are encoded as **lowercase hex of their raw UTF-8 bytes**. For example
`add` → `616464`, the empty name → `` (empty string).

### 4.1 Symbols the generated module must define (external linkage)

| symbol | signature | meaning |
|---|---|---|
| `w2l_init` | `void ()` | Instantiate: allocate memory and table, initialize globals, apply active element/data segments, run the start function. Called exactly once, before anything else. May trap. |
| `w2l_export_<hex(name)>` | `void (ptr args, ptr results)` | One per **exported function**. `args` holds the parameters (slot *i* = parameter *i*); write result *i* into `results` slot *i*. |
| `w2l_global_<hex(name)>` | `void (ptr out)` | One per **exported global**: write its current value into slot `out`. |
| `w2l_memory_<hex(name)>` | `ptr (ptr size_out)` | One per **exported memory**: return the base address of linear memory and store its current size in bytes (i64) to `*size_out`. |

Exported tables need no symbol.

### 4.2 Symbols the harness provides

| symbol | signature | meaning |
|---|---|---|
| `w2l_trap` | `void (i32 code)` **noreturn** | Report a trap. It never returns (the harness `longjmp`s out of your code). |
| `w2l_import_<hex(module)>_<hex(name)>` | `void (ptr args, ptr results)` | One per imported function, same slot convention as exports. |

The harness provides these host functions for import:

| module | name | type | effect |
|---|---|---|---|
| `spectest` | `print`, `print_i32`, `print_i64`, `print_f32`, `print_f64`, `print_i32_f32`, `print_f64_f64` | params as named, no results | write to the host output |
| `env` | `print_i32` (i32), `print_i64` (i64), `print_f32` (f32), `print_f64` (f64), `print_v128` (v128), `putchar` (i32) | no results | write to the host output |

A module importing anything else may fail to load. That is not your problem.

### 4.3 Trap codes

When execution traps, call `w2l_trap` with:

| code | trap |
|---|---|
| 0 | `unreachable` executed |
| 1 | integer divide by zero |
| 2 | integer overflow (`INT_MIN / -1`, trapping float→int out of range) |
| 3 | invalid conversion to integer (trapping float→int of NaN) |
| 4 | out-of-bounds memory access, including `memory.init/copy/fill` and data segments |
| 5 | out-of-bounds table access / undefined element, including `call_indirect` index ≥ table size, `table.init/copy` and element segments |
| 6 | uninitialized element (`call_indirect` to a null entry) |
| 7 | indirect call type mismatch |
| 8 | call stack exhausted |

The test harness checks the exact code.

### 4.4 Call stack exhaustion

Unbounded recursion must trap with code 8 in bounded time, never hang or crash.
The harness runs wasm code on a thread with an 8 MiB stack and a large guard region
below it. A native stack overflow that faults inside that guard region is reported as
trap 8. Relying on that alone is fragile, though. For example, LLVM may turn
recursion into a loop, and very large frames can skip the guard. Explicit depth
accounting that calls `w2l_trap(8)` is the robust approach. Legitimate recursion
at least 10 000 frames deep (small frames) must work.

### 4.5 Instance state after a trap

After a trap the harness may call further exports of the same instance. That is
legal wasm, since a trap does not destroy the instance. Make sure no internal
bookkeeping (such as a depth counter) is left corrupted when `w2l_trap` unwinds
your code.

### 4.6 Memory

Linear memory starts at its declared minimum size and grows with `memory.grow`
up to its declared maximum, or 65536 pages if there is none. `memory.grow` returns -1
if growth exceeds the limit (or cannot be satisfied). Otherwise it returns the old
size in pages, and the new pages are zeroed. The address returned by
`w2l_memory_*` is only required to be valid until the next call into the module.

A worked, hand-written example of the ABI is in `harness/example/`
(`example.wat` and its translation `example.ll`).

## 5. Validation

WebAssembly requires implementations to reject malformed and invalid
modules. Your translator should exit non-zero for them. This is scored as a
separate, secondary metric: the spec tests' `assert_malformed` and `assert_invalid`
on binary modules. Execution correctness of valid modules is what matters most.

## 6. How you are evaluated

Hidden tests of the same kinds as the visible ones in `tests/`:

* the WebAssembly spec testsuite (2.0 core + SIMD), filtered to the scope above
  (visible), plus additional spec tests (hidden);
* Rust programs compiled to wasm (`wasm32v1-none`, with
  `-C target-feature=+bulk-memory,+multivalue,+nontrapping-fptoint,+sign-ext,+simd128`)
  at several optimization levels: the visible ones plus more hidden ones;
* randomly generated modules (wasm-smith) compared against wasmtime: visible and
  hidden ones.

Every module is compiled at both `-O0` and `-O2`, and an assertion passes only if
it passes in both builds. Secondary metrics are validation (section 5), SIMD lowering
quality (whether SIMD instructions become LLVM vector operations rather than
per-lane scalar code), and run time of the generated code on compute-heavy
programs, compared with wasmtime.

## 7. Environment

Everything is offline. Tools are on `PATH` (from `/mnt/workspace/wasm2llvm/deps/bin`):

* LLVM 23.1.4: `clang`, `opt`, `llc`, `llvm-as`, `llvm-dis`, `llvm-link`, `lli`, ...
* `wasm-tools` 1.262 (`parse` wat→wasm, `print`, `dump`, `validate`, `smith`,
  `json-from-wast`, ...)
* wabt 1.0.42: `wat2wasm`, `wasm2wat`, `wasm-objdump`, `wast2json`, `spectest-interp`, ...
* `wasmtime` 50 CLI, and `w2l-ref`, a wasmtime-based reference runner that speaks
  the same protocol as the native harness (source in `tools/w2l-ref/`).
* Rust 1.99.1 (LLVM 23.1.1) with targets `x86_64-unknown-linux-gnu`,
  `wasm32v1-none`, `wasm32-unknown-unknown`.
* Python 3.14 (standard library only).

Documentation (`docs/`):

* `docs/wasm-spec-2.0/core/` – the WebAssembly 2.0 core specification (reStructuredText
  sources: `syntax/`, `valid/`, `exec/`, `binary/`, `appendix/`...)
* `docs/wasm-proposals/` – overviews of the five proposals (SIMD: `simd/SIMD.md`,
  `simd/BinarySIMD.md`, `simd/NewOpcodes.md`)
* `docs/wasm-reference-interpreter/` – the OCaml reference interpreter (executable
  semantics; see e.g. `exec/`)
* `docs/llvm-23/` – the LLVM 23 documentation sources (`LangRef.rst` above all)

## 8. Tests and tools (in this directory)

* `harness/w2l_harness.c` – the native harness (see `harness/PROTOCOL.md`)
* `harness/w2l.py` – test driver. Typical use:
  ```
  cargo build --release
  python3 harness/w2l.py run tests/spec tests/payloads tests/smith -j 20 --show-failures 3
  python3 harness/w2l.py run --opt 2 tests/spec/i32          # one suite, one opt level
  python3 harness/w2l.py run --backend ref tests/payloads     # run against wasmtime instead
  ```
* `tests/spec/` – spec tests converted to JSON scripts (wast2json format) plus `.wasm` files
* `tests/payloads/` – Rust programs (sources in `tests/payloads/_src/`) at O0/O3/Os
* `tests/smith/` – wasm-smith differential tests
* You can create your own tests. Write a `.wat`, convert it with `wasm-tools parse`,
  write a script containing bare `action` commands, and let
  `python3 harness/w2l.py record script.json out.json` fill in the expected results
  from wasmtime. `wasm-tools smith` is available for fuzzing.

Do not modify `tests/` or `harness/` to make tests pass. The grader uses its own
pristine copies.
