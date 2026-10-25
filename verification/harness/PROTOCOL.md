# Harness protocol

Both `w2l_harness` (native: loads translated `.so` modules) and `w2l-ref`
(reference: loads `.wasm` modules into wasmtime) read one command per line on
stdin and answer with exactly one line on stdout. `w2l.py` drives them; you
normally don't need to speak the protocol yourself, but it is handy for
debugging:

```
clang -O2 -shared -fPIC -Wl,-Bsymbolic -o m.so m.ll -lm
printf 'load a ./m.so\ninit a\ninvoke a 616464 1 01000000000000000000000000000000 02000000000000000000000000000000\n' \
  | .w2l-work/bin/w2l_harness          # built by w2l.py on first use
```

| command | reply |
|---|---|
| `load <id> <path>` | `ok` / `error <msg>` (native: `dlopen` of the `.so`; ref: compile the `.wasm`) |
| `init <id>` | `ok` / `trap <code>` (calls `w2l_init`) |
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
  values, see ABI.md).
* Trap codes: ABI.md.
* `memwrite_global` uses an exported i32 global as the byte offset. To write at a
  pointer returned by an export, call it with `invoke`, then use `memwrite` at
  the returned offset (the `w2l.py` test action `memwrite_fn` does exactly this
  after checking the export's signature). Out-of-bounds host writes are driver
  errors (`error`), not wasm traps.
* Host output formats: `i32:<signed decimal>\n`, `i64:<signed decimal>\n`,
  `f32:0x<8 hex digits of bits>\n`, `f64:0x<16 hex digits>\n`,
  `v128:<32 hex digits, byte order>\n`, `print\n`; `putchar` writes one raw byte.

The native harness runs all wasm code on a dedicated thread with an 8 MiB
stack and a 64 MiB `PROT_NONE` guard region below it; a SIGSEGV inside the guard
region while wasm code is running is reported as `trap 8`. Any other crash kills
the harness; `w2l.py` then records a failure, restarts the harness and replays
the state-changing commands of the script so far before continuing.
