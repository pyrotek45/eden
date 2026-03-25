# Audio Engine Notes

## Architecture

- Pull-based audio graph
- Lock-free callback: atomic `Arc` snapshot swap (`AudioSnapshot`)
- MIDI events via `ringbuf` ring buffer
- No `Mutex` inside callback
- All scratch buffers pre-allocated before closure, reused with `.clear()` / `.resize()`
- Fixed-time DSP: O(1) or O(buffer_size) per callback

## Phase Tracking

- Phase accumulator wraps via subtraction: `phase = next - next.floor()`
- Never use `phase %= 1.0` (float precision drift)
- PolyBLEP on saw, square, all pulse widths
- Triangle is inherently continuous

## Parameter Smoothing

- One-pole low-pass on pan, volume, all FX params, CStrip2 params, master volume
- `coeff = 0.002` per sample (~0.7 ms @ 44.1 kHz)
- Per-callback: `coeff_buf = (0.002 * frames).min(1.0)`
- `smooth += (target - smooth) * coeff`
- Smoothed values stored in pre-allocated `Vec<Vec<Vec<(String, f32)>>>` — strings allocated once

## Clip Edge Fades

- 220 samples (~5 ms @ 44.1 kHz) micro-fade at clip start/end
- Equal-power sine curve: `gain = sin(t * π/2)`
- User-specified fade-in/fade-out on audio clips applied on top

## Seek / Loop Anti-Click

- 220-sample equal-power sine fade-in on transport jump or loop wrap
- `anti_click_remaining` counter, decremented per sample

## Denormal Protection

- `mix_l += 1e-24; mix_r += 1e-24;` on master output
- Inaudible at -480 dB

## DC Offset Removal

- One-pole HP filter on master bus, fc ≈ 20 Hz
- `y[n] = x[n] - x[n-1] + R * y[n-1]`, R = 0.99972

## Slew Rate Limiter

- Max per-sample change = 0.8 on master output
- Catches runaway spikes, transparent to normal audio

## Soft Clipper

- `tanh(x * drive) / drive` before final clamp
- drive = 1.0: linear below ±0.7, gentle saturation above ±1.0

## Export (render.rs)

- Same DSP code path as real-time playback
- Fixed 1024-sample block size
- 2 ms fade-in, 5 ms fade-out on master output
- 2-second tail beyond last clip (reverb/delay decay)
- Interleaved L R output for WAV

## Metering

- Peak: instant attack, release `0.9997^frames`
- RMS: `sum_sq / frame_count` per callback
- Thread-safe: `AtomicU32` with `f32::to_bits` / `from_bits`

## Control Rate vs Audio Rate

- Control rate: per-callback (~86 Hz at 512 samples/buffer)
- Audio rate: per-sample (44100 Hz)
- One-pole interpolation bridges the two rates
- UI → snapshot → one-pole smooth → DSP

## Hot Path Rules

- No `Vec::push` (may reallocate)
- No `Arc::new`, `Box::new`, `Mutex::lock`
- No `format!`, string allocation
- Use `.clear()` + `.resize()` on pre-allocated buffers
- Stack buffers for small scratch data

## CStrip2 Channel Strip (per track)

- Runs after effect chain, before pan/volume
- 10 parameters: Treble, Mid, Bass, TrebFreq, BassFreq, LoCap, HiCap, Compress, CompSpd, Output
- Defaults at neutral (no coloring): all EQ at 0.5, LoCap=1.0, HiCap=0.0, Compress=0.0, Output=0.33
- 6-pole RC hi-pass + lo-pass (capacitor filters)
- 3-band Triplet EQ (IIR first-order crossover)
- ButterComp dual-rail compressor
- Spiral soft-clip output saturation
- All params smoothed per-callback (same one-pole as FX params)

## Status

| Protection | Status | Location |
|---|---|---|
| Micro-fades at clip edges | Done | audio.rs, render.rs |
| Pan/volume smoothing | Done | audio.rs |
| FX param smoothing | Done | audio.rs |
| CStrip2 param smoothing | Done | audio.rs |
| Master volume smoothing | Done | audio.rs |
| Lock-free audio thread | Done | audio.rs |
| Double-buffered DSP graph | Done | audio.rs |
| Continuous oscillator phase | Done | modules.rs |
| PolyBLEP bandlimiting | Done | modules.rs |
| Tail rendering on export | Done | render.rs |
| Denormal protection | Done | audio.rs, render.rs |
| DC-offset HP filter | Done | audio.rs, render.rs |
| Slew rate limiter | Done | audio.rs |
| Soft clipper (tanh) | Done | audio.rs, render.rs |
| Anti-click seek fade | Done | audio.rs |
| Export fade-in/out | Done | render.rs |
| Pre-allocation | Done | audio.rs |
| Dither on 16-bit export | TODO | render.rs |
| Lookahead limiter | TODO | — |
| Crossfades between clips | TODO | — |
| SIMD mix loop | TODO | audio.rs |
| Sample-accurate scheduling | TODO | audio.rs |
| True peak metering | TODO | — |
