# wasm2llvm — design and test notes

`wasm2llvm INPUT.wasm -o OUTPUT.ll` translates a wasm32 module into textual
LLVM 23 IR following `ABI.md`.  It is a single Rust binary with no
dependencies beyond `std`.

## Scope

Supported in full (every instruction of each proposal):

* WebAssembly core 1.0: control flow (including `br_table` and multiple
  results/block parameters), numeric instructions, memory, tables,
  `call_indirect`, globals, element/data segments, start function.
* bulk memory: `memory.init/copy/fill`, `data.drop`, `table.init/copy`,
  `elem.drop`, passive and declarative segments.
* multi-value: multiple function/block results and block parameters
  (including loop parameters).
* sign-extension operators (`extend8_s`, `extend16_s`, `extend32_s`).
* non-trapping float-to-int conversions (`trunc_sat_*`).
* SIMD: all instructions of the 128-bit SIMD proposal, including loads/stores,
  lane operations, conversions, saturation and pseudo-min/max.

The 2.0 core reference-type constructs are also implemented (funcref /
externref values, `ref.null/is_null/func`, `table.get/set/grow/size/fill`,
typed `select`, multiple tables and element-expression segments); they share
the table machinery used by bulk memory.  The reference runner used for
differential testing does not enable reference types, so those paths were
tested by hand/structurally rather than differentially.

Explicitly rejected (nonzero exit, message on stderr): threads/atomics,
shared memories, tail calls, exception handling/tags, memory64,
multi-memory, extended constant expressions, GC, typed function references /
`call_ref`, relaxed SIMD, custom page sizes, wide arithmetic, and unknown
opcodes or section kinds.

## Translation design

* **Parser/validator.** The binary reader enforces the spec LEB128 rules
  (including length and unused-bit constraints).  Decoding is followed by a
  full validation pass implementing the spec's validation algorithm: a
  polymorphic operand stack, control frames, structural block types, label
  typing for branches, `br_table` type consistency, and module-level rules
  (index bounds, limits, constant expressions, declared `ref.func` set,
  duplicate export names, segment/table/memory types, data-count rules).

* **Functions.** Each wasm function becomes an internal LLVM function with a
  native signature; multiple results use a literal struct.  The wasm operand
  stack is translated to SSA values.  Structured control flow uses basic
  blocks and phi nodes: branch values become phi inputs at the target block's
  merge point, loop headers carry parameter phis, and `br_table` uses
  per-target trampoline blocks so duplicate targets and phi predecessors stay
  well-formed.  Dead code is tracked so no instructions are emitted on
  unreachable paths; merge blocks are materialised only when needed.
  `memory.grow`/`table.grow` can move bases, so the base pointer is re-loaded
  at each access.

* **Locals.** Parameters and declared locals are allocas and are explicitly
  zero-initialised at function entry (wasm semantics; LLVM's mem2reg removes
  the stores at -O2).

* **ABI.** Slots are read/written as 16-byte little-endian slots in export
  wrappers and import trampolines.  Imported functions get an internal
  trampoline with the native signature, so direct calls and `call_indirect`
  through table entries work uniformly.  Tables store pointers to private
  descriptor constants `{ canonical type id, function pointer }`;
  `call_indirect` checks index bounds (trap 5), null (6), and structural
  type equality using canonicalised type ids (7).

* **Traps.** All trap conditions are explicit: divide-by-zero/overflow,
  conversion NaN/range, memory and table bounds, bulk operations, indirect
  call checks, and `unreachable`, with the ABI trap codes.  Each function
  checks a global call-depth counter (reset at every ABI entry, so traps do
  not corrupt it) and a native stack budget derived from
  `llvm.frameaddress`, trapping with code 8.

* **Memory.** Linear memory is `calloc`ed at instantiation and grown with
  `realloc`; new pages are zeroed and `memory.grow` returns -1 on failure or
  when exceeding the maximum.  Every access uses an unsigned 64-bit bounds
  check and `align 1` (wasm allows unaligned accesses).  Bulk operations use
  `memcpy`/`memmove`/`memset` after checking both ranges.

* **Floats.** No fast-math flags or FMA contraction.  Quoted bit-pattern
  constants (`f0x...`) preserve exact f32/f64 values including NaN payloads.
  Trapping conversions check NaN first (code 3) and the exact representable
  range (code 2) before `fptosi`/`fptoui`; saturating conversions use LLVM's
  `fptosi.sat`/`fptoui.sat` intrinsics.  `fmin`/`fmax` use
  `llvm.minimum`/`maximum` plus a canonical-NaN select, so signalling-NaN
  inputs produce the arithmetic NaN wasm requires; `pmin`/`pmax` follow the
  select-based pseudo-min semantics.  Integer shifts/rotates mask the count;
  `rem_s(INT_MIN, -1)` is special-cased.

* **SIMD.** Vector values are held as `<16 x i8>` and bitcast to lane types.
  Arithmetic uses LLVM vector instructions/intrinsics (`smin`, `uadd.sat`,
  `ctpop`, `fptosi.sat`, `fshl`, shuffles, ...), gathers are lane
  insert/extracts, and `i8x16.swizzle` uses the x86 `pshufb` intrinsic with a
  per-function `+ssse3` attribute.

* **Instantiation.** `w2l_init` allocates memory/tables, resets the depth
  counter and stack budget, applies active element then data segments (their
  bounds are known statically, so an out-of-bounds segment traps at that
  point with code 5/4), and calls the start function.  Active/declarative
  segments are considered dropped afterwards.

## Known limitations

* Imported globals are defined internally as zero (the harness ABI provides no
  symbol for host globals); imported memories/tables are allocated locally
  with the declared limits.  Host functions are called through the slot ABI.
* More than one memory is rejected (multi-memory is out of scope).
* More than 1,000,000 locals is rejected to bound memory use; this exceeds any
  practical program but is below the spec's theoretical limit.
* The native stack budget is 6 MiB per module entry.  Legitimate recursion
  10,000 frames deep works at -O0 and -O2 (frames are small); extremely deep
  recursion of large-frame functions may exhaust the budget and trap with code
  8, which is also what a real implementation would do at its stack limit.
* `i8x16.swizzle` is lowered with `pshufb` and therefore requires SSSE3 (all
  x86-64 CPUs produced since 2006 have it).
* NaN signs/payloads follow wasm's non-determinism (canonical NaNs are used
  where the spec allows a canonical result); tests that compare exact NaN
  payloads of arithmetic results would be non-portable by definition.

## Test setup and results

`tests/harness.c` is a host harness implementing the `ABI.md` symbols and the
`w2l-ref` line protocol (including trap recovery via `longjmp` and a
`SIGSEGV`-to-trap-8 fallback).  Test drivers are in `tests/`.
Everything was compared against `w2l-ref` (wasmtime) unless noted.

* Payloads (`tests/test_payloads.py`): qoa-simd (encode/decode), minicelt
  (mono/stereo encodes at many bitrates/complexities), minisilk, multi-value;
  compared return values, globals, host output, full memory hashes and packet
  bytes.  Pass at both `-O0` and `-O2`.
* Trap/bounds tests (`tests/test_traps.py`): division, overflow, conversion
  traps, memory and table bounds, `call_indirect` empty/type/OOB, bulk
  operations, zero-length edge cases, memory.grow limits, stale-instance
  reuse after traps, 10,000-deep recursion and unbounded recursion (trap 8).
* Numeric tests (`tests/test_numeric.py`): shift/rotate counts, clz/ctz/popcnt,
  sign extension, all conversion boundaries, min/max/copysign with NaN and
  signed zeros, NaN quiet-bit behaviour for scalar and SIMD min/max, and SIMD
  edge cases (narrow/extmul/q15/avgr/dot/bitmask/shuffle/swizzle/lane ops).
* Control tests (`tests/test_control.py`): multi-value calls, block/if/loop
  parameters and multiple results, `br_table` with values and duplicate
  targets, `br_if` values, `br` to the function label, v128 selects,
  `call_indirect` with v128 and multi-result signatures, 16-parameter calls.
* Instantiation tests (`tests/test_init.py`): OOB active data/element traps,
  start-function traps, zero-page memory/grow, segment-before-start ordering,
  empty modules.
* Import tests (`tests/wat/imports.wat`): every provided `env`/`spectest`
  host function, including v128, and `call_indirect` into an imported
  function.
* Rust modules (`tests/test_rust.py`, sources in `tests/rust/`):
  `wasm32v1-none` and `wasm32-unknown-unknown` builds with floats, pointer
  loops, `memory.copy`, recursion, trait objects/`call_indirect`, and SIMD
  intrinsics.  The second build uses non-minimal LEB encodings and reference
  type encodings that the w2l-ref runner refuses to load, so its simple
  exports are instead compared against the `wasmtime` CLI.
* Random modules: several hundred `wasm-tools smith` modules (SIMD, bulk
  memory, multi-value, sign-extension, saturating conversions; canonicalised
  NaNs; fuel-bounded), including computation-focused variants generated with
  traps disallowed, compared function-by-function with random arguments and
  memory hashes at `-O0` and `-O2`.  The generated corpus exercises all 236
  SIMD mnemonics and essentially the whole core instruction set; zero
  mismatches (the only reported difference was a driver artefact after a
  trapping instantiation, where the reference runner refuses further commands).
* Validation: more than 10,000 byte/truncation mutations of valid modules (including the
  payloads and rich fuzz modules); accept/reject decisions compared against
  `wasm-tools validate -f wasm2`.  Zero mismatches; a hand-written corpus of
  invalid modules (duplicate exports, bad start type, segment/table type
  mismatches, const-expression violations, bad alignment, undeclared
  `ref.func`, ...) also agrees with the oracle.  `tests/test_scope.py` checks
  that modules using each rejected proposal above are refused (and that wasm2
  refuses them too).
* `tests/run_all.sh` runs the whole suite (both optimisation levels, fuzzers)
  from a built tree; the harness and drivers are test-only and are not needed
  by the translator.
* Performance: `minicelt_encode` (stereo, 960 samples) runs at about 1.1x the
  wasmtime time and qoa `encode_frame` at about 1.4x (including protocol
  overhead in both cases), measured in the same harness process.
