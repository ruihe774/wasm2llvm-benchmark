# Payloads

These are the modules your translator must handle. Together they define the
required WebAssembly feature scope: support every proposal they use, in full
(every instruction of each proposal, not just the instructions that happen to
appear in these binaries), and reject modules that use anything outside it.

There is no list of proposals. Discover them from the binaries and from the
specifications in `../docs/`. `wasm-tools print`, `wasm-tools validate`,
`wasm-tools dump` and `wasm-tools objdump` are available; `wasm-tools smith`
can generate further modules.

| payload | binary | what it is |
|---|---|---|
| `qoa-simd` | `qoa-simd/qoa-simd.wasm` | QOA audio codec (encode/decode), SIMD build |
| `minicelt` | `minicelt/minicelt.wasm` | CELT-only Opus encoder, 48 kHz |
| `minisilk` | `minisilk/minisilk.wasm` | SILK-only Opus encoder, 16 kHz |
| `multi-value` | `multi-value/multi-value.wasm` | small example of multiple results and block parameters |

Each directory has a `README.md` describing the exported interface (entry
points, memory layout, expected calls). The interface descriptions are there so
you can drive the payloads with your own tests; they are not a test suite.

The translator is graded on hidden modules of the same kinds (these payloads
with different inputs and additional builds, more Rust programs, spec tests and
randomly generated modules). Do not modify anything under `payloads/`.
