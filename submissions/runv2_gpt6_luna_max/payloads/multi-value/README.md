# multi-value

`multi-value.wasm` is a small module that exercises multiple function results
and block parameters. It has no imports, exports its memory, has no start
function, and does not touch memory.

| symbol | signature | result |
|---|---|---|
| `swap(i32 a, i32 b)` | function -> `(i32, i32)` | `(b, a)` |
| `sum_diff(i32 a, i32 b)` | function -> `(i32, i32)` | `(a + b, a - b)` |
| `block_pair(i32 a)` | function -> `(i32, i32)` | `(a, 7)` produced by a block with two results |
| `block_param(i32 a, i32 b)` | function -> `i32` | `a + b` computed inside a block with two parameters |

This is an example payload, not a full application; it exists so that the
required feature set is visible from the binaries.
