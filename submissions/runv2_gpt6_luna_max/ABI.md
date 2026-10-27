# Generated-code ABI

The emitted `.ll` is compiled by the grader with LLVM 23 exactly like this,
**at both `-O0` and `-O2`**:

```
clang -O0|-O2 -shared -fPIC -Wl,-Bsymbolic -o module.so OUTPUT.ll -lm
```

The result is loaded with `dlopen` into a harness process, which drives it
through the ABI below. The harness is not part of this workspace; this file is
the complete contract.

## Output requirements

* The IR must be valid LLVM 23 IR and target `x86_64-unknown-linux-gnu`. You may
  omit the target triple or data layout, or set them to the usual x86-64 Linux values.
* The IR must have well-defined behavior for **every** input the original wasm
  module can receive. It must not rely on LLVM undefined behavior, `poison`, or
  `undef` for observable results. The code is compiled with optimizations, so
  any UB will be exploited.
* Floating point must follow WebAssembly semantics exactly. In particular, do not
  use fast-math flags (`fast`, `contract`, `nnan`, `nsz`, ...) that change results.
  `fma` contraction is not allowed. NaN payloads follow the wasm rules, and the
  tests accept any NaN wherever wasm allows any NaN.
* Each `.so` is one module instance. Use whatever global state you like, but give
  everything except the ABI symbols `internal`/`private` linkage.
* You may call functions from the C library and libm (for example `malloc`, `calloc`,
  `realloc`, `free`, `mmap`, `munmap`, `mremap`, `memcpy`, `memmove`, `memset`,
  `abort`, `ceilf`, ...) and any LLVM intrinsic.
* Do **not** install signal handlers or depend on hardware faults (guard pages,
  `SIGFPE`, ...) for wasm semantics. The harness owns signal handling. Bounds and
  trap conditions must be checked explicitly in the generated code. The one exception
  is native stack overflow, described below.

## Slots and names

All values cross the ABI in **16-byte slots**. A slot holds one wasm value,
little-endian, in its low bytes: i32/f32 in bytes 0..3 (f32 as its IEEE bit
pattern), i64/f64 in bytes 0..7, v128 in bytes 0..15 (byte *i* is lane byte
*i*). Unused bytes are don't-care on input and may be anything on output. Slot
arrays are 16-byte aligned.

Names are encoded as **lowercase hex of their raw UTF-8 bytes**. For example
`add` → `616464`, the empty name → `` (empty string).

## Symbols the generated module must define (external linkage)

| symbol | signature | meaning |
|---|---|---|
| `w2l_init` | `void ()` | Instantiate: allocate memory and table, initialize globals, apply active element/data segments, run the start function. Called exactly once, before anything else. May trap. |
| `w2l_export_<hex(name)>` | `void (ptr args, ptr results)` | One per **exported function**. `args` holds the parameters (slot *i* = parameter *i*); write result *i* into `results` slot *i*. |
| `w2l_global_<hex(name)>` | `void (ptr out)` | One per **exported global**: write its current value into slot `out`. |
| `w2l_memory_<hex(name)>` | `ptr (ptr size_out)` | One per **exported memory**: return the base address of linear memory and store its current size in bytes (i64) to `*size_out`. |

Exported tables need no symbol.

## Symbols the harness provides

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

## Trap codes

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

## Call stack exhaustion

Unbounded recursion must trap with code 8 in bounded time, never hang or crash.
The harness runs wasm code on a thread with an 8 MiB stack and a large guard
region below it; a native stack overflow that faults inside that guard region is
reported as trap 8. Legitimate recursion at least 10 000 frames deep (small
frames) must work.

## Instance state after a trap

After a trap the harness may call further exports of the same instance. That is
legal wasm, since a trap does not destroy the instance. Make sure no internal
bookkeeping (such as a depth counter) is left corrupted when `w2l_trap` unwinds
your code.

## Memory

Linear memory starts at its declared minimum size and grows with `memory.grow`
up to its declared maximum, or 65536 pages if there is none. `memory.grow` returns -1
if growth exceeds the limit (or cannot be satisfied). Otherwise it returns the old
size in pages, and the new pages are zeroed. The address returned by
`w2l_memory_*` is only required to be valid until the next call into the module.

A worked, hand-written example of the ABI is in `examples/abi/`
(`example.wat` and its translation `example.ll`).
