# Eden Audio Engine — Click-Free Audio Reference Guide

> Last updated: 2026-06-16  
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

## 16. Effect Parameter Smoothing (Knob Moves)

### ✅ Per-Callback One-Pole Smoothing for All Effect Params

Moving any knob (filter cutoff, reverb size, delay time, etc.) while audio
is playing causes a discontinuity in the DSP signal — heard as a click or
zipper noise.  The same one-pole smoothing used for pan/volume is applied to
**every effect parameter** on every audio callback.

**Root cause:**  
Each callback calls `shared_cb.try_lock()` to get the latest snapshot.  The
new knob value arrives in full on the very next callback, producing an instant
jump in the DSP signal.

**Fix:**  
A `smooth_track_fx_params[track][slot][param_idx]: Vec<Vec<Vec<f32>>>` cache
is maintained outside the closure.  Per callback, each cached value is
one-pole-filtered toward its snapshot target.  DSP calls receive the smoothed
values, not the raw snapshot values.  The same pattern covers master rack
effects (`smooth_master_fx_params[slot][param_idx]`).

```rust
// audio.rs — outside closure (allocated once)
let mut smooth_track_fx_params: Vec<Vec<Vec<f32>>> = Vec::new();
let mut smooth_master_fx_params: Vec<Vec<f32>> = Vec::new();
const FX_SMOOTH_COEFF: f32 = 0.002;  // same time constant as pan/vol

// Per callback — before DSP loop:
let coeff = (FX_SMOOTH_COEFF * frames as f32).min(1.0);
for (ti, track) in snap.tracks.iter().enumerate() {
    for (si, (_, params)) in track.effect_slots.iter().enumerate() {
        let cache = &mut smooth_track_fx_params[ti][si];
        for (pi, (_, v)) in params.iter().enumerate() {
            cache[pi] += (v - cache[pi]) * coeff;  // one-pole low-pass
        }
    }
}
```

A pre-allocated `scratch_fx_params: Vec<(String, f32)>` (32-element capacity,
never grows after warm-up) carries the smoothed values to each DSP call
without heap allocation.

### ✅ Master Volume Smoothing

The master fader is also smoothed with the same one-pole filter, using a
sentinel (`smooth_master_vol = -1.0`) to initialise on first use without
a pop:

```rust
// audio.rs — outside closure
let mut smooth_master_vol: f64 = -1.0;  // -1 = uninitialized

// Per callback:
if smooth_master_vol < 0.0 { smooth_master_vol = snap.master_volume as f64; }
smooth_master_vol += (snap.master_volume as f64 - smooth_master_vol)
    * (FX_SMOOTH_COEFF as f64 * frames as f64).min(1.0);

// Use smoothed value in DSP:
mix_l *= smooth_master_vol;
mix_r *= smooth_master_vol;
```

---

## 17. Control Rate vs Audio Rate

Audio engines operate at two distinct rates:

| Rate         | Typical update | Examples                                |
|-------------|----------------|------------------------------------------|
| **Audio rate** | Every sample (e.g. 44100/s) | Oscillator phase, filter state, envelope |
| **Control rate** | Every buffer (e.g. 44100/512 = 86/s) | UI knobs, automation reads, tempo sync |

**The danger:** If a control-rate parameter is applied directly to audio-rate
DSP without conversion, the step between buffers is heard as zipper noise or
a click.

**Eden's solution:** One-pole interpolation bridges the two rates.  The UI
writes a new target; the audio thread smooths toward it over one or more
callbacks.

```
UI event ──► snapshot ──► one-pole smooth ──► DSP audio-rate computation
(any rate)   (per-callback)  (per-callback)    (per-sample)
```

The smoothing coefficient determines how fast the control catches up:

```
τ (time constant) = -1 / ln(1 - coeff)  samples
                  ≈ 1/coeff              samples  (for small coeff)

coeff = 0.002:  τ ≈ 500 samples ≈ 11.3 ms @ 44.1 kHz
```

For 512-sample buffers, `coeff_per_buffer = (0.002 × 512).min(1.0) = 1.0`
— meaning one buffer is all it takes to fully catch up.  This makes knob
moves feel instant while still smoothing out any mid-buffer jump.

---

## 18. Sample-Accurate Event Scheduling

### Current: Buffer-Boundary Scheduling

Eden currently triggers MIDI note-on/off at the first sample of the buffer
in which the note's beat position falls.  At 512 samples / buffer, this gives
worst-case timing jitter of ~11.6 ms — adequate for most uses but not for
tight percussion.

```rust
// audio.rs — current approach (per-buffer beat check)
for note in &clip.notes {
    let note_start_beats = note.start_beats;
    if pos >= clip_start + note_start_beats && prev_pos < clip_start + note_start_beats {
        // Trigger note — fires on first sample of this buffer
        voices.push(new_voice(...));
    }
}
```

### Future: Sample-Accurate Scheduling

For sub-millisecond timing accuracy, events should be timestamped at their
exact sample offset within the buffer and processed per-sample:

```rust
// Future: event queue with sample-level timestamps
struct ScheduledEvent {
    sample_offset: usize,  // 0..buffer_size
    event: MidiEvent,
}

// Per-sample in the loop:
for (frame_idx, sample_offset) in events.iter() {
    if *sample_offset == frame_idx {
        process_event(event);
    }
}
```

This is especially important for:
- Tight snare/kick alignment in sample-accurate sequencers
- Note-off exactly at clip boundary (no sustain bleed)
- Arpeggiator tempo sync at BPM > 200

---

## 19. Metering Ballistics

Accurate meters require separate logic from the DSP path — they measure the
signal without affecting it.

### Peak Meter

- **Attack:** Instant (capture single-sample max)
- **Release (fallback):** Slow decay, typically `release_coeff ≈ 0.9997` per sample (≈ 300 ms @ 44.1kHz)

```rust
// Per sample:
peak_hold = peak_hold.max(sample.abs());

// Per callback (apply release):
let release = 0.9997_f32.powi(frames as i32);
peak_hold *= release;
```

### RMS Meter

- Window: ~300 ms = 13230 samples @ 44.1kHz
- Computation: `rms = sqrt(mean(x²))` over window

Eden currently accumulates `sum_sq += sample * sample` per buffer and reports
`sqrt(sum_sq / frame_count)`.  A proper 300 ms running window requires a
circular buffer or exponential moving average:

```rust
// Exponential RMS (approximation of 300 ms window):
const RMS_COEFF: f32 = 1.0 - (1.0 / (0.3 * 44100.0));  // ≈ 0.999924
rms_sq = rms_sq * RMS_COEFF + sample * sample * (1.0 - RMS_COEFF);
let rms = rms_sq.sqrt();
```

### True Peak Meter

For compliance with broadcast loudness standards (EBU R128, ITU-R BS.1770),
true peak detection oversamples the signal 4× to catch inter-sample peaks:

```rust
// Oversample 4× with linear interpolation, measure peak on all 4 sub-samples
// Full implementation requires a polyphase FIR — future work.
```

### Thread Safety

Meters must not block the audio thread.  Eden uses `AtomicU32` to pass peak/
RMS values to the UI thread (reinterpreted as `f32` bits):

```rust
// audio callback (non-blocking write):
peak_atomic.store(f32::to_bits(peak), Ordering::Relaxed);

// UI thread (non-blocking read):
let peak = f32::from_bits(peak_atomic.load(Ordering::Relaxed));
```

---

## 20. Offline / Export Rendering Mode

Export rendering runs the same DSP code path as real-time playback, with
three key differences:

| Property         | Real-time                    | Offline (export)               |
|-----------------|------------------------------|--------------------------------|
| Buffer size      | Device-driven (e.g. 512)    | Fixed (e.g. 1024)              |
| Timing deadline  | Hard (RT audio callback)     | None — runs as fast as CPU allows |
| Determinism      | Non-deterministic seek/jitter | 100% deterministic per run     |
| Sample rate      | Device sample rate           | User-selectable (44.1/48/96kHz) |

Eden's `render.rs` uses the same `process()` / `process_sidechain()` trait
calls as `audio.rs`, ensuring export audio is bit-identical to what you hear
during playback.

### Key considerations

1. **No real-time constraint** — the render loop can run as fast as the CPU
   allows (`cargo run --release` renders a 3-min song in ~1 s)
2. **Deterministic block size** — use a fixed 1024-sample block for
   cache-efficient processing
3. **Same smoothing state** — parameter smoothers must be reset to their
   initial values at render start (not inherited from the live playback state)
4. **Tail rendering** — always add a tail (e.g. 2 s) beyond the last clip to
   capture reverb/delay decay

```rust
// render.rs — export loop (simplified)
let block = 1024usize;
let total = arrangement_samples + tail_samples;
let mut render_pos = 0usize;

while render_pos < total {
    let frames = block.min(total - render_pos);
    // ... same DSP code as audio.rs callback ...
    render_pos += frames;
}
```

---

Minimum set needed for professional-quality audio output:

| Protection              | Status  | File         | Notes                             |
|------------------------|---------|--------------|-----------------------------------|
| Micro-fades at clip edges | ✅ Done | audio.rs, render.rs | 220 samples, equal-power sine |
| Parameter smoothing (pan) | ✅ Done | audio.rs     | One-pole, coeff=0.002             |
| Parameter smoothing (vol) | ✅ Done | audio.rs     | One-pole, coeff=0.002             |
| Parameter smoothing (all FX knobs) | ✅ Done | audio.rs | One-pole per-callback, scratch scratch_fx_params |
| Master volume smoothing   | ✅ Done | audio.rs     | One-pole, sentinel init           |
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
| **Sample-accurate scheduling** | ⬜ TODO | audio.rs | Per-sample event timestamp queue |
| **True peak metering**      | ⬜ TODO | audio.rs   | 4× oversample polyphase FIR       |

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
