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
⚙️ 1. Rust-Specific Foundations
🔹 Zero-cost abstractions (use them, but verify)

Rust can compile to C-level performance—but only if:

You avoid hidden allocations
You let LLVM optimize

Always check:

cargo asm
cargo build --release
🔹 Use #[inline(always)] aggressively (hot paths)
#[inline(always)]
fn process_sample(x: f32) -> f32 {
    x * 0.5
}

⚠️ Overuse can hurt instruction cache—profile it.

🔹 Prefer stack over heap

Bad:

let buffer = Vec::new(); // allocation

Good:

let buffer = [0.0f32; 64];
🔹 Avoid bounds checks in hot loops

Rust inserts bounds checks unless optimized away.

Use:

for i in 0..buffer.len() {
    unsafe {
        *buffer.get_unchecked_mut(i) *= 0.5;
    }
}

Or better:

for x in buffer.iter_mut() {
    *x *= 0.5;
}

👉 Iterators often optimize better than indexing.

🚀 2. Oscillator Optimization (Rust)
🔹 Phase accumulator (branchless wrap)
phase += phase_inc;
phase -= (phase >= 1.0) as i32 as f32;

✔ avoids branch
✔ compiles to SIMD-friendly code

🔹 Use f32::mul_add (FMA)
phase = phase.mul_add(phase_inc, 0.0);

✔ single instruction on modern CPUs
✔ reduces rounding error

🔹 Wavetable with interpolation
let idx = phase * table_len as f32;
let i = idx as usize;
let frac = idx - i as f32;

let a = table[i];
let b = table[(i + 1) & (table_len - 1)];

let out = a + frac * (b - a);
🔥 Trick: power-of-two table
let mask = table_len - 1;
let i = idx as usize & mask;

✔ removes modulo
✔ huge speedup

🔹 PolyBLEP (branchless version)

Instead of:

if t < dt { ... }

Use:

let t = phase;
let dt = phase_inc;

let x = t / dt;
let correction = (x + x - x * x - 1.0).max(0.0);

✔ reduces branching penalties

🔹 Cache phase increment

Only update when frequency changes:

if freq != prev_freq {
    phase_inc = freq * inv_sample_rate;
}
🔊 3. Filter Optimization (Rust)
🔹 Transposed Direct Form II (fastest baseline)
#[inline(always)]
fn process(&mut self, x: f32) -> f32 {
    let y = self.b0 * x + self.z1;
    self.z1 = self.b1 * x - self.a1 * y + self.z2;
    self.z2 = self.b2 * x - self.a2 * y;
    y
}

✔ minimal memory ops
✔ great for SIMD

🔹 Precompute EVERYTHING
fn update_coeffs(&mut self) {
    let g = (std::f32::consts::PI * self.cutoff * self.inv_sr).tan();
    // compute once
}

Never in sample loop.

🔹 Fast tan approximation

Instead of:

g = tan(x);

Use:

fn fast_tan(x: f32) -> f32 {
    x + 0.3333 * x * x * x
}

✔ huge speed gain
⚠️ only safe for small x

🔹 Denormal killer (Rust-safe)
#[inline(always)]
fn fix_denorm(x: f32) -> f32 {
    x + 1e-18 - 1e-18
}

Or globally:

unsafe {
    std::arch::x86_64::_mm_setcsr(
        std::arch::x86_64::_mm_getcsr() | 0x8040
    );
}

✔ enables FTZ + DAZ

⚡ 4. SIMD & Vectorization (Rust)
🔹 Use std::simd (modern Rust)
use std::simd::f32x4;

let a = f32x4::from_array([1.0, 2.0, 3.0, 4.0]);
let b = f32x4::splat(0.5);

let c = a * b;

✔ portable SIMD
✔ auto-vectorization backup

🔹 Process blocks of 4–8 samples

Instead of scalar loop:

for i in 0..n {
    out[i] = process(in[i]);
}

Do SIMD batches.

🔹 Align memory
#[repr(align(16))]
struct AlignedBuffer([f32; 64]);

✔ avoids SIMD penalties

🧠 5. CPU-Level “Crazy” Tricks
🔹 Branch prediction hacking

Replace:

if x > 0.0 { a } else { b }

With:

let mask = (x > 0.0) as i32 as f32;
let out = mask * a + (1.0 - mask) * b;

✔ eliminates branch misprediction

🔹 Loop unrolling
for i in (0..n).step_by(4) {
    process(i);
    process(i+1);
    process(i+2);
    process(i+3);
}

✔ better instruction pipelining

🔹 Avoid false sharing (multithreading)
#[repr(align(64))]
struct VoiceState {
    data: f32
}

✔ prevents cache line contention

🔹 Pre-fetching (rare but powerful)
unsafe {
    core::arch::x86_64::_mm_prefetch(ptr as *const i8, _MM_HINT_T0);
}

✔ useful for large buffers

🧪 6. Memory & Data Layout Tricks
🔹 Structure of Arrays (SoA)

Bad:

struct Voice {
    phase: f32,
    freq: f32,
}

Better:

struct Voices {
    phase: Vec<f32>,
    freq: Vec<f32>,
}

✔ SIMD-friendly
✔ cache efficient

🔹 Avoid virtual dispatch

Bad:

trait Filter {
    fn process(&mut self, x: f32) -> f32;
}

Better:

enums
generics

✔ eliminates vtable lookup

🔥 7. Extreme Tricks (Expert Only)
🔹 Fast inverse sqrt (yes, still useful)
fn fast_inv_sqrt(x: f32) -> f32 {
    let x2 = x * 0.5;
    let mut y = x;
    let mut i = y.to_bits();
    i = 0x5f3759df - (i >> 1);
    y = f32::from_bits(i);
    y * (1.5 - x2 * y * y)
}
🔹 Phase as integer (fixed-point oscillator)
let phase: u32;
phase = phase.wrapping_add(phase_inc);

✔ zero floating-point drift
✔ super fast

🔹 Compile-time LUT generation
const fn generate_table() -> [f32; 1024] {
    let mut table = [0.0; 1024];
    let mut i = 0;
    while i < 1024 {
        table[i] = (i as f32).sin();
        i += 1;
    }
    table
}

✔ zero runtime cost

🔹 Profile-guided optimization (PGO)
RUSTFLAGS="-Cprofile-generate" cargo build

Then:

RUSTFLAGS="-Cprofile-use" cargo build

✔ massive real-world gains

🧯 8. Real-Time Audio Safety (CRITICAL)
🚫 Never do in audio thread:
allocation
locking (Mutex)
file I/O
logging
✅ Use lock-free structures
use crossbeam::queue::ArrayQueue;
✅ Double buffering parameters
📊 9. Measurement & Profiling

Tools:

cargo flamegraph
perf
valgrind
criterion (benchmarks)
🏁 Final “Insane Optimization” Checklist

If you want max performance:

✅ phase accumulator (fixed-point optional)
✅ LUT with interpolation + power-of-two size
✅ PolyBLEP (branchless)
✅ transposed DF2 filters
✅ SIMD (std::simd)
✅ no bounds checks in hot loops
✅ SoA memory layout
✅ denormals disabled
✅ no branches in DSP core
✅ PGO build