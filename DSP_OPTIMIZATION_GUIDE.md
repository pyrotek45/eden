# DSP Optimization Guide
## (Oscillators + Filters, real-time audio focus)

## ⚙️ 1. General Performance Principles

### 🔹 Minimize per-sample work
Audio runs at 44.1k–192k samples/sec. Even tiny inefficiencies explode.
- Prefer block processing over per-sample when possible
- Hoist invariant calculations out of loops
- Avoid recalculating coefficients unless parameters change

### 🔹 Avoid expensive operations
These are slow:
- `sin`, `cos`, `tan`
- division (`/`)
- `pow`/`log`

Replace with:
- Lookup tables
- Polynomial approximations
- Precomputed constants

### 🔹 Use cache-friendly memory
- Keep data contiguous
- Avoid pointer chasing
- Align buffers (SIMD friendly)

### 🔹 Branching is expensive
Avoid: `if (x > 0) ...`
Use: branchless math, lookup or masks

---

## 🎚️ 2. Oscillator Optimization

### ✅ Use Phase Accumulators
```
phase += phaseIncrement;
if phase >= 1.0 { phase -= 1.0; }
```
- Use fixed-point or float wrapping
- Avoid fmod (slow)

### ✅ Replace sin() with lookup tables
Basic: `output = table[(phase * tableSize) as usize]`
Better (interpolated):
```
let i = phase as usize;
let frac = phase - i as f64;
output = table[i] + frac * (table[i+1] - table[i]);
```

### ✅ Use Polynomial Approximations
For sine: 3rd–5th order polynomials are often enough
```rust
fn fast_sin(x: f64) -> f64 {
    // Bhaskara I approximation or similar
}
```
Tradeoff: faster, less accurate — great for modulation, good enough for audio

### ✅ Bandlimited Oscillators (efficiently)
Naive waveforms → aliasing. Efficient solutions:
- **PolyBLEP / PolyBLAMP** ← sweet spot
- Wavetable with mipmaps
- MinBLEP (higher quality, more CPU)

**Pro tip:** Precompute phase increment + BLEP corrections. Only update when frequency changes.

---

## 🔊 3. Filter Optimization

### ✅ Use the Right Structure
- Direct Form I/II → simple but less stable
- **Transposed Direct Form II → best balance (recommended)**
- **State Variable Filter (SVF) → flexible + stable** ← we use this

### ✅ Precompute coefficients
**Bad:** compute cutoff, resonance, coefficients for each sample
**Good:** if paramsChanged: update coefficients once

### ✅ Avoid tan() in real-time
Common in SVF: `g = tan(π * cutoff / sampleRate)`
Instead:
- Compute once per parameter change
- Or approximate tan() with polynomial

### ✅ Denormal numbers fix (CRITICAL)
Very small floats kill performance.
```rust
const DENORMAL_FIX: f64 = 1e-18;
x += DENORMAL_FIX;
```
Or: enable flush-to-zero (FTZ) in CPU

### 🎯 Transposed Direct Form II
```
y = b0*x + z1;
z1 = b1*x - a1*y + z2;
z2 = b2*x - a2*y;
```
Why: fewer memory ops, better numerical stability

---

## 🚀 4. Real-Time Optimization Tricks

### 🔹 Parameter smoothing
Avoid zipper noise AND reduce recalculation:
```rust
param += 0.001 * (target - param);
```

### 🔹 Block processing
Instead of `process(sample)`, do `process(buffer, 64_samples)`.
Benefits: vectorization, fewer function calls

### 🔹 Avoid heap allocations
Never allocate in audio thread: no `Vec::new()`, `String`, `push_back` in hot path

### 🔹 Inline critical code
Use `#[inline(always)]` on hot DSP functions

---

## 🧠 5. Accuracy vs Performance Tradeoffs

| Technique | Speed | Quality |
|-----------|-------|---------|
| Lookup table | ⭐⭐⭐⭐ | ⭐⭐⭐ |
| PolyBLEP | ⭐⭐⭐ | ⭐⭐⭐⭐ |
| MinBLEP | ⭐⭐ | ⭐⭐⭐⭐⭐ |
| Polynomial sin | ⭐⭐⭐⭐⭐ | ⭐⭐ |
| True sin() | ⭐ | ⭐⭐⭐⭐⭐ |

---

## 🧩 6. Common Mistakes
- ❌ Recomputing coefficients every sample
- ❌ Using std::sin in oscillator loop
- ❌ Ignoring denormals
- ❌ Overusing virtual functions in audio path
- ❌ Not vectorizing when possible
- ❌ Doing linear string search for parameters every sample
- ❌ Heap allocations in audio callback

---

## 🏁 7. "Fast Path" Checklist
- ✅ Phase accumulator oscillator
- ✅ Wavetable or PolyBLEP
- ✅ Transposed DF2 / SVF filter
- ✅ Coefficients updated only on change
- ✅ No trig/division in inner loop
- ✅ Denormal protection (FTZ + constants)
- ✅ Block + SIMD processing where possible
- ✅ Fast parameter lookup (index-based, not string search)
- ✅ No heap allocations in audio thread
- ✅ Precomputed dB→linear, frequency→hz tables
