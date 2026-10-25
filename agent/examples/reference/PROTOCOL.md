# Reference runner protocol

`w2l-ref` is a wasmtime-based reference runner for WebAssembly modules. It is on
`PATH` (in `deps/bin`) and reads one command per line on stdin, answering with
exactly one line on stdout. It is useful for running the payloads in
`payloads/`, and any `.wasm` modules you write, to obtain reference results.

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
| `memwrite_fn <id> <ptrfn-hexname> <mem-hexname> <hexbytes>` | `ok` / `trap <code>` / `error <msg>` |
| `memwrite_global <id> <global-hexname> <mem-hexname> <hexbytes>` | `ok` / `error <msg>` |
| `output <id>` | `ok <hex>` host output since the last `output` (`-` if none) |
| `ping` | `ok` |
| `quit` | (exits) |

* `<hexname>`: lowercase hex of the UTF-8 export name; `-` for the empty name.
* `<slot>`: 32 hex digits = the 16 slot bytes in memory order (little-endian
  values, see `../../ABI.md`).
* Trap codes: `../../ABI.md`.
* `memwrite_fn` calls a no-argument exported function and uses its i32 result as
  the byte offset to write to; `memwrite_global` uses an exported i32 global.
  Both operate on the named exported memory. Out-of-bounds host writes are
  driver errors (`error`), not wasm traps.
* Host output formats: `i32:<signed decimal>\n`, `i64:<signed decimal>\n`,
  `f32:0x<8 hex digits of bits>\n`, `f64:0x<16 hex digits>\n`,
  `v128:<32 hex digits, byte order>\n`, `print\n`; `putchar` writes one raw byte.

`wasmtime` itself is also available as a general WebAssembly engine.

The payload `README.md` files describe their interfaces. For example, to run
the `minicelt` encoder you would `load` its `.wasm`, `init` it, call
`minicelt_init` with `invoke`, write samples with `memwrite_fn` to the pointer
returned by `minicelt_input_ptr`, call `minicelt_encode`, and inspect the result
with `invoke`/`memread`/`memhash`.
