# minisilk

`minisilk.wasm` is a small SILK-only Opus encoder: one static 16 kHz mono,
20 ms encoder. It has no imports and exports its memory.

## Exports

| symbol | type | meaning |
|---|---|---|
| `memory` | memory | the module memory |
| `minisilk_init(i32 bitrate, i32 complexity, i32 dtx)` | function -> i32 | (re)initialise; returns 0 or a negative Opus error code |
| `minisilk_input_ptr()` | function -> i32 | address of the f32 input buffer |
| `minisilk_input_i16_ptr()` | function -> i32 | address of the i16 input buffer |
| `minisilk_output_ptr()` | function -> i32 | address of the packet output buffer (capacity 1276 bytes) |
| `minisilk_encode()` | function -> i32 | encode 320 interleaved f32 samples |
| `minisilk_encode_i16()` | function -> i32 | encode 320 interleaved i16 samples |
| `minisilk_set_bitrate(i32)` | function -> i32 | OPUS_SET_BITRATE |
| `minisilk_set_complexity(i32)` | function -> i32 | OPUS_SET_COMPLEXITY |
| `minisilk_set_dtx(i32)` | function -> i32 | OPUS_SET_DTX |
| `minisilk_set_packet_loss_perc(i32)` | function -> i32 | OPUS_SET_PACKET_LOSS_PERC |
| `minisilk_lookahead()` | function -> i32 | OPUS_GET_LOOKAHEAD (samples at 16 kHz) |
| `minisilk_final_range()` | function -> i32 | OPUS_GET_FINAL_RANGE |
| `minisilk_in_dtx()` | function -> i32 | OPUS_GET_IN_DTX |

`bitrate` is bits/s, `-1000` (`OPUS_AUTO`) or `-1` (`OPUS_BITRATE_MAX`).

## Driving it

1. Call `minisilk_init(bitrate, complexity, dtx)`; it returns 0 on success.
2. Write 320 samples (one channel) to `minisilk_input_ptr()` (f32, nominal range
   ±1) or to `minisilk_input_i16_ptr()` (i16).
3. Call `minisilk_encode()` / `minisilk_encode_i16()`. The return value is the
   packet length in bytes (the packet is at `minisilk_output_ptr()`, capacity
   1276), or a negative Opus error code.

The module exports no start function; call `minisilk_init` after
instantiation. The memory starts zeroed.
