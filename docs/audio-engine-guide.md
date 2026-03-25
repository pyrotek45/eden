# Eden Audio Engine — Click-Free Audio Reference Guide

> Last updated: 2026-03-24  
> Source: `src/audio.rs`, `src/render.rs`, `src/modules.rs`

This document is the definitive internal reference for how Eden prevents audio
artifacts (clicks, pops, zipper noise, aliasing, DC bias, denormals) at every
level of the audio pipeline.  It is structured the same way a pro DAW audio
engine team would think about it: from the sample level up to the architecture
level.

---

## 0. The Big Picture — How Pro DAWs Think

DAWs don't "fix clicks" after the fact — they **prevent discontinuities
everywhere**:

| Level           | What can go wrong              | Fix strategy                      |
|-----------------|-------------------------------|-----------------------------------|
| Signal (sample) | Waveform discontinuities       | PolyBLEP, micro-fades, soft clip  |
| Control (param) | Zipper noise from steps        | One-pole smoothing, ramp-interp   |
| Graph (DSP)     | Mid-buffer graph changes       | Double-buffered snapshot          |
| Time (schedule) | Buffer underruns, sync drift   | Lock-free RT, fixed block size    |

Key insight: **no single fix changes the sound. Layering many tiny protections
eliminates all artifacts.**

---

## 1. Hard Real-Time Safety (100 % Transparent)

These never affect the audio signal — they only prevent glitches.

### ✅ Lock-Free Audio Thread

Eden's audio callback is completely lock-free in the hot path.

- UI → Audio communication: atomic `Arc` snapshot swap (`AudioSnapshot`)
- Ring buffers for MIDI events (`ringbuf` crate)
- **No `Mutex` inside the callback**

```rust
// audio.rs — snapshot swap (outside hot path)
let snap = Arc::clone(&audio_snap.load());
```

### ✅ Pre-Allocation

Every buffer used in the callback is allocated **once before the closure** and
reused with `.clear()` / `.resize_with()`:

```rust
// audio.rs (outside closure)
let mut cb_pending_preview: Vec<(usize, u8, u8)> = Vec::with_capacity(16);
let mut per_track_sample: Vec<(f64, f64)> = Vec::with_capacity(32);
// ... many more pre-alloc'd scratch buffers
```

**No `Vec::push` that reallocates inside the callback hot path.**

### ✅ Fixed-Time DSP

All DSP algorithms inside the callback run in O(1) or O(buffer_size) time.
No dynamic-length searches, growing arrays, or variable-time algorithms.

---

## 2. Sample-Accurate Continuity

### ✅ Continuous Phase Tracking (Synths)

Oscillator phases are never reset unless intentional. The phase accumulator
wraps via subtraction, not truncation:

```rust
// modules.rs — correct phase accumulation
let next = phase + inc;
phases[i] = next - next.floor();   // wraps cleanly, no discontinuity
```

Never do:
```rust
phase %= 1.0;  // can drift due to float precision ❌
```

### ✅ PolyBLEP Bandlimited Oscillators

All non-sine waveforms use PolyBLEP (Polynomial Band-Limited Step) correction
to eliminate aliasing and discontinuities at waveform edges.

```rust
// modules.rs — polyblep() function
fn polyblep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

// Applied to sawtooth:
let naive = 2.0 * phase - 1.0;
let saw = naive - polyblep(phase, inc.abs().max(1e-12));

// Applied to square:
let mut s = if phase < 0.5 { 1.0 } else { -1.0 };
s += polyblep(phase, dt);
s -= polyblep((phase + 0.5) % 1.0, dt);
```

Shapes covered: **Saw, Square, all pulse widths, Impulse**.  
Triangle is inherently continuous (no step edges) so it doesn't need BLEP.

---

## 3. Parameter Smoothing Everywhere

### ✅ Per-Track Pan Smoothing (One-Pole)

Pan changes from automation or UI sliders are smoothed with a one-pole
low-pass filter (~1–5 ms time constant).  This eliminates zipper noise.

```rust
// audio.rs — outside closure (pre-allocated)
let mut smooth_pan: Vec<f64> = Vec::new();

// audio.rs — inside mix loop (per sample)
const SMOOTH_COEFF: f64 = 0.002;  // ≈ 1 ms @ 44.1kHz
smooth_pan[ti] += (track.pan as f64 - smooth_pan[ti]) * SMOOTH_COEFF;
let theta = (smooth_pan[ti] + 1.0) * 0.5 * FRAC_PI_2;
let pan_l = fast_cos(theta);
let pan_r = fast_sin(theta);
```

### ✅ Per-Track Volume Smoothing (One-Pole)

Track fader moves are also smoothed to prevent gain step artifacts:

```rust
// audio.rs — outside closure
let mut smooth_vol: Vec<f64> = Vec::new();

// inside mix loop
smooth_vol[ti] += (track.volume as f64 - smooth_vol[ti]) * SMOOTH_COEFF;
mix_l += tl * pan_l * smooth_vol[ti];
mix_r += tr * pan_r * smooth_vol[ti];
```

### Time Constant Formula

```
SMOOTH_COEFF = 1 - e^(-2π·fc / sr)

For fc = 200 Hz @ 44100 Hz:
SMOOTH_COEFF ≈ 0.028   (fast, ~1.5 ms)

For fc = 70 Hz @ 44100 Hz:
SMOOTH_COEFF ≈ 0.010   (safe, ~5 ms)

Eden uses 0.002 (≈ 0.7 ms — very fast, inaudible lag)
```

---

## 4. Dezippering (Parameter Steps → Ramps)

Zipper noise occurs when a parameter jumps by a discrete step in one sample.

**Prevention:**  
- Use `smooth_pan` / `smooth_vol` one-pole filters (see §3)
- For coarser block-rate updates, ramp across the buffer:

```rust
// Dezipper across buffer (if computing at block rate)
let step = (target_vol - current_vol) / buffer_size as f64;
for i in 0..buffer_size {
    current_vol += step;
    out[i] *= current_vol;
}
```

---

## 5. Lookahead (Future Use)

Lookahead is used in limiters and automation.  It delays audio by a small
amount (1–10 ms) and processes control signals ahead of time, eliminating
sudden jumps.

Eden does not yet implement lookahead.  Candidate locations:
- Master limiter (prevent transient clipping)
- Clip launch scheduling (sample-accurate event alignment)

```rust
// Future: lookahead ring buffer
let lookahead_samples = (0.005 * sample_rate as f64) as usize; // 5 ms
```

---

## 6. Smart Clip Boundaries

### ✅ Micro-Fades at Clip Edges (~5 ms = 220 samples @ 44.1kHz)

Every audio clip fades in and out with an **equal-power sine curve**.  This is
applied in both `audio.rs` (playback) and `render.rs` (export).

```rust
// audio.rs / render.rs — clip edge fades
let fade_len = 220usize;  // ≈ 5 ms @ 44.1kHz

// Fade-in at clip start:
if clip_sample < fade_len {
    let t = clip_sample as f64 / fade_len as f64;
    s *= (t * FRAC_PI_2).sin();  // equal-power: sin(0)..sin(π/2)
}

// Fade-out at clip end:
let remaining = clip_len_samples.saturating_sub(clip_sample);
if remaining < fade_len {
    let t = remaining as f64 / fade_len as f64;
    s *= (t * FRAC_PI_2).sin();
}
```

**Why equal-power (sin) and not linear?**

| Fade type    | Energy in fade region | Perceived loudness |
|-------------|----------------------|-------------------|
| Linear       | 1/3 of peak           | Drops during fade  |
| Equal-power  | 1/2 of peak (= -3 dB) | Stays constant     |

### ✅ User-Specified Fade-In / Fade-Out

Audio clips also support user-specified fade-in/out times (stored in
`AudioClip.fade_in` and `AudioClip.fade_out`), applied on top of the
micro-fades.

### Future: Crossfades Between Adjacent Clips

Instead of clip A → hard cut → clip B, apply a crossfade:

```
A ──────────────────╲
                     ╲  (equal-power fade-out)
                      ╲__
                     /
                    /  (equal-power fade-in)
B ─────────────────/──────────────────────────
```

This requires detecting adjacent clips with overlapping end/start times.

---

## 7. Graph Update Safety (Double-Buffered Snapshot)

Changing the DSP graph mid-playback causes clicks.

### ✅ Eden's Approach: Atomic Snapshot Swap

```rust
// audio.rs — AudioSnapshot is an Arc'd immutable value
// UI builds a NEW snapshot, then atomically replaces the old one.
// The audio thread always reads from a stable, immutable snapshot.
audio_snap.store(Arc::new(new_snapshot));
```

The audio callback never holds a mutable reference to the project state.
Graph topology changes (adding/removing tracks/effects) are applied between
callback invocations, not mid-buffer.

---

## 8. Buffer-Level Tricks

### ✅ Pre-Allocated Scratch Buffers (No RT Allocation)

```rust
// audio.rs — declared outside closure
let mut frame_samples_l = vec![0.0f32; MAX_FRAME_SIZE];
let mut frame_samples_r = vec![0.0f32; MAX_FRAME_SIZE];
let mut per_track_sample: Vec<(f64, f64)> = Vec::with_capacity(32);
```

### Future: Guard Samples / Overlap-Add

For convolution reverb and time-stretching effects, process slightly beyond
buffer edges to avoid boundary discontinuities.  This requires an
overlap-add accumulation buffer.

---

## 9. Silence Handling

### ✅ Denormal Prevention

The CPU enters a slow-path for denormal (subnormal) floating-point numbers
(values between 0 and ~1e-38).  Adding a tiny constant prevents this.

```rust
// audio.rs + render.rs — on master output, every sample
mix_l += 1.0e-24;
mix_r += 1.0e-24;
```

At -480 dB this is completely inaudible.

### ✅ Silence Detection (Early Exit)

Tracks with no active voices and no audio clips at the current position are
skipped without processing.  Effects with tails (`has_tail()`) are kept alive
until they decay.

---

## 10. Transient Protection

### ✅ Slew Rate Limiter (Safety Net)

A slew limiter caps the maximum per-sample change in the master output.  This
catches extreme transients that slip past all other protections.  The threshold
is generous enough to pass all legitimate audio (e.g., sharp drum transients)
while catching runaway spikes.

```rust
// audio.rs — applied on master output
const SLEW_MAX: f64 = 0.8;   // max change per sample (generous safety net)
let delta_l = (mix_l - prev_mix_l).clamp(-SLEW_MAX, SLEW_MAX);
mix_l = prev_mix_l + delta_l;
prev_mix_l = mix_l;
let delta_r = (mix_r - prev_mix_r).clamp(-SLEW_MAX, SLEW_MAX);
mix_r = prev_mix_r + delta_r;
prev_mix_r = mix_r;
```

> ⚠️ Set `SLEW_MAX` very high (≥ 0.5) so it only catches runaway spikes —
> not normal fast transients like snare hits.

### ✅ Soft Clipper (Tanh)

Before the final clamp, a gentle tanh saturator prevents hard digital
clipping while keeping headroom.

```rust
// audio.rs / render.rs — just before clamp(-1, 1)
// Drive = 1.0 → transparent below 0 dBFS, gentle saturation above
const SOFT_CLIP_DRIVE: f64 = 1.0;
mix_l = (mix_l * SOFT_CLIP_DRIVE).tanh() / SOFT_CLIP_DRIVE;
mix_r = (mix_r * SOFT_CLIP_DRIVE).tanh() / SOFT_CLIP_DRIVE;
```

At drive = 1.0: signals below ±0.7 are perfectly linear; above ±1.0 the
curve gently bends toward ±1.0 instead of hard-clipping.

---

## 11. Numerical Stability

### ✅ DC-Offset Removal (One-Pole HP Filter, fc ≈ 20 Hz)

Any slowly drifting bias is removed by a high-pass filter on the master bus.
It has no effect on audible content (20 Hz cutoff).

```rust
// audio.rs + render.rs — applied on every master sample
const DC_HP_R: f64 = 0.99972;   // R = 1 − 2π·fc/sr,  fc ≈ 20 Hz @ 44.1kHz

let new_l = mix_l - dc_hp_x_l + DC_HP_R * dc_hp_y_l;
dc_hp_x_l = mix_l;
dc_hp_y_l = new_l;
mix_l = new_l;
// (same for R channel)
```

### Filter Design

```
One-pole HPF:  y[n] = x[n] − x[n−1] + R·y[n−1]
Transfer fn:   H(z) = (1 − z⁻¹) / (1 − R·z⁻¹)
Cutoff:        fc = (1−R)·sr / (2π)  →  R = 1 − 2π·fc/sr
```

At R = 0.99972:  fc ≈ 20 Hz @ 44.1 kHz → inaudible, pure DC removal.

### ✅ Stable Filter Coefficients

All IIR filters (HP, LP, reverb feedback) are designed with coefficients
that guarantee stability (poles strictly inside the unit circle).  The
one-pole HP with R < 1.0 is unconditionally stable.

### ✅ Float Precision

- Inner DSP loops use `f64` to avoid precision loss in long accumulators
- Final output converts to `f32` only at the last step (device output)
- No unnecessary `f32 ↔ f64` conversions inside loops

---

## 12. Export Engine

### ✅ Render Fade-In / Fade-Out

Exported WAV files have a 2 ms fade-in and 5 ms fade-out applied to the
master output.  This prevents pops if the file is played back immediately
after another file in a playlist.

```rust
// render.rs — export master processing
let fade_in_samples  = (0.002 * sample_rate as f64) as usize;  // ~88 samples
let fade_out_samples = (0.005 * sample_rate as f64) as usize;  // ~220 samples

if _si < fade_in_samples {
    let t    = _si as f64 / fade_in_samples as f64;
    let gain = (t * FRAC_PI_2).sin();
    mix_l *= gain; mix_r *= gain;
} else if _si >= total_samples.saturating_sub(fade_out_samples) {
    let dist = total_samples - _si;
    let t    = dist as f64 / fade_out_samples as f64;
    let gain = (t * FRAC_PI_2).sin();
    mix_l *= gain; mix_r *= gain;
}
```

### ✅ Tail Rendering

The export pipeline renders an extra **2 seconds of silence tail** beyond the
last clip end.  This captures reverb/delay tails that would otherwise be
cut off.

```rust
// render.rs — extend total_samples by tail
const TAIL_SECS: f64 = 2.0;
let tail_samples = (TAIL_SECS * sample_rate as f64) as usize;
let total_samples = base_samples + tail_samples;
```

### ✅ Correct Interleaving

WAV output is interleaved `L R L R L R …` as required by the WAV format:

```rust
writer.write_sample(mix_l_i16)?;
writer.write_sample(mix_r_i16)?;
```

### Future: Dither

When exporting to 16-bit, add TPDF dither (two rectangular PDF noise) to
avoid quantization distortion at low levels:

```rust
// TPDF dither: two uniform random values in [-0.5 LSB, +0.5 LSB]
let dither = (rng.next() as f64 / i32::MAX as f64)
           - (rng.next() as f64 / i32::MAX as f64);
let sample_int = (sample_f64 * 32767.0 + dither) as i16;
```

---

## 13. Anti-Click Safety Nets (Last Resort)

### ✅ Click Detector + Repair (Future)

Detect spikes in post-processing:

```rust
if (sample - prev_sample).abs() > CLICK_THRESHOLD {
    // Replace with interpolated value
    sample = prev_sample + (next_sample - prev_sample) * 0.5;
}
```

### ✅ Soft Clipper

See §10.  Applied before final hard clamp.

### ✅ Anti-Click Fade at Seek / Loop Boundaries

When transport jumps (seek, loop wrap), a 220-sample (~5 ms) equal-power
sine fade-in is applied to prevent the discontinuity from being heard:

```rust
// audio.rs — triggered on seek / loop reset
anti_click_remaining = ANTI_CLICK_SAMPLES;  // 220

// per-sample in playing branch:
if anti_click_remaining > 0 {
    let t    = 1.0 - (anti_click_remaining as f64 / ANTI_CLICK_SAMPLES as f64);
    let gain = (t * FRAC_PI_2).sin();
    mix_l *= gain;
    mix_r *= gain;
    anti_click_remaining -= 1;
}
```

---

## 14. Architecture-Level Tricks

### ✅ Pull-Based Audio Graph

Eden's audio graph is pull-based: the callback pulls samples from the graph
top-down rather than pushing audio bottom-up.  This ensures correct ordering
and synchronisation.

### ✅ Sample-Accurate MIDI Scheduling

MIDI events are timestamped at exact sample indices within the buffer, not
just at buffer boundaries.  This prevents timing jitter on note-on/off.

### ✅ Fixed Block Size Internally

Even if the audio device requests variable buffer sizes, Eden processes
audio in a consistent internal block size.

### ✅ Double-Buffered DSP Graph

See §7.  UI writes to a new `AudioSnapshot`; audio reads from the current
immutable one atomically.

---

## 15. Rust-Specific Best Practices

### ✅ Lock-Free Structures

- `atomic::AtomicU64` for transport position (beat counter)
- `arc_swap::ArcSwap` or similar for the audio snapshot
- `ringbuf` for MIDI event ring buffers

### ✅ Avoid in Hot Path

```rust
// Never do these in the audio callback:
vec.push(x);           // may reallocate
Arc::new(x);           // heap allocation
Mutex::lock();         // can block
Box::new(x);           // heap allocation
format!("...", ...);   // allocation + formatting
```

### ✅ Prefer in Hot Path

```rust
// Pre-allocated, reused every callback:
vec.clear();            // O(1), no allocation
vec.resize(n, 0.0);     // only allocates if n > capacity

// Stack buffers for small scratch data:
let mut scratch = [0.0f64; 64];
```

### ✅ SIMD (Future Optimization)

The mix loop (N tracks × M samples) is the hottest code path.  Using
`std::simd` or the `wide` crate to process 4/8 samples at once would
cut CPU usage by ~4×:

```rust
// Future: process 4 samples at once with f64x4
use std::simd::f64x4;
let chunk = f64x4::from_slice(&input[i..]);
let result = chunk * gain_splat;
result.copy_to_slice(&mut output[i..]);
```

---

## Quick Reference: "The Pro DAW Stack" for Eden

Minimum set needed for professional-quality audio output:

| Protection              | Status  | File         | Notes                             |
|------------------------|---------|--------------|-----------------------------------|
| Micro-fades at clip edges | ✅ Done | audio.rs, render.rs | 220 samples, equal-power sine |
| Parameter smoothing (pan) | ✅ Done | audio.rs     | One-pole, coeff=0.002             |
| Parameter smoothing (vol) | ✅ Done | audio.rs     | One-pole, coeff=0.002             |
| Lock-free audio thread    | ✅ Done | audio.rs     | ArcSwap snapshot                  |
| Double-buffered DSP graph | ✅ Done | audio.rs     | AudioSnapshot pattern             |
| Continuous oscillator phase | ✅ Done | modules.rs  | `next - next.floor()`             |
| PolyBLEP bandlimiting     | ✅ Done | modules.rs   | Saw, Square, all pulse widths     |
| Tail rendering on export  | ✅ Done | render.rs    | +2 s beyond last clip             |
| Denormal protection       | ✅ Done | audio.rs, render.rs | `+= 1e-24`              |
| DC-offset HP filter       | ✅ Done | audio.rs, render.rs | fc ≈ 20 Hz               |
| Slew rate limiter         | ✅ Done | audio.rs     | max delta = 0.8/sample            |
| Soft clipper (tanh)       | ✅ Done | audio.rs, render.rs | drive=1.0, transparent  |
| Anti-click seek fade      | ✅ Done | audio.rs     | 220 samples, equal-power          |
| Export fade-in/out        | ✅ Done | render.rs    | 2 ms / 5 ms                       |
| Pre-allocation (no RT alloc) | ✅ Done | audio.rs  | All scratch buffers pre-alloc'd   |
| **Dither on 16-bit export** | ⬜ TODO | render.rs  | TPDF dither for CD-quality output |
| **Lookahead limiter**       | ⬜ TODO | render.rs  | 5 ms lookahead on master bus      |
| **Crossfades between clips** | ⬜ TODO | render.rs | Overlap-add crossfade engine      |
| **SIMD mix loop**           | ⬜ TODO | audio.rs   | 4× throughput with f64x4/f32x8    |

---

## What Actually Causes 90% of Clicks

In real engines, it's almost always one of:

| Cause                     | Prevention                        |
|--------------------------|-----------------------------------|
| Parameter jumps (no smooth) | One-pole filter on every control |
| Graph changes mid-buffer  | Atomic snapshot swap              |
| Buffer underruns          | Lock-free callback, no blocking   |
| No envelopes on note on/off | ADSR on every voice             |
| Hard clip edges           | 220-sample micro-fades            |
| Phase resets              | Continuous phase accumulator      |
| Denormal CPU spikes       | `+= 1e-24` on every output        |

---

*This document should be updated whenever a new protection is added to the engine.*
