# wasm2llvm — design notes

`wasm2llvm` translates a wasm32 binary module to textual LLVM 23 IR following
`ABI.md`. It is a single Rust binary with no dependencies beyond `std`, reads
only its input file, and runs no external programs.

## Usage

```
wasm2llvm INPUT.wasm -o OUTPUT.ll
```

Exit status 0 means `OUTPUT.ll` was written. Malformed, invalid, or
out-of-scope input produces a diagnostic on stderr and a non-zero exit status.

## Feature scope

The payloads require the following, and all of it is implemented:

* WebAssembly 2.0 core, minus the reference-types proposal:
  multi-value (block/function parameters and results), bulk memory
  (`memory.init/copy/fill`, `data.drop`, `table.init/copy`, `elem.drop`),
  sign-extension operators, non-trapping float-to-int conversions, and
  MVP features (imports/exports of functions, globals, memory, one table,
  `call_indirect`, active/passive/declarative element segments, active/passive
  data segments, start function, multiple memories are *not* allowed).
* The 128-bit SIMD proposal in full: every instruction listed in the
  Wasm 2.0 binary specification (all v128 loads/stores and lane variants,
  splats, shuffles/swizzles, integer/float arithmetic and comparisons, saturating
  and narrowing operations, conversions, `bitselect`, etc.).

Out of scope and rejected: reference types (`externref`/`funcref` values,
`ref.null`, `ref.func`, `ref.is_null`, `table.get/set/size/grow/fill`, typed
`select`, multiple tables), threads/atomics, memory64, multiple memories,
tail calls, exceptions/tags, GC, extended constant expressions, relaxed SIMD,
custom page sizes and wide arithmetic. Only function imports are supported;
other import kinds are rejected because the ABI has no way to supply them.

## Architecture

Three passes, all in-process:

1. **`src/parse.rs`** — a bounds-checked binary decoder. It decodes all
   sections, module items, and instructions into an `Instr` enum with
   immediates. Integer LEB128 decoding enforces the specification's length and
   unused-bit rules. Instructions outside the supported proposals are rejected
   during decoding.
2. **`src/validate.rs`** — a full type checker following the validation
   algorithm in the specification appendix: operand/control stacks with
   polymorphic unreachable typing, block/loop/if typing, `br_table` label-type
   equality, index-space and alignment checks, lane-index checks, limits
   (memory ≤ 65536 pages), element/data segment rules, constant expressions,
   export/start rules, and the data-count constraints. Validation runs before
   code generation, so codegen may assume a well-typed module.
3. **`src/codegen.rs`** — emits textual LLVM IR directly.

### Code generation

* Every wasm function (including imported functions) is emitted as an internal
  native-signature LLVM function. Imported functions become thin wrappers that
  marshal arguments/results through 16-byte ABI slots and call
  `w2l_import_<hexmodule>_<hexname>`.
* Locals, block parameters/results that cross branches, and call temporaries
  live in entry-block `alloca`s. Values that do not cross control flow stay as
  SSA values. `br`/`br_if`/`br_table` spill the label values into per-label
  slots and jump; the merge block reloads them, so no hand-built phis are
  needed. Slot arrays and merge blocks are allocated lazily: a `block` that is
  never branched to, or an `if` with only one reachable side, costs no slots
  and no merge block at all, which keeps IR compact and native frames small (a
  function with 1000 nested untargeted blocks has the same frame as one with
  none). Unreachable code is simply not emitted; reachability is restored at
  labels that actually have predecessors.
* Multi-value results use LLVM struct returns. `v128` is canonically
  `<16 x i8>` and is bitcast to `<N x T>` lane types for vector operations.
* Memory is `mmap`-ed at instantiation (at least one page, even for a
  zero-page memory) and grown with `mremap(MREMAP_MAYMOVE)`; the logical byte
  size is tracked separately. `memory.grow` checks the declared maximum,
  returns −1 on failure, returns the old page count on success, and the new
  pages are zero-filled by the kernel. Every load/store and bulk operation
  performs an explicit overflow-safe 64-bit bounds check (`addr + offset +
  size <= mem.size`) and calls `w2l_trap(4)` on failure.
* The table is an array of function pointers plus a parallel array of
  canonical function-type ids. `call_indirect` checks bounds (5), null (6), then
  type identity (7). Canonicalisation makes structurally identical duplicate
  type indices compare equal. Element segments are compiled to pointer/type-id
  constant arrays copied with `llvm.memcpy`.
* Globals and segment lengths are simple internal LLVM globals; active
  segments are applied in `w2l_init` with the required trap codes (4 for data,
  5 for elements), and passive segments honour `data.drop`/`elem.drop`.
* `w2l_init` allocates memory/table, initialises globals and segments, and
  calls the start function.
* Floating point: no fast-math flags anywhere. `f32/f64.nearest` uses
  `llvm.roundeven`; `min`/`max` use `llvm.minimum`/`llvm.maximum` (IEEE-754
  `minimum`/`maximum`, which match wasm's NaN and signed-zero rules) followed by
  explicit quieting of any NaN result, because LLVM otherwise permits returning
  a signalling NaN unchanged. `pmin`/`pmax` are a compare plus `select`.
  Trapping float→int conversions test NaN (trap 3) and exact range boundaries
  (trap 2) before using the saturating conversion intrinsics, so no poison is
  ever produced. `rem_s` avoids `srem(INT_MIN, -1)` UB with a safe divisor and a
  select.
* SIMD is lowered to genuine vector IR: lane-typed arithmetic, `shufflevector`,
  `icmp`/`fcmp` + `sext` masks, `llvm.smin/smax/umin/umax`,
  `llvm.sadd.sat`/`ssub.sat`/`uadd.sat`/`usub.sat`, `llvm.abs`, `llvm.ctpop`,
  `llvm.fptosi.sat`/`fptoui.sat`, `llvm.minimum`/`maximum`,
  `llvm.vector.reduce.and` for `all_true`, `bitcast` of an `<N x i1>` comparison
  for `bitmask`, and half-vector shuffles for extend/narrow/extmul/dot.
  `i8x16.swizzle` uses 16 vector selects (one per source lane).
* Call-stack exhaustion: every wasm function increments a global depth counter
  in its prologue and traps with 8 above 100,000. All returns merge at one
  block that decrements it. Export wrappers and `w2l_init` reset the counter,
  so a trap (which unwinds with `longjmp` through `w2l_trap`) cannot leave it
  corrupted. The depth counter also guarantees that recursion which LLVM might
  turn into a loop (tail-call optimisation) still terminates with trap 8, while
  the 8 MiB harness stack/guard remains the primary mechanism for large frames.
  Recursion of 10,000+ small frames works at both `-O0` and `-O2`.

## Testing

Everything was tested differentially against `w2l-ref` (wasmtime) at both
`-O0` and `-O2`, using a small C harness in `tests/harness.c` that implements
the ABI (`tests/compare.py` drives it).

* **Payloads**: `qoa-simd` (encode + decode, SIMD), `minicelt` (f32 and i16
  encodes, setters, `final_range`), `minisilk`, `multi-value`, and the ABI
  example module produce byte-identical results, traps, memory hashes and host
  output to the reference.
* **Targeted suites** (`tests/run_all.py`):
  * `features` — tables (active/passive/declarative segments, `call_indirect`,
    traps 5/6/7, `table.init/copy`, `elem.drop`), bulk memory, memory
    size/grow/max, big memarg offsets, multi-value control flow (`br_if`,
    `br_table`, loop/block/if parameters), `select` including v128, globals,
    start function.
  * `float` — 2,163 checks over all numeric operators, exact truncation
    boundaries for all eight conversions, NaN/infinity/zero cases, shifts and
    rotations, with NaN payloads normalised per the ABI's allowance.
  * `simd` — 3,110 checks covering every SIMD opcode (all shapes, lanes,
    immediates, all load/store variants and bounds traps) plus memory hashes.
  * `imports` — mixed i32/i64/f32/f64/v128 multi-result imports, exports of
    imports, and imported functions called through tables (expected values).
  * `memory` — zero-page memories, `grow(0)`, growth preserving data, growth to
    300 pages, maximum enforcement, and out-of-bounds active data/element
    segments at instantiation (traps 4/5).
  * `dup-types` — structurally equal duplicate type indices for
    `call_indirect`, multi-value indirect returns, and all global types
    (including mutable v128).
  * `misc` — empty/unicode/quote-containing/empty export names, 25-parameter
    signatures, v128 multi-value returns, float select, 40-deep nesting, 3,000
    locals, `br`/`br_table` to the function label, mid-function `return`,
    dead-code control flow, and `br_table` with 0 and 100 targets.
  * `validation` — 30 semantically invalid modules and 15 malformed binaries are
    rejected, agreeing with the reference engine's accept/reject verdict on all
    of them.
* **Randomised differential testing** (`tests/fuzz.py`, `wasm-tools smith`):
  over 900 generated modules in scope (including a SIMD-heavy mode with
  `--allowed-instructions vector,...` and large random seeds), each compiled at
  `-O0` and `-O2` and compared export-by-export with random interesting
  argument slots, including traps and memory hashes. All matched. (Generated
  modules that use out-of-scope proposals, e.g. an extra memory, are rejected by
  both engines.) Active/passive/declarative element segments and active data
  segments were also checked in their flag-0/1/2/3 binary encodings, including
  the explicit table/memory-index forms.
* **Mutation testing** (`tests/mutate_test.py`): about 5,000 randomly corrupted
  payload/shape modules were checked for accept/reject agreement with the
  reference engine and for absence of translator panics (0 panics and, after
  fixing one missing check on `i8x16.shuffle` lane immediates, 0 verdict
  disagreements).
* **Degenerate shapes**: import-only modules (no code section), memory-only,
  global-only, empty modules, passive segments without any memory/table, and
  10,000-deep recursion through functions with up to 1,000 nested untargeted
  blocks (at both `-O0` and `-O2`).
* **Performance** (secondary metric): on the payloads, generated `-O2` code was
  faster than the reference runner in the same protocol harness (minicelt
  encode ≈6× faster, qoa-simd decode ≈2× faster, including protocol overhead).

## Known limitations

* Only function imports are supported. Modules importing globals, tables or
  memories cannot be represented by the ABI and are rejected.
* The depth limit is 100,000 wasm calls; legitimate recursion deeper than that
  traps 8 (the requirement is 10,000).
* NaN payloads are not canonicalised beyond the places where wasm requires a
  quiet NaN; the ABI states that tests accept any NaN where wasm allows one.
* `memory.grow` relies on `mmap`/`mremap`; if the OS refuses the mapping the
  call returns −1, as wasm permits.
* The generated IR assumes the x86-64 Linux target named in `ABI.md` (no
  target-specific intrinsics are used, but the ABI slot layout and libc
  declarations are for that target).
