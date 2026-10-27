# wasm2llvm notes

## Design

The translator uses a standard-library-only Rust binary parser, a typed operand-stack and control-flow validator, and textual LLVM IR emission. It emits typed internal Wasm functions, 16-byte operand-stack slots, typed local allocas, explicit control-flow labels, and ABI wrappers with lowercase-hex export/import names. The generated runtime allocates wasm32 linear memory, applies active segments, runs the start function from `w2l_init`, bounds-checks memory and table accesses, implements bulk memory operations, and calls `w2l_trap` with the ABI trap codes. SIMD operations are lowered to LLVM vector operations. Scalar float-to-int saturating conversions use LLVM saturating-conversion intrinsics; no fast-math flags are emitted.

## Scope and limitations

Implemented scope includes multi-value functions and block signatures, fixed-width SIMD, scalar non-trapping float-to-int conversions, sign-extension operators, and the memory/table operations in the bulk-memory scope (`memory.init`, `data.drop`, `memory.copy`, `memory.fill`, `table.init`, `elem.drop`, and `table.copy`). Branch-hint custom metadata is ignored.

The translator rejects features outside that scope, including reference-type values/instructions, `table.grow`/`table.size`/`table.fill`, typed select, imported memories/tables/globals, memory64, multiple memories/tables, threads, relaxed SIMD, exceptions, tail calls, and extended constant expressions. Host imports are emitted using the ABI import wrapper; the host must provide each referenced import symbol.

## Verification

- `cargo build --release --offline` succeeded with both the normal and an isolated clean `CARGO_HOME`/target directory.
- Clang 23.1.4 linked all four supplied payloads at `-O0` and `-O2` with the ABI linker flags.
- At both optimization levels, ABI checks passed for five multi-value cases; `w2l-ref` comparisons passed for QOA patterned encode/decode and CELT/SILK i16 and f32 encodes.
- Focused SIMD reference tests passed at both optimization levels for high-lane extension/extmul, signed/unsigned narrowing, i64 shifts, float comparisons, lane loads/stores, and `v128.load32_zero` at the final four bytes of memory.
- Focused memory bulk tests passed for active/passive data, `memory.init`/`data.drop`, overlapping `memory.copy`, `memory.fill`, `memory.grow`, and zero-length boundary accesses. Table tests passed for passive `table.init`, `elem.drop`, overlapping `table.copy`, and indirect calls.
- The ABI trap harness passed at both optimization levels: trap codes 0–8, 10,000-frame recursion, stack-exhaustion trapping at 20,000 frames, and continued instance use after traps. `table.init`/`table.copy` bounds and dropped-segment traps also returned code 5 with successful recovery.
- Malformed-module checks rejected truncated headers/sections, malformed LEB128, invalid stack/type indices, mismatched function/code counts, and unknown opcodes. Out-of-scope checks rejected `table.size`, imported memory, and reference-type parameters.
- These are focused regression checks, not an exhaustive WebAssembly conformance or fuzzing suite. The Rust build still reports unused/dead-code warnings.
