# Reference runner protocol

`w2l-ref` is a wasmtime-based reference runner for WebAssembly modules. It is on
`PATH` and reads one command per line on stdin, answering with
exactly one line on stdout. It is useful for running the payloads in
`payloads/`, and any `.wasm` modules you write, to obtain reference results.

## Feature scope

`w2l-ref` is built with exactly the benchmark's WebAssembly feature set. It
therefore rejects at `load` time modules that use proposals outside the scope
defined by the payloads — typically with an error ending in
`... support is not enabled`, or with an encoding error for a construct that is
only defined under a broader feature set. Such a failure means the module is
outside the benchmark's scope, not that the runner is broken.

The defaults of the other tools are broader: `wasm-tools validate -f wasm2`,
`wasm-tools validate --features all`, and `wasm-tools smith` without explicit
`--*-enabled false` flags accept or generate constructs beyond the benchmark's
scope. Their verdicts are not the benchmark's scope oracle.

```
printf 'load a ./payloads/multi-value/multi-value.wasm\ninit a\ninvoke a 73776170 2 01000000000000000000000000000000 02000000000000000000000000000000\n' \
  | w2l-ref
```

| command | reply |
|---|---|
| `load <id> <path>` | `ok` / `error <msg>` (loads a `.wasm` file) |
| `init <id>` | `ok` / `trap <code>` (instantiates and runs the start function) |
| `invoke <id> <hexname> <nresults> <slot>...` | `ok <slot>...` / `trap <code>` / `error <msg>` |
| `global <id> <hexname>` | `ok <slot>` |
| `memhash <id> <hexname>` | `ok <size-in-bytes> <fnv1a64-hex>` of the exported memory |
| `memwrite <id> <hexname> <offset> <hexbytes>` | `ok` / `error <msg>` |
| `memread <id> <hexname> <offset> <len>` | `ok <hexbytes>` / `error <msg>` |
| `memwrite_global <id> <global-hexname> <mem-hexname> <hexbytes>` | `ok` / `error <msg>` |
| `output <id>` | `ok <hex>` host output since the last `output` (`-` if none) |
| `ping` | `ok` |
| `quit` | (exits) |

* `<hexname>`: lowercase hex of the UTF-8 export name; `-` for the empty name.
* `<slot>`: 32 hex digits = the 16 slot bytes in memory order (little-endian
  values, see `../../ABI.md`).
* Trap codes: `../../ABI.md`.
* `memwrite_global` uses an exported i32 global as the byte offset. To write at a
  pointer returned by an export, call the export with `invoke`, then write at
  the returned offset with `memwrite`. Out-of-bounds host writes are driver
  errors (`error`), not wasm traps.
* Host output formats: `i32:<signed decimal>\n`, `i64:<signed decimal>\n`,
  `f32:0x<8 hex digits of bits>\n`, `f64:0x<16 hex digits>\n`,
  `v128:<32 hex digits, byte order>\n`, `print\n`; `putchar` writes one raw byte.

`wasmtime` itself is also available as a general WebAssembly engine.

The payload `README.md` files describe their interfaces. For example, to run
the `minicelt` encoder you would `load` its `.wasm`, `init` it, call
`minicelt_init` with `invoke`, obtain the input address with
`invoke a <hexname(minicelt_input_ptr)> 1`, write samples at that offset with
`memwrite`, call `minicelt_encode`, and inspect the result with
`invoke`/`memread`/`memhash`.
