# minicelt

`minicelt.wasm` is a small CELT-only Opus encoder: one static 48 kHz, 20 ms
encoder for 1 or 2 channels. It has no imports and exports its memory.

## Exports

| symbol | type | meaning |
|---|---|---|
| `memory` | memory | the module memory |
| `minicelt_init(i32 channels, i32 bitrate, i32 complexity, i32 vbr_constraint, i32 dtx)` | function -> i32 | (re)initialise; returns 0 or a negative Opus error code |
| `minicelt_input_ptr()` | function -> i32 | address of the f32 input buffer |
| `minicelt_input_i16_ptr()` | function -> i32 | address of the same buffer, viewed as i16 |
| `minicelt_output_ptr()` | function -> i32 | address of the packet output buffer (capacity 1276 bytes) |
| `minicelt_encode()` | function -> i32 | encode 960 * channels interleaved f32 samples |
| `minicelt_encode_i16()` | function -> i32 | encode 960 * channels interleaved i16 samples |
| `minicelt_set_bitrate(i32)` | function -> i32 | OPUS_SET_BITRATE |
| `minicelt_set_complexity(i32)` | function -> i32 | OPUS_SET_COMPLEXITY |
| `minicelt_set_vbr_constraint(i32)` | function -> i32 | OPUS_SET_VBR_CONSTRAINT |
| `minicelt_set_dtx(i32)` | function -> i32 | OPUS_SET_DTX |
| `minicelt_set_packet_loss_perc(i32)` | function -> i32 | OPUS_SET_PACKET_LOSS_PERC |
| `minicelt_set_bandwidth(i32)` | function -> i32 | OPUS_SET_BANDWIDTH |
| `minicelt_lookahead()` | function -> i32 | OPUS_GET_LOOKAHEAD (120 samples at 48 kHz) |
| `minicelt_final_range()` | function -> i32 | OPUS_GET_FINAL_RANGE |
| `minicelt_in_dtx()` | function -> i32 | OPUS_GET_IN_DTX |

`bitrate` is bits/s, `-1000` (`OPUS_AUTO`) or `-1` (`OPUS_BITRATE_MAX`). VBR is
always on. `channels` is 1 or 2.

## Driving it

1. Call `minicelt_init(channels, bitrate, complexity, vbr_constraint, dtx)`; it
   returns 0 on success.
2. Write `960 * channels` interleaved samples to `minicelt_input_ptr()` (f32,
   nominal range ±1) or to `minicelt_input_i16_ptr()` (i16).
3. Call `minicelt_encode()` / `minicelt_encode_i16()`. The return value is the
   packet length in bytes (the packet is at `minicelt_output_ptr()`, capacity
   1276), or a negative Opus error code.
4. Both input pointers are the same buffer; the encoder works in it, so it must
   be rewritten before every call.

The module exports no start function; call `minicelt_init` after
instantiation. The memory starts zeroed.
