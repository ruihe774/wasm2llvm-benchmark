# qoa-simd

`qoa-simd.wasm` is the SIMD build of the QOA (Quite OK Audio) lossless audio
codec. It has no imports and exports its memory.

## Exports

| symbol | type | meaning |
|---|---|---|
| `memory` | memory | the module memory (starts at 2 pages) |
| `heap_base` | i32 global (const) | byte offset of the codec's working area |
| `encoder_state` | i32 global | byte offset of the encoder state |
| `encoder_state_size` | i32 global | size of the encoder state in bytes |
| `frame_channels` | mut i32 global | channel count of the last decoded frame |
| `frame_samplerate` | mut i32 global | sample rate of the last decoded frame |
| `frame_samples` | mut i32 global | samples per channel of the last decoded frame |
| `encode_init(i32 channels)` | function | (re)initialise the encoder for 1..31 channels |
| `encode_frame(i32 src, i32 channels, i32 sampleRate, i32 frameLen, i32 dst)` | function -> i32 | encode one frame |
| `decode_frame(i32 src, i32 avail, i32 dst)` | function -> i32 | decode one frame |

The module has a `start` function; instantiate it (the ABI's `w2l_init`) before
calling anything else.

## Encoding

Put `frameLen * channels` interleaved little-endian `i16` samples at `src`
(`frameLen <= 5120`), call `encode_init(channels)` once, then call
`encode_frame(src, channels, sampleRate, frameLen, dst)`. It writes one QOA
frame at `dst` and returns its size in bytes (a positive value), or a negative
error code. No output is written to `stdout`.

The reference TypeScript wrapper places the input at `heap_base` and the output
at `heap_base + align16(input_bytes)`, growing the memory if necessary.

## Decoding

Put an encoded frame at `src` with `avail` readable bytes, and call
`decode_frame(src, avail, dst)`. It writes `frame_samples * frame_channels`
interleaved `i16` samples at `dst`, updates the three `frame_*` globals, and
returns a non-negative value (frame size/consumed bytes) or a negative error
code (`-1` truncated header, `-2` invalid header, `-3` size mismatch, `-4`
truncated frame).

## Testing note

The memory starts zeroed, as required by WebAssembly. A driver can verify the
encoder and decoder by hashing or comparing the memory after the calls; the
exact addresses used by a driver are its own choice as long as they stay inside
the memory and do not overlap live state.
