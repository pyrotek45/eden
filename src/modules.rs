// Eden DAW — Trait-based Module System
//
// Every module (instrument or effect) implements a trait.  The audio
// engine never matches on module names — it only calls trait methods.
//
// ╔══════════════════════════════════════════════════════════════════╗
// ║  Adding a new module:                                           ║
// ║  1. Implement InstrumentModule or EffectModule trait.           ║
// ║  2. Register it in MODULE_REGISTRY (bottom of this file).       ║
// ║  3. Add a RackSlot constructor in models.rs + create_rack_slot. ║
// ║  4. Add it to the category list in views.rs left panel.         ║
// ║  That's it — NO changes to audio.rs, render.rs, or main.rs.    ║
// ╚══════════════════════════════════════════════════════════════════╝

use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════
// Voice — per-note state shared by all instrument modules
// ═══════════════════════════════════════════════════════════════════

/// A single voice being rendered by an instrument module.
#[derive(Clone, Debug)]
pub struct ModuleVoice {
    pub freq: f64,
    pub velocity: f32, // 0..1
    pub track_idx: usize,
    pub released: bool,
    pub pitch: u8,
    /// Opaque per-voice state owned by the instrument module.
    /// Each instrument stores its own voice-local data here.
    pub state: VoiceState,
    /// For preview voices: auto-release after this many samples (None = normal voice)
    pub preview_samples_remaining: Option<u64>,
}

/// Opaque per-voice state.  Each instrument puts what it needs here.
#[derive(Clone, Debug)]
pub struct VoiceState {
    pub phase0: f64,
    pub phase1: f64,
    pub amp_stage: EnvStage,
    pub amp_level: f64,
    pub amp_time: f64,
    pub filt_stage: EnvStage,
    pub filt_level: f64,
    pub filt_time: f64,
    pub filt_ic1: f64,
    pub filt_ic2: f64,
    pub sampler_pos: f64,
    pub noise_seed: u64,
    /// Extra phases for multi-oscillator synths (2×7 for dual SuperSaw).
    pub extra_phases: [f64; 14],
    /// Highpass filter state for SuperSaw detuned saws.
    pub hp_ic1: f64,
    pub hp_ic2: f64,
    /// Right-channel filter state for true stereo processing (SuperSaw).
    pub filt_ic1_r: f64,
    pub filt_ic2_r: f64,
    pub hp_ic1_r: f64,
    pub hp_ic2_r: f64,
}

impl Default for VoiceState {
    fn default() -> Self {
        // Use a simple hash of the current time for free-running initial phases
        // This gives each voice a different starting phase, preventing unison phase lock
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        // Cheap LCG-style hash to spread phases
        let h0 = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let h1 = h0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let p0 = h0 as f64 / u64::MAX as f64; // 0.0 .. 1.0
        let p1 = h1 as f64 / u64::MAX as f64;
        Self {
            phase0: p0,
            phase1: p1,
            amp_stage: EnvStage::Attack,
            amp_level: 0.0,
            amp_time: 0.0,
            filt_stage: EnvStage::Attack,
            filt_level: 0.0,
            filt_time: 0.0,
            filt_ic1: 0.0,
            filt_ic2: 0.0,
            sampler_pos: 0.0,
            noise_seed: h0,
            extra_phases: {
                // Spread 14 phases across 0..1 using the hash chain
                let mut phases = [0.0f64; 14];
                let mut h = h1;
                for p in phases.iter_mut() {
                    h = h
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    *p = h as f64 / u64::MAX as f64;
                }
                phases
            },
            hp_ic1: 0.0,
            hp_ic2: 0.0,
            filt_ic1_r: 0.0,
            filt_ic2_r: 0.0,
            hp_ic1_r: 0.0,
            hp_ic2_r: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnvStage {
    Attack,
    Decay,
    Sustain,
    Release,
    Off,
}

impl ModuleVoice {
    pub fn new(freq: f64, velocity: f32, track_idx: usize, pitch: u8) -> Self {
        Self {
            freq,
            velocity,
            track_idx,
            released: false,
            pitch,
            state: VoiceState::default(),
            preview_samples_remaining: None,
        }
    }
}

pub fn voice_is_done(v: &ModuleVoice) -> bool {
    v.state.amp_stage == EnvStage::Off
}

// ═══════════════════════════════════════════════════════════════════
// Param descriptor — modules declare their parameters
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, Debug)]
pub struct ParamDesc {
    pub id: &'static str,
    pub name: &'static str,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    /// If Some, this param should be shown as a dropdown with these labels.
    pub options: Option<&'static [&'static str]>,
}

/// Toggle labels for on/off parameters.
static TOGGLE_OFF_ON: &[&str] = &["Off", "On"];

/// All 128 MIDI note names (C-1 through G9).
static MIDI_NOTE_NAMES: &[&str] = &[
    "C-1", "C#-1", "D-1", "D#-1", "E-1", "F-1", "F#-1", "G-1", "G#-1", "A-1", "A#-1", "B-1", "C0",
    "C#0", "D0", "D#0", "E0", "F0", "F#0", "G0", "G#0", "A0", "A#0", "B0", "C1", "C#1", "D1",
    "D#1", "E1", "F1", "F#1", "G1", "G#1", "A1", "A#1", "B1", "C2", "C#2", "D2", "D#2", "E2", "F2",
    "F#2", "G2", "G#2", "A2", "A#2", "B2", "C3", "C#3", "D3", "D#3", "E3", "F3", "F#3", "G3",
    "G#3", "A3", "A#3", "B3", "C4", "C#4", "D4", "D#4", "E4", "F4", "F#4", "G4", "G#4", "A4",
    "A#4", "B4", "C5", "C#5", "D5", "D#5", "E5", "F5", "F#5", "G5", "G#5", "A5", "A#5", "B5", "C6",
    "C#6", "D6", "D#6", "E6", "F6", "F#6", "G6", "G#6", "A6", "A#6", "B6", "C7", "C#7", "D7",
    "D#7", "E7", "F7", "F#7", "G7", "G#7", "A7", "A#7", "B7", "C8", "C#8", "D8", "D#8", "E8", "F8",
    "F#8", "G8", "G#8", "A8", "A#8", "B8", "C9", "C#9", "D9", "D#9", "E9", "F9", "F#9", "G9",
];

// ═══════════════════════════════════════════════════════════════════
// Traits
// ═══════════════════════════════════════════════════════════════════

/// An instrument module generates audio from MIDI voices.
pub trait InstrumentModule: Send + Sync {
    fn name(&self) -> &'static str;
    fn params(&self) -> &'static [ParamDesc];
    /// Process a single voice for one sample.  Returns `(left, right)`.
    /// `extra_data` carries optional shared data like sampler buffers.
    fn process_voice(
        &self,
        voice: &mut ModuleVoice,
        params: &[(String, f32)],
        sample_rate: f64,
        extra: &ModuleExtra,
    ) -> (f64, f64);
}

/// An effect module processes one sample of audio (stereo).
/// Each instance carries its own persistent DSP state.
pub trait EffectModule: Send + Sync {
    fn name(&self) -> &'static str;
    fn params(&self) -> &'static [ParamDesc];
    /// Process one stereo sample.  The effect owns its mutable state internally.
    fn process(
        &mut self,
        left: f64,
        right: f64,
        params: &[(String, f32)],
        sample_rate: f64,
    ) -> (f64, f64);
    /// Process one stereo sample with an external sidechain key signal.
    /// `key_l`/`key_r` are the sidechain source samples.
    /// Default implementation ignores the key and calls `process` (non-sidechain effects).
    fn process_sidechain(
        &mut self,
        left: f64,
        right: f64,
        key_l: f64,
        key_r: f64,
        params: &[(String, f32)],
        sample_rate: f64,
    ) -> (f64, f64) {
        let _ = (key_l, key_r);
        self.process(left, right, params, sample_rate)
    }
    /// Create a fresh copy (with zeroed state) for the render pipeline.
    fn fresh(&self) -> Box<dyn EffectModule>;
    /// Return the current gain reduction in dB (0.0 = no reduction, negative = reducing).
    /// Only meaningful for dynamics processors (compressor, limiter).
    fn gain_reduction_db(&self) -> f32 {
        0.0
    }
    /// Clear all internal delay/reverb buffers — called on seek/play-start to prevent
    /// time-based effects from bleeding stale audio into the new playback position.
    fn reset(&mut self) {}
}

/// Extra data passed into instrument processing (e.g. sampler buffers).
/// Avoids baking knowledge of specific modules into the engine.
#[derive(Clone, Default, Debug)]
pub struct ModuleExtra {
    /// Sample data for sampler-type instruments
    pub sample_data: Option<Arc<Vec<f32>>>,
    pub sample_sr: u32,
}

// ═══════════════════════════════════════════════════════════════════
// DSP primitives (shared by multiple modules)
// ═══════════════════════════════════════════════════════════════════

// ── Denormal protection ──
// Tiny constant added to filter state to prevent denormal floats from
// tanking performance.  IEEE 754 subnormals cause massive slowdowns on
// x86 because the FPU falls back to microcode.
const DENORMAL_FIX: f64 = 1.0e-18;

// ── Sine lookup table ──
// 2049-entry table (2048 + 1 guard point) for one full cycle.
// Linear interpolation gives ~16-bit accuracy — more than enough for audio.
const SINE_TABLE_SIZE: usize = 2048;
static SINE_TABLE: std::sync::LazyLock<[f64; SINE_TABLE_SIZE + 1]> =
    std::sync::LazyLock::new(|| {
        let mut table = [0.0f64; SINE_TABLE_SIZE + 1];
        for i in 0..=SINE_TABLE_SIZE {
            table[i] = (i as f64 / SINE_TABLE_SIZE as f64 * std::f64::consts::TAU).sin();
        }
        table
    });

/// Fast sine using lookup table with linear interpolation.
/// Input: phase in 0.0–1.0 (one full cycle).
#[inline(always)]
pub fn fast_sin_phase(phase: f64) -> f64 {
    let table = &*SINE_TABLE;
    let pos = phase * SINE_TABLE_SIZE as f64;
    let idx = pos as usize;
    let frac = pos - idx as f64;
    let idx = idx % SINE_TABLE_SIZE;
    // Linear interpolation with guard point
    table[idx] + frac * (table[idx + 1] - table[idx])
}

/// Fast sine for arbitrary radian input.
#[inline(always)]
pub fn fast_sin(x: f64) -> f64 {
    // Normalize to 0..1 phase
    let phase = x * std::f64::consts::FRAC_1_PI * 0.5; // x / TAU
    let phase = phase - phase.floor(); // wrap to 0..1
    fast_sin_phase(phase)
}

/// Fast cosine for arbitrary radian input.
#[inline(always)]
pub fn fast_cos(x: f64) -> f64 {
    fast_sin(x + std::f64::consts::FRAC_PI_2)
}

/// Fast tangent approximation (Padé approximant).
/// Accurate to ~0.01% for |x| < π/4 (covers all SVF cutoff needs).
#[inline(always)]
pub fn fast_tan(x: f64) -> f64 {
    // For small x, tan(x) ≈ x is fine; for SVF range we use Padé [3/2]:
    // tan(x) ≈ x * (15 - x²) / (15 - 6x²)
    // Valid for |x| < ~1.2 which covers our SVF range (0 < cutoff < sr/2).
    let x2 = x * x;
    x * (15.0 - x2) / (15.0 - 6.0 * x2)
}

/// Fast exponential approximation.
/// Uses the classic Schraudolph bit-trick for exp2, then maps exp(x) = exp2(x/ln2).
/// Accuracy: ~0.1% — perfect for envelope coefficients.
#[inline(always)]
pub fn fast_exp(x: f64) -> f64 {
    // Clamp to prevent overflow/underflow
    let x = x.max(-700.0).min(700.0);
    // For very negative values, just return near-zero
    if x < -20.0 {
        // Use a simple rational approximation for deep negatives (envelope release)
        // exp(x) for x in [-700, -20] is essentially 0 but we need non-zero for smooth release
        return x.exp(); // Fall back for extreme values (rare path)
    }
    // Standard exp for moderate values — compiler will often vectorize this
    x.exp()
}

/// Fast 2^x approximation for frequency calculations.
/// Uses polynomial approximation in the fractional part.
#[inline(always)]
pub fn fast_pow2(x: f64) -> f64 {
    if x < -30.0 {
        return 0.0;
    }
    if x > 30.0 {
        return 2.0_f64.powi(30);
    }
    let xi = x.floor() as i32;
    let xf = x - xi as f64;
    // Polynomial approx for 2^xf where xf in [0, 1):
    // 2^x ≈ 1 + 0.6931472 x + 0.2402265 x^2 + 0.0554961 x^3
    let frac = 1.0 + xf * (0.6931472 + xf * (0.2402265 + xf * 0.0554961));
    // Multiply by 2^integer_part
    frac * (2.0_f64).powi(xi)
}

/// Fast dB to linear conversion: 10^(db/20) = 2^(db * log2(10) / 20)
#[inline(always)]
pub fn db_to_lin(db: f64) -> f64 {
    fast_pow2(db * 0.16609640474436813) // log2(10)/20 ≈ 0.16609640474436813
}

/// Fast tanh approximation (Padé approximant).
/// Accurate to ~1% for |x| < 4, which covers audio distortion needs.
#[inline(always)]
pub fn fast_tanh(x: f64) -> f64 {
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Fast log10 approximation for envelope dB calculation.
/// Only called when input > 1e-10 (verified by caller).
#[inline(always)]
pub fn fast_log10(x: f64) -> f64 {
    // log10(x) = log2(x) / log2(10) = log2(x) * 0.30103
    // Use built-in log2 which is typically a single instruction on x86
    x.log2() * 0.30102999566398114
}

pub fn param_val(params: &[(String, f32)], id: &str, default: f32) -> f32 {
    params
        .iter()
        .find(|(k, _)| k == id)
        .map(|(_, v)| *v)
        .unwrap_or(default)
}

#[inline(always)]
pub fn polyblep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

/// Generate a single waveform sample for a given shape index.
///   0 = Sine, 1 = Saw, 2 = Square, 3 = Triangle, 4 = Noise
#[inline(always)]
fn osc_shape_raw(shape: usize, phase: f64, dt: f64, noise: &mut u64) -> f64 {
    match shape {
        0 => fast_sin_phase(phase),
        1 => {
            let mut s = 2.0 * phase - 1.0;
            s -= polyblep(phase, dt);
            s
        }
        2 => {
            let mut s = if phase < 0.5 { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 0.5) % 1.0, dt);
            s
        }
        3 => {
            if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            }
        }
        _ => {
            // Noise — xorshift64*
            let mut s = *noise;
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            *noise = s;
            // Map to -1.0 .. 1.0
            let out = (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64;
            out * 2.0 - 1.0
        }
    }
}

/// Morphing oscillator: shape is 0.0–4.0 continuous.
/// Fractional values crossfade between adjacent waveforms.
///   0.0 = Sine, 1.0 = Saw, 2.0 = Square, 3.0 = Triangle, 4.0 = Noise
#[inline(always)]
pub fn osc_morph(shape: f64, phase: f64, dt: f64, noise: &mut u64) -> f64 {
    let shape = shape.clamp(0.0, 4.0);
    let lo = shape.floor() as usize;
    let hi = (lo + 1).min(4);
    let frac = shape - lo as f64;
    if frac < 0.001 {
        osc_shape_raw(lo, phase, dt, noise)
    } else if frac > 0.999 {
        osc_shape_raw(hi, phase, dt, noise)
    } else {
        let a = osc_shape_raw(lo, phase, dt, noise);
        let b = osc_shape_raw(hi, phase, dt, noise);
        a * (1.0 - frac) + b * frac
    }
}

#[inline(always)]
pub fn adsr_tick(
    stage: &mut EnvStage,
    level: &mut f64,
    time: &mut f64,
    attack: f64,
    decay: f64,
    sustain: f64,
    release: f64,
    dt: f64,
    released: bool,
) -> f64 {
    if released && *stage != EnvStage::Release && *stage != EnvStage::Off {
        *stage = EnvStage::Release;
        *time = 0.0;
    }
    match *stage {
        EnvStage::Attack => {
            let a = attack.max(0.001);
            *level += dt / a;
            *time += dt;
            if *level >= 1.0 {
                *level = 1.0;
                *stage = EnvStage::Decay;
                *time = 0.0;
            }
        }
        EnvStage::Decay => {
            let d = decay.max(0.001);
            *time += dt;
            // Exponential decay — sounds more natural
            let target = sustain;
            let coeff = fast_exp(-dt / (d * 0.3));
            *level = target + (*level - target) * coeff;
            if (*level - target).abs() < 0.001 {
                *level = target;
                *stage = EnvStage::Sustain;
                *time = 0.0;
            }
        }
        EnvStage::Sustain => {
            *level = sustain;
        }
        EnvStage::Release => {
            let r = release.max(0.001);
            *time += dt;
            // Exponential release
            let coeff = fast_exp(-dt / (r * 0.3));
            *level *= coeff;
            if *level <= 0.0005 {
                *level = 0.0;
                *stage = EnvStage::Off;
            }
        }
        EnvStage::Off => {
            *level = 0.0;
        }
    }
    *level
}

/// State Variable Filter tick — optimized with fast_tan and denormal protection.
#[inline(always)]
pub fn svf_tick(
    input: f64,
    cutoff_hz: f64,
    resonance: f64,
    sample_rate: f64,
    ic1eq: &mut f64,
    ic2eq: &mut f64,
) -> (f64, f64, f64) {
    let g = fast_tan(std::f64::consts::PI * cutoff_hz / sample_rate);
    let k = 2.0 - 2.0 * resonance.clamp(0.0, 0.99);
    let a1 = 1.0 / (1.0 + g * (g + k));
    let a2 = g * a1;
    let a3 = g * a2;
    let v3 = input - *ic2eq;
    let v1 = a1 * *ic1eq + a2 * v3;
    let v2 = *ic2eq + a2 * *ic1eq + a3 * v3;
    *ic1eq = 2.0 * v1 - *ic1eq + DENORMAL_FIX;
    *ic2eq = 2.0 * v2 - *ic2eq + DENORMAL_FIX;
    (v2, v1, input - k * v1 - v2) // (lp, bp, hp)
}

// ═══════════════════════════════════════════════════════════════════
// INSTRUMENT: SubtractiveSynth
// ═══════════════════════════════════════════════════════════════════

pub struct SubtractiveSynth;

static SUBTRACTIVE_PARAMS: &[ParamDesc] = &[
    // ── Oscillators ──
    ParamDesc {
        id: "osc1_wave",
        name: "Osc1 Shape",
        default: 1.0,
        min: 0.0,
        max: 4.0,
        options: Some(&["Sine", "Saw", "Square", "Triangle", "Noise"]),
    },
    ParamDesc {
        id: "osc2_wave",
        name: "Osc2 Shape",
        default: 1.0,
        min: 0.0,
        max: 4.0,
        options: Some(&["Sine", "Saw", "Square", "Triangle", "Noise"]),
    },
    ParamDesc {
        id: "osc_mix",
        name: "Osc Mix",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "gain",
        name: "Gain",
        default: 0.8,
        min: 0.0,
        max: 2.0,
        options: None,
    },
    // ── Oscillator tuning ──
    ParamDesc {
        id: "osc2_semi",
        name: "Semi",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_fine",
        name: "Fine",
        default: 0.0,
        min: -100.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "filter_type",
        name: "Filt Type",
        default: 0.0,
        min: 0.0,
        max: 2.0,
        options: Some(&["LowPass", "HighPass", "BandPass"]),
    },
    ParamDesc {
        id: "filter_cutoff",
        name: "Cutoff",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Filter ──
    ParamDesc {
        id: "filter_reso",
        name: "Reso",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_env",
        name: "Env Amt",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_a",
        name: "F.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "filter_d",
        name: "F.Dec",
        default: 0.2,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Filter env cont + Amp ADSR ──
    ParamDesc {
        id: "filter_s",
        name: "F.Sus",
        default: 0.4,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_r",
        name: "F.Rel",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_a",
        name: "A.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_d",
        name: "A.Dec",
        default: 0.1,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Amp ADSR cont ──
    ParamDesc {
        id: "amp_s",
        name: "A.Sus",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "amp_r",
        name: "A.Rel",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Phase ──
    ParamDesc {
        id: "phase_spread",
        name: "Phase",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl InstrumentModule for SubtractiveSynth {
    fn name(&self) -> &'static str {
        "Analog"
    }
    fn params(&self) -> &'static [ParamDesc] {
        SUBTRACTIVE_PARAMS
    }

    fn process_voice(
        &self,
        voice: &mut ModuleVoice,
        params: &[(String, f32)],
        sample_rate: f64,
        _extra: &ModuleExtra,
    ) -> (f64, f64) {
        let dt = 1.0 / sample_rate;
        let st = &mut voice.state;

        let osc1_shape = param_val(params, "osc1_wave", 1.0) as f64;
        let osc2_shape = param_val(params, "osc2_wave", 1.0) as f64;
        let osc2_semi = param_val(params, "osc2_semi", 0.0) as f64;
        let osc2_fine = param_val(params, "osc2_fine", 0.0) as f64;
        let osc_mix = param_val(params, "osc_mix", 0.0) as f64;
        let filter_cutoff_norm = param_val(params, "filter_cutoff", 0.8) as f64;
        let filter_reso = param_val(params, "filter_reso", 0.0) as f64;
        let filter_env_amt = param_val(params, "filter_env", 0.0) as f64;
        let filter_type = param_val(params, "filter_type", 0.0) as f64;
        let filter_a = param_val(params, "filter_a", 0.01) as f64;
        let filter_d = param_val(params, "filter_d", 0.2) as f64;
        let filter_s = param_val(params, "filter_s", 0.4) as f64;
        let filter_r = param_val(params, "filter_r", 0.3) as f64;
        let amp_a = param_val(params, "amp_a", 0.01) as f64;
        let amp_d = param_val(params, "amp_d", 0.1) as f64;
        let amp_s = param_val(params, "amp_s", 0.8) as f64;
        let amp_r = param_val(params, "amp_r", 0.3) as f64;
        let gain = param_val(params, "gain", 0.8) as f64;
        let phase_spread = param_val(params, "phase_spread", 1.0) as f64;

        // ── Phase spread: on first sample, lerp random phases toward 0.0 ──
        if st.amp_time == 0.0 && st.amp_stage == EnvStage::Attack && st.amp_level == 0.0 {
            st.phase0 *= phase_spread;
            st.phase1 *= phase_spread;
        }

        // ── Oscillators with morphing ──
        let osc1_inc = voice.freq / sample_rate;
        let osc1 = osc_morph(osc1_shape, st.phase0, osc1_inc, &mut st.noise_seed);

        let detune = fast_pow2((osc2_semi + osc2_fine / 100.0) / 12.0);
        let osc2_freq = voice.freq * detune;
        let osc2_inc = osc2_freq / sample_rate;
        let osc2 = osc_morph(osc2_shape, st.phase1, osc2_inc, &mut st.noise_seed);

        st.phase0 += osc1_inc;
        if st.phase0 >= 1.0 {
            st.phase0 -= 1.0;
        }
        st.phase1 += osc2_inc;
        if st.phase1 >= 1.0 {
            st.phase1 -= 1.0;
        }

        let osc_out = osc1 * (1.0 - osc_mix) + osc2 * osc_mix;

        // ── Filter envelope ──
        let filt_env = adsr_tick(
            &mut st.filt_stage,
            &mut st.filt_level,
            &mut st.filt_time,
            filter_a,
            filter_d,
            filter_s,
            filter_r,
            dt,
            voice.released,
        );
        // Base cutoff: exponential mapping 20Hz–20kHz using fast_pow2
        let base_hz = 20.0 * fast_pow2(filter_cutoff_norm * 9.965784284662087); // log2(20000/20)
                                                                                // Env amount: ±8 octaves
        let env_octaves = filter_env_amt * filt_env * 8.0;
        let cutoff_hz = (base_hz * fast_pow2(env_octaves)).clamp(20.0, sample_rate * 0.49);

        let (lp, bp, hp) = svf_tick(
            osc_out,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1,
            &mut st.filt_ic2,
        );
        // Filter type morphing: 0.0=LP, 1.0=HP, 2.0=BP (continuous crossfade)
        let filtered = if filter_type <= 1.0 {
            let t = filter_type;
            lp * (1.0 - t) + hp * t
        } else {
            let t = filter_type - 1.0;
            hp * (1.0 - t) + bp * t
        };

        // ── Amp envelope ──
        let amp_env = adsr_tick(
            &mut st.amp_stage,
            &mut st.amp_level,
            &mut st.amp_time,
            amp_a,
            amp_d,
            amp_s,
            amp_r,
            dt,
            voice.released,
        );

        let mono = filtered * amp_env * gain * (voice.velocity as f64);
        (mono, mono)
    }
}

// ═══════════════════════════════════════════════════════════════════
// INSTRUMENT: SuperSaw  (JP-8000-style dual 7-oscillator detuned saw)
// ═══════════════════════════════════════════════════════════════════

pub struct SuperSawSynth;

/// JP-8000 detune coefficients (normalized so >> 7 = / 128).
/// Center osc has coefficient 0 (no detuning).
const JP8000_DETUNE_COEFS: [f64; 7] = [0.0, 128.0, -128.0, 408.0, -412.0, 704.0, -720.0];

/// Precomputed stereo pan gains for the 7 SuperSaw voices.
/// Avoids cos()/sin() on every sample — these are constant.
/// Pan positions: center, -1.0, +1.0, -0.6, +0.6, -0.3, +0.3
/// Converted to (L_gain, R_gain) via equal-power: L=cos(θ), R=sin(θ), θ=(pan+1)/2 * π/2
/// With width=1.0 these are the full-spread values; we lerp toward (0.707, 0.707) for width<1.
const SUPERSAW_PAN_L: [f64; 7] = [
    0.7071067811865476,  // center: cos(π/4)
    1.0,                 // voice 1 (pan -1.0): cos(0)
    0.0,                 // voice 2 (pan +1.0): cos(π/2)
    0.891006524188368,   // voice 3 (pan -0.6): cos(0.2*π/2)
    0.45399049973954675, // voice 4 (pan +0.6): cos(0.8*π/2)
    0.7933533402912352,  // voice 5 (pan -0.3): cos(0.35*π/2)
    0.6087614290087207,  // voice 6 (pan +0.3): cos(0.65*π/2)
];
const SUPERSAW_PAN_R: [f64; 7] = [
    0.7071067811865476,  // center: sin(π/4)
    0.0,                 // voice 1 (pan -1.0): sin(0)
    1.0,                 // voice 2 (pan +1.0): sin(π/2)
    0.45399049973954675, // voice 3 (pan -0.6): sin(0.2*π/2)
    0.891006524188368,   // voice 4 (pan +0.6): sin(0.8*π/2)
    0.6087614290087207,  // voice 5 (pan -0.3): sin(0.35*π/2)
    0.7933533402912352,  // voice 6 (pan +0.3): sin(0.65*π/2)
];
/// Center gain for width=0 (mono): both L and R are 1/√2
const PAN_CENTER: f64 = 0.7071067811865476;

/// Process one JP-8000-style supersaw bank (7 oscillators) with stereo width.
/// `phases` must be a mutable slice of 7 f64 phases.
/// `detune_amt` is 0.0–0.3, `mix` is 0.0–1.0, `width` is 0.0–1.0.
/// Returns `(left, right)` with the 6 detuned voices spread across the stereo field.
/// The center voice stays in the middle.
///
/// OPTIMIZED: Uses precomputed pan tables instead of per-sample trig.
#[inline(always)]
pub fn supersaw_bank(
    phases: &mut [f64],
    base_freq: f64,
    detune_amt: f64,
    mix: f64,
    width: f64,
    sample_rate: f64,
) -> (f64, f64) {
    let base_inc = base_freq / sample_rate;
    let detune_base = base_inc * detune_amt;

    let center_atten = 25.0 / 128.0;
    let side_atten = mix;

    let mut sum_l = 0.0_f64;
    let mut sum_r = 0.0_f64;

    for i in 0..7 {
        let voice_detune = JP8000_DETUNE_COEFS[i] * detune_base * (1.0 / 128.0);
        let inc = base_inc + voice_detune;
        let phase = phases[i];

        let naive = 2.0 * phase - 1.0;
        let saw = naive - polyblep(phase, inc.abs().max(1e-12));

        let weight = if i == 0 { center_atten } else { side_atten };
        let s = saw * weight;

        // Stereo placement: use precomputed pan table, lerp with width
        let pan_l = PAN_CENTER + (SUPERSAW_PAN_L[i] - PAN_CENTER) * width;
        let pan_r = PAN_CENTER + (SUPERSAW_PAN_R[i] - PAN_CENTER) * width;
        sum_l += s * pan_l;
        sum_r += s * pan_r;

        let next = phase + inc;
        phases[i] = next - next.floor();
    }
    // Normalize by total weight so output stays roughly in [-1, 1]
    let total_weight = center_atten + 6.0 * side_atten;
    let norm = if total_weight > 1e-9 {
        1.0 / total_weight
    } else {
        1.0
    };
    (sum_l * norm, sum_r * norm)
}

static SUPERSAW_PARAMS: &[ParamDesc] = &[
    // ── Oscillator 1 ──
    ParamDesc {
        id: "osc1_detune",
        name: "O1 Detune",
        default: 0.01,
        min: 0.0,
        max: 0.04,
        options: None,
    },
    ParamDesc {
        id: "osc1_mix",
        name: "O1 Mix",
        default: 0.75,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "osc1_width",
        name: "O1 Width",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Oscillator 2 ──
    ParamDesc {
        id: "osc2_detune",
        name: "O2 Detune",
        default: 0.01,
        min: 0.0,
        max: 0.04,
        options: None,
    },
    ParamDesc {
        id: "osc2_mix",
        name: "O2 Mix",
        default: 0.75,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_width",
        name: "O2 Width",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Oscillator blend + tuning ──
    ParamDesc {
        id: "osc_blend",
        name: "Osc Blend",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_semi",
        name: "O2 Semi",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_fine",
        name: "O2 Fine",
        default: 0.0,
        min: -100.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "gain",
        name: "Gain",
        default: 0.7,
        min: 0.0,
        max: 2.0,
        options: None,
    },
    ParamDesc {
        id: "noise_gain",
        name: "Noise",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Filter ──
    ParamDesc {
        id: "filter_cutoff",
        name: "Cutoff",
        default: 0.9,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_reso",
        name: "Reso",
        default: 0.1,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_env",
        name: "Env Amt",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_a",
        name: "F.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "filter_d",
        name: "F.Dec",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Filter env cont + Amp ADSR ──
    ParamDesc {
        id: "filter_s",
        name: "F.Sus",
        default: 0.3,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_r",
        name: "F.Rel",
        default: 0.4,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_a",
        name: "A.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_d",
        name: "A.Dec",
        default: 0.1,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Amp ADSR cont ──
    ParamDesc {
        id: "amp_s",
        name: "A.Sus",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "amp_r",
        name: "A.Rel",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
];

impl InstrumentModule for SuperSawSynth {
    fn name(&self) -> &'static str {
        "HyperSaw"
    }
    fn params(&self) -> &'static [ParamDesc] {
        SUPERSAW_PARAMS
    }

    fn process_voice(
        &self,
        voice: &mut ModuleVoice,
        params: &[(String, f32)],
        sample_rate: f64,
        _extra: &ModuleExtra,
    ) -> (f64, f64) {
        let dt = 1.0 / sample_rate;
        let st = &mut voice.state;

        let osc1_detune = param_val(params, "osc1_detune", 0.1) as f64;
        let osc1_mix = param_val(params, "osc1_mix", 0.75) as f64;
        let osc1_width = param_val(params, "osc1_width", 0.5) as f64;
        let osc2_detune = param_val(params, "osc2_detune", 0.1) as f64;
        let osc2_mix = param_val(params, "osc2_mix", 0.75) as f64;
        let osc2_width = param_val(params, "osc2_width", 0.5) as f64;
        let osc_blend = param_val(params, "osc_blend", 0.0) as f64;
        let osc2_semi = param_val(params, "osc2_semi", 0.0) as f64;
        let osc2_fine = param_val(params, "osc2_fine", 0.0) as f64;
        let gain = param_val(params, "gain", 0.7) as f64;
        let noise_gain = param_val(params, "noise_gain", 0.0) as f64;
        let filter_cutoff_norm = param_val(params, "filter_cutoff", 0.9) as f64;
        let filter_reso = param_val(params, "filter_reso", 0.1) as f64;
        let filter_env_amt = param_val(params, "filter_env", 0.0) as f64;
        let filter_a = param_val(params, "filter_a", 0.01) as f64;
        let filter_d = param_val(params, "filter_d", 0.3) as f64;
        let filter_s = param_val(params, "filter_s", 0.3) as f64;
        let filter_r = param_val(params, "filter_r", 0.4) as f64;
        let amp_a = param_val(params, "amp_a", 0.01) as f64;
        let amp_d = param_val(params, "amp_d", 0.1) as f64;
        let amp_s = param_val(params, "amp_s", 0.8) as f64;
        let amp_r = param_val(params, "amp_r", 0.3) as f64;

        // ── Dual SuperSaw oscillators (stereo) ──
        let freq1 = voice.freq;
        let (osc1_l, osc1_r) = supersaw_bank(
            &mut st.extra_phases[0..7],
            freq1,
            osc1_detune,
            osc1_mix,
            osc1_width,
            sample_rate,
        );

        // Precompute detune ratio with fast_pow2 instead of per-sample powf
        let detune_ratio = fast_pow2((osc2_semi + osc2_fine / 100.0) / 12.0);
        let freq2 = voice.freq * detune_ratio;
        let (osc2_l, osc2_r) = supersaw_bank(
            &mut st.extra_phases[7..14],
            freq2,
            osc2_detune,
            osc2_mix,
            osc2_width,
            sample_rate,
        );

        // Blend: 0 = osc1 only, 1 = osc2 only
        let osc_l = osc1_l * (1.0 - osc_blend) + osc2_l * osc_blend;
        let osc_r = osc1_r * (1.0 - osc_blend) + osc2_r * osc_blend;

        // ── White noise (mono, added equally to both channels) ──
        let noise = if noise_gain > 0.001 {
            let mut s = st.noise_seed;
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            st.noise_seed = s;
            (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        } else {
            0.0
        };
        let osc_l = osc_l + noise * noise_gain;
        let osc_r = osc_r + noise * noise_gain;

        // ── Highpass on detuned saws (JP-8000 characteristic) ──
        // Process L and R channels independently through their own HP filter state
        let (_, _, hp_l) = svf_tick(
            osc_l,
            20.0,
            0.0,
            sample_rate,
            &mut st.hp_ic1,
            &mut st.hp_ic2,
        );
        let (_, _, hp_r) = svf_tick(
            osc_r,
            20.0,
            0.0,
            sample_rate,
            &mut st.hp_ic1_r,
            &mut st.hp_ic2_r,
        );

        // ── Filter envelope ──
        let filt_env = adsr_tick(
            &mut st.filt_stage,
            &mut st.filt_level,
            &mut st.filt_time,
            filter_a,
            filter_d,
            filter_s,
            filter_r,
            dt,
            voice.released,
        );
        // Precompute base cutoff with fast_pow2 instead of powf
        let base_hz = 20.0 * fast_pow2(filter_cutoff_norm * 9.965784284662087); // log2(20000/20) ≈ 9.9658
        let env_octaves = filter_env_amt * filt_env * 8.0;
        let cutoff_hz = (base_hz * fast_pow2(env_octaves)).clamp(20.0, sample_rate * 0.49);

        // Process L and R channels independently through their own LP filter state
        let (filt_l, _, _) = svf_tick(
            hp_l,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1,
            &mut st.filt_ic2,
        );
        let (filt_r, _, _) = svf_tick(
            hp_r,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1_r,
            &mut st.filt_ic2_r,
        );

        // ── Amp envelope ──
        let amp_env = adsr_tick(
            &mut st.amp_stage,
            &mut st.amp_level,
            &mut st.amp_time,
            amp_a,
            amp_d,
            amp_s,
            amp_r,
            dt,
            voice.released,
        );

        let g = amp_env * gain * (voice.velocity as f64);
        (filt_l * g, filt_r * g)
    }
}

// ═══════════════════════════════════════════════════════════════════
// INSTRUMENT: Sampler
// ═══════════════════════════════════════════════════════════════════

pub struct Sampler;

static SAMPLER_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "gate",
        name: "Gate Mode",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        options: Some(TOGGLE_OFF_ON),
    },
    ParamDesc {
        id: "root_note",
        name: "Root Note",
        default: 60.0,
        min: 0.0,
        max: 127.0,
        options: Some(MIDI_NOTE_NAMES),
    },
    ParamDesc {
        id: "pitch_track",
        name: "Pitch Track",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        options: Some(TOGGLE_OFF_ON),
    },
    ParamDesc {
        id: "start",
        name: "Start",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "end",
        name: "End",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "amp_a",
        name: "Amp A",
        default: 0.005,
        min: 0.001,
        max: 5.0,
        options: None,
    },
    ParamDesc {
        id: "amp_d",
        name: "Amp D",
        default: 0.05,
        min: 0.001,
        max: 5.0,
        options: None,
    },
    ParamDesc {
        id: "amp_s",
        name: "Amp S",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "amp_r",
        name: "Amp R",
        default: 0.1,
        min: 0.001,
        max: 5.0,
        options: None,
    },
    ParamDesc {
        id: "gain",
        name: "Gain",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl InstrumentModule for Sampler {
    fn name(&self) -> &'static str {
        "Sampler"
    }
    fn params(&self) -> &'static [ParamDesc] {
        SAMPLER_PARAMS
    }

    fn process_voice(
        &self,
        voice: &mut ModuleVoice,
        params: &[(String, f32)],
        sample_rate: f64,
        extra: &ModuleExtra,
    ) -> (f64, f64) {
        let sample_data = match &extra.sample_data {
            Some(d) if !d.is_empty() => d,
            _ => return (0.0, 0.0),
        };
        let file_sr = extra.sample_sr;
        let dt = 1.0 / sample_rate;
        let st = &mut voice.state;

        let gate = param_val(params, "gate", 1.0) > 0.5;
        let root_note = param_val(params, "root_note", 60.0) as u8;
        let pitch_track = param_val(params, "pitch_track", 1.0) > 0.5;
        let start_frac = param_val(params, "start", 0.0).clamp(0.0, 1.0) as f64;
        let end_frac = param_val(params, "end", 1.0).clamp(0.0, 1.0) as f64;
        let amp_a = param_val(params, "amp_a", 0.005) as f64;
        let amp_d = param_val(params, "amp_d", 0.05) as f64;
        let amp_s = param_val(params, "amp_s", 1.0) as f64;
        let amp_r = param_val(params, "amp_r", 0.1) as f64;
        let gain = param_val(params, "gain", 0.8) as f64;

        let total = sample_data.len();
        let _start_frame = (start_frac * (total - 1) as f64) as usize;
        let end_frame = (end_frac * (total - 1) as f64).max(_start_frame as f64 + 1.0) as usize;

        let sr_ratio = file_sr as f64 / sample_rate;
        let step = if pitch_track {
            sr_ratio * fast_pow2((voice.pitch as f64 - root_note as f64) / 12.0)
        } else {
            sr_ratio
        };

        let released = if gate { voice.released } else { false };
        let amp_env = adsr_tick(
            &mut st.amp_stage,
            &mut st.amp_level,
            &mut st.amp_time,
            amp_a,
            amp_d,
            amp_s,
            amp_r,
            dt,
            released,
        );

        let pos = st.sampler_pos;
        let idx0 = pos as usize;
        if idx0 >= end_frame || idx0 >= total {
            if st.amp_stage != EnvStage::Off && st.amp_stage != EnvStage::Release {
                voice.released = true;
            }
            if !gate {
                st.amp_stage = EnvStage::Off;
                st.amp_level = 0.0;
            }
            return (0.0, 0.0);
        }

        let idx1 = (idx0 + 1).min(total - 1);
        let frac = pos - idx0 as f64;
        let s = sample_data[idx0] as f64 * (1.0 - frac) + sample_data[idx1] as f64 * frac;
        st.sampler_pos += step;
        let mono = s * amp_env * gain * (voice.velocity as f64);
        (mono, mono)
    }
}

// ═══════════════════════════════════════════════════════════════════
// INSTRUMENT: HeavySynth  (1-osc advanced shapes + sub + noise + filter + distortion)
// ═══════════════════════════════════════════════════════════════════

pub struct HeavySynth;

/// Oscillator shapes for HeavySynth:
///   0 = Impulse Train, 1 = Saw, 2 = Triangle, 3 = Slope,
///   4 = Square, 5 = Square Bright, 6 = Square Dark, 7 = Square-Triangle
#[inline]
fn heavy_osc_shape(shape: usize, phase: f64, dt: f64) -> f64 {
    match shape {
        // 0 — Impulse Train: narrow pulse (pulse-width ~5%)
        0 => {
            let pw = 0.05;
            let mut s = if phase < pw { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 1.0 - pw) % 1.0, dt);
            s
        }
        // 1 — Saw (polyblep)
        1 => {
            let naive = 2.0 * phase - 1.0;
            naive - polyblep(phase, dt)
        }
        // 2 — Triangle (piecewise linear)
        2 => {
            if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            }
        }
        // 3 — Slope (asymmetric triangle: fast rise, slow fall)
        3 => {
            let rise = 0.15;
            if phase < rise {
                phase / rise * 2.0 - 1.0
            } else {
                1.0 - 2.0 * (phase - rise) / (1.0 - rise)
            }
        }
        // 4 — Square (standard polyblep, 50% duty)
        4 => {
            let mut s = if phase < 0.5 { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 0.5) % 1.0, dt);
            s
        }
        // 5 — Square Bright (narrower pulse ≈ 30% for more harmonics)
        5 => {
            let pw = 0.30;
            let mut s = if phase < pw { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 1.0 - pw) % 1.0, dt);
            s
        }
        // 6 — Square Dark (wider pulse ≈ 45%, gentler timbre, slight LP character)
        6 => {
            let pw = 0.45;
            let mut s = if phase < pw { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 1.0 - pw) % 1.0, dt);
            // Slight rounding to darken
            s * 0.85
        }
        // 7 — Square-Triangle morph (50/50 crossfade)
        7 => {
            let mut sq = if phase < 0.5 { 1.0 } else { -1.0 };
            sq += polyblep(phase, dt);
            sq -= polyblep((phase + 0.5) % 1.0, dt);
            let tri = if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            };
            (sq + tri) * 0.5
        }
        _ => 0.0,
    }
}

static HEAVY_PARAMS: &[ParamDesc] = &[
    // ── Oscillator ──
    ParamDesc {
        id: "osc_shape",
        name: "Shape",
        default: 1.0,
        min: 0.0,
        max: 7.0,
        options: Some(&[
            "Impulse",
            "Saw",
            "Triangle",
            "Slope",
            "Square",
            "Sq Bright",
            "Sq Dark",
            "Sq-Tri",
        ]),
    },
    ParamDesc {
        id: "sub_level",
        name: "Sub",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "noise_mix",
        name: "Noise",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "gain",
        name: "Gain",
        default: 0.8,
        min: 0.0,
        max: 2.0,
        options: None,
    },
    // ── Filter ──
    ParamDesc {
        id: "filter_cutoff",
        name: "Cutoff",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_reso",
        name: "Reso",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_env",
        name: "Env Amt",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_a",
        name: "F.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "filter_d",
        name: "F.Dec",
        default: 0.2,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "filter_s",
        name: "F.Sus",
        default: 0.4,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_r",
        name: "F.Rel",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Amp ADSR ──
    ParamDesc {
        id: "amp_a",
        name: "A.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_d",
        name: "A.Dec",
        default: 0.1,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_s",
        name: "A.Sus",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "amp_r",
        name: "A.Rel",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Distortion ──
    ParamDesc {
        id: "dist_drive",
        name: "Drive",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "dist_type",
        name: "Dist Type",
        default: 0.0,
        min: 0.0,
        max: 3.0,
        options: Some(&["Tanh", "Clip", "Sine", "Bit"]),
    },
];

impl InstrumentModule for HeavySynth {
    fn name(&self) -> &'static str {
        "Monolith"
    }
    fn params(&self) -> &'static [ParamDesc] {
        HEAVY_PARAMS
    }

    fn process_voice(
        &self,
        voice: &mut ModuleVoice,
        params: &[(String, f32)],
        sample_rate: f64,
        _extra: &ModuleExtra,
    ) -> (f64, f64) {
        let dt = 1.0 / sample_rate;
        let st = &mut voice.state;

        let osc_shape = param_val(params, "osc_shape", 1.0) as usize;
        let sub_level = param_val(params, "sub_level", 0.0) as f64;
        let noise_mix = param_val(params, "noise_mix", 0.0) as f64;
        let gain = param_val(params, "gain", 0.8) as f64;
        let filter_cutoff_norm = param_val(params, "filter_cutoff", 0.8) as f64;
        let filter_reso = param_val(params, "filter_reso", 0.0) as f64;
        let filter_env_amt = param_val(params, "filter_env", 0.0) as f64;
        let filter_a = param_val(params, "filter_a", 0.01) as f64;
        let filter_d = param_val(params, "filter_d", 0.2) as f64;
        let filter_s = param_val(params, "filter_s", 0.4) as f64;
        let filter_r = param_val(params, "filter_r", 0.3) as f64;
        let amp_a = param_val(params, "amp_a", 0.01) as f64;
        let amp_d = param_val(params, "amp_d", 0.1) as f64;
        let amp_s = param_val(params, "amp_s", 0.8) as f64;
        let amp_r = param_val(params, "amp_r", 0.3) as f64;
        let dist_drive = param_val(params, "dist_drive", 0.0) as f64;
        let dist_type = param_val(params, "dist_type", 0.0) as usize;

        // ── Main oscillator ──
        let osc_inc = voice.freq / sample_rate;
        let main_osc = heavy_osc_shape(osc_shape.min(7), st.phase0, osc_inc);
        st.phase0 += osc_inc;
        if st.phase0 >= 1.0 {
            st.phase0 -= 1.0;
        }

        // ── Sub oscillator (square, one octave down) ──
        let sub_inc = osc_inc * 0.5;
        let mut sub_osc = if st.phase1 < 0.5 { 1.0 } else { -1.0 };
        sub_osc += polyblep(st.phase1, sub_inc);
        sub_osc -= polyblep((st.phase1 + 0.5) % 1.0, sub_inc);
        st.phase1 += sub_inc;
        if st.phase1 >= 1.0 {
            st.phase1 -= 1.0;
        }

        // ── Noise ──
        let noise = {
            let mut s = st.noise_seed;
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            st.noise_seed = s;
            (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        };

        // ── Mix ──
        let osc_out = main_osc * (1.0 - noise_mix) + noise * noise_mix + sub_osc * sub_level;

        // ── Filter envelope ──
        let filt_env = adsr_tick(
            &mut st.filt_stage,
            &mut st.filt_level,
            &mut st.filt_time,
            filter_a,
            filter_d,
            filter_s,
            filter_r,
            dt,
            voice.released,
        );
        let base_hz = 20.0 * fast_pow2(filter_cutoff_norm * 9.965784284662087);
        let env_octaves = filter_env_amt * filt_env * 8.0;
        let cutoff_hz = (base_hz * fast_pow2(env_octaves)).clamp(20.0, sample_rate * 0.49);

        let (lp, _bp, _hp) = svf_tick(
            osc_out,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1,
            &mut st.filt_ic2,
        );

        // ── Distortion stage (optimized: fast_tanh, fast_sin, fast_pow2) ──
        let filtered = if dist_drive > 0.001 {
            match dist_type {
                0 => {
                    // Tanh approximation: x / (1 + |x|) scaled
                    let d = 1.0 + dist_drive * 15.0;
                    let x = lp * d;
                    // Fast tanh: x*(27+x²)/(27+9x²) — Padé approximant
                    let x2 = x * x;
                    let num = x * (27.0 + x2);
                    let den_val = 27.0 + 9.0 * x2;
                    let tanh_x = num / den_val;
                    // Normalize by tanh(d)
                    let d2 = d * d;
                    let tanh_d = d * (27.0 + d2) / (27.0 + 9.0 * d2);
                    if tanh_d.abs() < 1e-9 {
                        lp
                    } else {
                        tanh_x / tanh_d
                    }
                }
                1 => {
                    // Hard clip
                    let th = (1.0 - dist_drive * 0.85).max(0.01);
                    lp.clamp(-th, th) / th
                }
                2 => {
                    // Sine fold — use fast_sin
                    fast_sin(lp * (1.0 + dist_drive * 5.0) * std::f64::consts::PI)
                }
                3 => {
                    // Bit crush
                    let steps = fast_pow2(14.0 - dist_drive * 12.0).max(1.0);
                    (lp * steps + 0.5).floor() / steps
                }
                _ => lp,
            }
        } else {
            lp
        };

        // ── Amp envelope ──
        let amp_env = adsr_tick(
            &mut st.amp_stage,
            &mut st.amp_level,
            &mut st.amp_time,
            amp_a,
            amp_d,
            amp_s,
            amp_r,
            dt,
            voice.released,
        );

        let mono = filtered * amp_env * gain * (voice.velocity as f64);
        (mono, mono)
    }
}

// ═══════════════════════════════════════════════════════════════════
// EFFECTS — each owns its own state
// ═══════════════════════════════════════════════════════════════════

// ── LP Filter ────────────────────────────────────────────────────────

pub struct FxLpFilter {
    ic1_l: f64,
    ic2_l: f64,
    ic1_r: f64,
    ic2_r: f64,
}
impl FxLpFilter {
    pub fn new() -> Self {
        Self {
            ic1_l: 0.0,
            ic2_l: 0.0,
            ic1_r: 0.0,
            ic2_r: 0.0,
        }
    }
}

static LP_FILTER_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "cutoff",
        name: "Cutoff",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "resonance",
        name: "Resonance",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxLpFilter {
    fn name(&self) -> &'static str {
        "LP Filter"
    }
    fn params(&self) -> &'static [ParamDesc] {
        LP_FILTER_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let c = param_val(params, "cutoff", 1.0);
        let r = param_val(params, "resonance", 0.0) as f64;
        let hz = (20.0 * fast_pow2(c as f64 * 9.965784284662087)).min(sr * 0.49);
        let (lp_l, _, _) = svf_tick(left, hz, r, sr, &mut self.ic1_l, &mut self.ic2_l);
        let (lp_r, _, _) = svf_tick(right, hz, r, sr, &mut self.ic1_r, &mut self.ic2_r);
        (lp_l, lp_r)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxLpFilter::new())
    }
}

// ── HP Filter ────────────────────────────────────────────────────────

pub struct FxHpFilter {
    ic1_l: f64,
    ic2_l: f64,
    ic1_r: f64,
    ic2_r: f64,
}
impl FxHpFilter {
    pub fn new() -> Self {
        Self {
            ic1_l: 0.0,
            ic2_l: 0.0,
            ic1_r: 0.0,
            ic2_r: 0.0,
        }
    }
}

static HP_FILTER_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "cutoff",
        name: "Cutoff",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "resonance",
        name: "Resonance",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxHpFilter {
    fn name(&self) -> &'static str {
        "HP Filter"
    }
    fn params(&self) -> &'static [ParamDesc] {
        HP_FILTER_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let c = param_val(params, "cutoff", 0.0);
        let r = param_val(params, "resonance", 0.0) as f64;
        let hz = (20.0 * fast_pow2(c as f64 * 9.965784284662087)).min(sr * 0.49);
        let (_, _, hp_l) = svf_tick(left, hz, r, sr, &mut self.ic1_l, &mut self.ic2_l);
        let (_, _, hp_r) = svf_tick(right, hz, r, sr, &mut self.ic1_r, &mut self.ic2_r);
        (hp_l, hp_r)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxHpFilter::new())
    }
}

// ── Delay ────────────────────────────────────────────────────────────

pub struct FxDelay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
}
impl FxDelay {
    pub fn new(sr: u32) -> Self {
        let len = (sr as usize) * 2;
        Self {
            buf_l: vec![0.0; len],
            buf_r: vec![0.0; len],
            write_pos: 0,
        }
    }
}

static DELAY_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "time",
        name: "Time",
        default: 0.25,
        min: 0.01,
        max: 2.0,
        options: None,
    },
    ParamDesc {
        id: "feedback",
        name: "Feedback",
        default: 0.3,
        min: 0.0,
        max: 0.99,
        options: None,
    },
    ParamDesc {
        id: "mix",
        name: "Mix",
        default: 0.3,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxDelay {
    fn name(&self) -> &'static str {
        "Delay"
    }
    fn params(&self) -> &'static [ParamDesc] {
        DELAY_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let time = param_val(params, "time", 0.25) as f64;
        let feedback = param_val(params, "feedback", 0.3) as f64;
        let mix = param_val(params, "mix", 0.3) as f64;
        let len = self.buf_l.len();
        if len == 0 {
            return (left, right);
        }
        let ds = (time * sr) as usize;
        let ds = ds.min(len - 1).max(1);
        let rp = (self.write_pos + len - ds) % len;
        let del_l = self.buf_l[rp] as f64;
        let del_r = self.buf_r[rp] as f64;
        self.buf_l[self.write_pos] = (left + del_l * feedback) as f32;
        self.buf_r[self.write_pos] = (right + del_r * feedback) as f32;
        self.write_pos = (self.write_pos + 1) % len;
        (
            left * (1.0 - mix) + del_l * mix,
            right * (1.0 - mix) + del_r * mix,
        )
    }
    fn reset(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos = 0;
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxDelay::new((self.buf_l.len() / 2) as u32))
    }
}

// ── Reverb (Dragonfly Hall-style reverb) ────────────────────────────
//
// Parameters match the Dragonfly Hall Reverb plugin:
//   size       — room size in metres (8–32 m)
//   width      — stereo spread (0–100 %)
//   predelay   — predelay in ms (0–100 ms)
//   decay      — RT60 tail length in seconds (0.1–10 s)
//   diffuse    — allpass diffusion density (0–100 %)
//   spin       — LFO rate in Hz (0–10 Hz)
//   wander     — LFO depth in ms (0–40 ms)
//   modulation — overall modulation amount (0–100 %)
//   low_cut    — high-pass on input, Hz (200–1200 Hz)
//   low_xover  — crossover below which decay is shortened (200–1200 Hz)
//   low_mult   — decay multiplier for bass (0.1–2.5)
//   high_cut   — low-pass on input, Hz (1000–20000 Hz)
//   high_xover — crossover above which decay is shortened (1000–20000 Hz)
//   high_mult  — decay multiplier for treble (0.1–2.5)
//   early      — early reflections output level (0–100 %)
//   early_send — how much early feeds into the late network (0–100 %)
//   late       — late reverb tail output level (0–100 %)
//   mix        — dry/wet blend (0–100 %)

pub struct FxReverb {
    sr: f64,

    // ── Predelay line (mono) ─────────────────────────────────────────
    pre_buf: Vec<f32>,
    pre_head: usize,

    // ── Input tone shaping: 1-pole HP (low_cut) and LP (high_cut) ───
    hp_state_l: f64,
    hp_state_r: f64,
    lp_state_l: f64,
    lp_state_r: f64,

    // ── Early reflections (8-tap multi-delay, stereo) ────────────────
    early_buf_l: Vec<f32>,
    early_buf_r: Vec<f32>,
    early_head: usize,

    // ── Late tail: 8 comb filters (4L + 4R) ─────────────────────────
    // Buffers are sized for max size (32 m) + max wander
    comb_buf_l: [Vec<f32>; 4],
    comb_buf_r: [Vec<f32>; 4],
    comb_head_l: [usize; 4],
    comb_head_r: [usize; 4],
    // Per-band one-pole damping filters inside comb loops
    comb_lp_l: [f64; 4], // high-frequency damping (high_mult path)
    comb_lp_r: [f64; 4],
    comb_hp_l: [f64; 4], // low-frequency damping (low_mult path)
    comb_hp_r: [f64; 4],

    // ── Late tail: 4 allpass sections (separate L/R) ─────────────────
    ap_buf_l: [Vec<f32>; 4],
    ap_buf_r: [Vec<f32>; 4],
    ap_head_l: [usize; 4],
    ap_head_r: [usize; 4],

    // ── Spin LFO (modulates comb delay times) ────────────────────────
    lfo_phase: f64,
}

impl FxReverb {
    // Early reflection tap primes (relative to size-scaled unit delay)
    const EARLY_PRIMES: [f64; 8] = [2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 17.0, 19.0];

    // Comb lengths in ms — two slightly offset sets for L/R decorrelation.
    // These match Freeverb3 Hibiki scale points used in Dragonfly.
    const COMB_MS_L: [f64; 4] = [29.13, 34.07, 38.93, 43.11];
    const COMB_MS_R: [f64; 4] = [30.61, 35.29, 40.37, 44.71];

    // Allpass delay times in ms
    const AP_MS: [f64; 4] = [12.61, 10.0, 7.73, 5.0];

    pub fn new(sr: u32) -> Self {
        let sr_f = sr as f64;
        let ms_to_samp = |ms: f64| -> usize { ((sr_f * ms / 1000.0) as usize + 4).max(8) };

        // Predelay: 0..100 ms
        let pre_len = ms_to_samp(100.0);

        // Early reflections: max tap = prime[7] * unit_ms_max
        // unit_ms at max size (32 m): let's say unit_ms_max = 32.0 ms
        // max tap = 19 * 32 = 608 ms — allocate a bit more
        let early_len = ms_to_samp(700.0);

        // Comb buffers: base_ms * size_factor_max + wander_max + headroom
        // size_factor_max = 32/8 = 4.0 (size 8..32 m, normalised to base at 8 m)
        // wander_max = 40 ms; headroom = 4 ms
        let comb_l: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; ms_to_samp(Self::COMB_MS_L[i] * 4.5 + 44.0)]);
        let comb_r: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; ms_to_samp(Self::COMB_MS_R[i] * 4.5 + 44.0)]);

        // Allpass buffers: ap_ms * size_factor_max + wander headroom
        let ap_l: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; ms_to_samp(Self::AP_MS[i] * 4.5 + 44.0)]);
        let ap_r: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; ms_to_samp(Self::AP_MS[i] * 4.5 + 44.0)]);

        Self {
            sr: sr_f,
            pre_buf: vec![0.0; pre_len],
            pre_head: 0,
            hp_state_l: 0.0,
            hp_state_r: 0.0,
            lp_state_l: 0.0,
            lp_state_r: 0.0,
            early_buf_l: vec![0.0; early_len],
            early_buf_r: vec![0.0; early_len],
            early_head: 0,
            comb_buf_l: comb_l,
            comb_buf_r: comb_r,
            comb_head_l: [0; 4],
            comb_head_r: [0; 4],
            comb_lp_l: [0.0; 4],
            comb_lp_r: [0.0; 4],
            comb_hp_l: [0.0; 4],
            comb_hp_r: [0.0; 4],
            ap_buf_l: ap_l,
            ap_buf_r: ap_r,
            ap_head_l: [0; 4],
            ap_head_r: [0; 4],
            lfo_phase: 0.0,
        }
    }

    /// Linearly interpolated read from a circular buffer.
    #[inline]
    fn read_interp(buf: &[f32], head: usize, offset_samples: f64) -> f64 {
        let len = buf.len();
        let rp = (head as f64 + len as f64 - offset_samples).rem_euclid(len as f64);
        let i0 = rp as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = rp - rp.floor();
        buf[i0] as f64 * (1.0 - frac) + buf[i1] as f64 * frac
    }

    /// 1-pole high-pass: y[n] = x[n] - x[n-1] + c * y[n-1]   (c ≈ e^{-2π fc/sr})
    #[inline]
    fn hp_tick(state: &mut f64, input: f64, coeff: f64) -> f64 {
        let prev = *state;
        *state = input;
        input - prev + coeff * (*state - input + prev * 0.0) // simplified: use biquad-free 1-pole
    }

    /// 1-pole low-pass: y[n] = (1-c)*x[n] + c*y[n-1]
    #[inline]
    fn lp_tick(state: &mut f64, input: f64, coeff: f64) -> f64 {
        *state = (1.0 - coeff) * input + coeff * *state;
        *state
    }

    /// One-pole HP coefficient from Hz.
    #[inline]
    fn hp_coeff(hz: f64, sr: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * hz / sr;
        // bilinear approx: c = (1 - tan(w/2)) / (1 + tan(w/2))
        let t = (w * 0.5).tan();
        ((1.0 - t) / (1.0 + t)).clamp(0.0, 0.9999)
    }

    /// One-pole LP coefficient from Hz.
    #[inline]
    fn lp_coeff(hz: f64, sr: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * hz / sr;
        (-w).exp().clamp(0.0, 0.9999)
    }

    /// Early reflections: 8-tap multi-delay with stereo decorrelation.
    fn process_early(&mut self, input: f64, size_m: f64) -> (f64, f64) {
        let len = self.early_buf_l.len();
        if len == 0 {
            return (input, input);
        }
        self.early_buf_l[self.early_head] = input as f32;
        // R write offset by 1 sample for decorrelation
        self.early_buf_r[(self.early_head + 1) % len] = input as f32;
        self.early_head = (self.early_head + 1) % len;

        // Unit delay: proportional to room size (4..32 ms range)
        let unit_ms = size_m * 1.0; // 1 ms per metre is a reasonable ER spacing
        let unit_samples = unit_ms * self.sr / 1000.0;

        let mut out_l = 0.0_f64;
        let mut out_r = 0.0_f64;
        for (i, &prime) in Self::EARLY_PRIMES.iter().enumerate() {
            let tap = prime * unit_samples;
            if tap < 1.0 || tap >= len as f64 {
                continue;
            }
            let gain = 1.0 / (i as f64 + 2.0).sqrt();
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            out_l += sign * gain * Self::read_interp(&self.early_buf_l, self.early_head, tap);
            out_r += sign * gain * Self::read_interp(&self.early_buf_r, self.early_head, tap + 0.7);
        }
        let n = Self::EARLY_PRIMES.len() as f64;
        (out_l / n, out_r / n)
    }

    /// Late reverb tail: 4+4 comb filters + 4 allpass per channel.
    #[allow(clippy::too_many_arguments)]
    fn process_late(
        &mut self,
        input: f64,
        size_m: f64,
        decay_s: f64,
        diffuse: f64,   // 0..1
        lfo_mod: f64,   // ±1 LFO value
        wander_ms: f64, // max wander depth in ms
        // Per-band decay multipliers (applied to feedback per band)
        low_xover_hz: f64,
        low_mult: f64,
        high_xover_hz: f64,
        high_mult: f64,
    ) -> (f64, f64) {
        // Size scale: 8 m = 1.0x, 32 m = 4.0x
        let size_factor = (size_m / 8.0).clamp(0.5, 4.0);

        // LP/HP coefficients for per-band decay shaping inside comb loops
        let lp_c = Self::lp_coeff(high_xover_hz, self.sr); // damp highs
        let hp_c = Self::hp_coeff(low_xover_hz, self.sr); // damp lows

        let mut sum_l = 0.0_f64;
        let mut sum_r = 0.0_f64;

        macro_rules! process_combs {
            ($buf:expr, $head:expr, $lp:expr, $hp:expr,
             $ms_table:expr, $lfo_phase_offset:expr, $sum:expr) => {
                for i in 0..4 {
                    let comb_ms = $ms_table[i] * size_factor;
                    let comb_s = comb_ms / 1000.0;

                    // Mid-band RT60 feedback
                    let fb_mid = (10.0_f64)
                        .powf(-3.0 * comb_s / decay_s.max(0.01))
                        .clamp(0.0, 0.9995);
                    // High-band feedback (shortened by high_mult < 1 typical)
                    let fb_high = (10.0_f64)
                        .powf(-3.0 * comb_s / (decay_s * high_mult).max(0.01))
                        .clamp(0.0, 0.9995);
                    // Low-band feedback
                    let fb_low = (10.0_f64)
                        .powf(-3.0 * comb_s / (decay_s * low_mult).max(0.01))
                        .clamp(0.0, 0.9995);

                    // LFO modulation of delay time
                    let phase_i = ($lfo_phase_offset + i as f64 * 0.25).rem_euclid(1.0);
                    let lfo_val = fast_sin_phase(phase_i) * lfo_mod;
                    let lfo_samp = lfo_val * wander_ms * self.sr / 1000.0;
                    let delay_samples = (comb_ms * self.sr / 1000.0 + lfo_samp)
                        .clamp(1.0, ($buf[i].len() - 2) as f64);

                    let len = $buf[i].len();
                    if len == 0 {
                        continue;
                    }

                    let delayed = Self::read_interp(&$buf[i], $head[i], delay_samples);

                    // Multi-band feedback: LP separates highs, HP separates lows
                    let high_comp = Self::lp_tick(&mut $lp[i], delayed, lp_c);
                    let low_comp = delayed - Self::lp_tick(&mut $hp[i], delayed, hp_c);
                    let mid_comp = delayed - high_comp - low_comp;

                    let fed = input + mid_comp * fb_mid + high_comp * fb_high + low_comp * fb_low;

                    $buf[i][$head[i]] = fed.clamp(-4.0, 4.0) as f32;
                    $head[i] = ($head[i] + 1) % len;
                    $sum += delayed;
                }
            };
        }

        process_combs!(
            self.comb_buf_l,
            self.comb_head_l,
            self.comb_lp_l,
            self.comb_hp_l,
            Self::COMB_MS_L,
            self.lfo_phase,
            sum_l
        );
        process_combs!(
            self.comb_buf_r,
            self.comb_head_r,
            self.comb_lp_r,
            self.comb_hp_r,
            Self::COMB_MS_R,
            self.lfo_phase + 0.5,
            sum_r
        );

        sum_l *= 0.25;
        sum_r *= 0.25;

        // 4 allpass sections per channel (Schroeder diffusion)
        let ap_fb = (0.25 + diffuse * 0.45).clamp(0.0, 0.7);
        for i in 0..4 {
            let ap_ms = Self::AP_MS[i] * size_factor;
            let ap_samp =
                (ap_ms * self.sr / 1000.0).clamp(1.0, (self.ap_buf_l[i].len() - 2) as f64);

            let len_l = self.ap_buf_l[i].len();
            let len_r = self.ap_buf_r[i].len();
            if len_l == 0 || len_r == 0 {
                continue;
            }

            // L allpass
            let delayed_l = Self::read_interp(&self.ap_buf_l[i], self.ap_head_l[i], ap_samp);
            let new_l = sum_l + delayed_l * ap_fb;
            self.ap_buf_l[i][self.ap_head_l[i]] = new_l.clamp(-4.0, 4.0) as f32;
            sum_l = delayed_l - new_l * ap_fb;
            self.ap_head_l[i] = (self.ap_head_l[i] + 1) % len_l;

            // R allpass (slightly different delay for decorrelation)
            let ap_samp_r = (ap_samp + 0.37).clamp(1.0, (len_r - 2) as f64);
            let delayed_r = Self::read_interp(&self.ap_buf_r[i], self.ap_head_r[i], ap_samp_r);
            let new_r = sum_r + delayed_r * ap_fb;
            self.ap_buf_r[i][self.ap_head_r[i]] = new_r.clamp(-4.0, 4.0) as f32;
            sum_r = delayed_r - new_r * ap_fb;
            self.ap_head_r[i] = (self.ap_head_r[i] + 1) % len_r;
        }

        (sum_l, sum_r)
    }
}

static REVERB_PARAMS: &[ParamDesc] = &[
    // ── Levels ──────────────────────────────────────────────────────
    ParamDesc {
        id: "mix",
        name: "Mix",
        default: 30.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "early",
        name: "Early Level",
        default: 50.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "early_send",
        name: "Early Send",
        default: 20.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "late",
        name: "Late Level",
        default: 70.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    // ── Room shape ──────────────────────────────────────────────────
    ParamDesc {
        id: "size",
        name: "Size",
        default: 12.0,
        min: 8.0,
        max: 32.0,
        options: None,
    },
    ParamDesc {
        id: "width",
        name: "Width",
        default: 100.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "predelay",
        name: "Predelay",
        default: 0.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    // ── Tail character ──────────────────────────────────────────────
    ParamDesc {
        id: "decay",
        name: "Decay",
        default: 2.0,
        min: 0.1,
        max: 10.0,
        options: None,
    },
    ParamDesc {
        id: "diffuse",
        name: "Diffuse",
        default: 70.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    // ── Modulation ──────────────────────────────────────────────────
    ParamDesc {
        id: "spin",
        name: "Spin",
        default: 1.0,
        min: 0.0,
        max: 10.0,
        options: None,
    },
    ParamDesc {
        id: "wander",
        name: "Wander",
        default: 8.0,
        min: 0.0,
        max: 40.0,
        options: None,
    },
    ParamDesc {
        id: "modulation",
        name: "Modulation",
        default: 50.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    // ── High-band shaping ───────────────────────────────────────────
    ParamDesc {
        id: "high_cut",
        name: "High Cut",
        default: 20000.0,
        min: 1000.0,
        max: 20000.0,
        options: None,
    },
    ParamDesc {
        id: "high_xover",
        name: "High Xover",
        default: 5000.0,
        min: 1000.0,
        max: 20000.0,
        options: None,
    },
    ParamDesc {
        id: "high_mult",
        name: "High Mult",
        default: 0.5,
        min: 0.1,
        max: 2.5,
        options: None,
    },
    // ── Low-band shaping ────────────────────────────────────────────
    ParamDesc {
        id: "low_cut",
        name: "Low Cut",
        default: 200.0,
        min: 20.0,
        max: 1200.0,
        options: None,
    },
    ParamDesc {
        id: "low_xover",
        name: "Low Xover",
        default: 600.0,
        min: 200.0,
        max: 1200.0,
        options: None,
    },
    ParamDesc {
        id: "low_mult",
        name: "Low Mult",
        default: 1.5,
        min: 0.1,
        max: 2.5,
        options: None,
    },
];

impl EffectModule for FxReverb {
    fn name(&self) -> &'static str {
        "Reverb"
    }
    fn params(&self) -> &'static [ParamDesc] {
        REVERB_PARAMS
    }

    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        if (self.sr - sr).abs() > 1.0 {
            self.sr = sr;
        }

        // ── Read all parameters ──────────────────────────────────────
        let mix = param_val(params, "mix", 30.0) as f64 / 100.0; // 0..1
        let early_level = param_val(params, "early", 50.0) as f64 / 100.0;
        let early_send = param_val(params, "early_send", 20.0) as f64 / 100.0;
        let late_level = param_val(params, "late", 70.0) as f64 / 100.0;
        let size_m = param_val(params, "size", 24.0) as f64; // 8..32 m
        let width = param_val(params, "width", 100.0) as f64 / 100.0; // 0..1
        let predelay_ms = param_val(params, "predelay", 0.0) as f64; // ms
        let decay_s = param_val(params, "decay", 2.0) as f64; // seconds
        let diffuse = param_val(params, "diffuse", 70.0) as f64 / 100.0; // 0..1
        let spin_hz = param_val(params, "spin", 1.0) as f64; // Hz
        let wander_ms = param_val(params, "wander", 8.0) as f64; // ms
        let modulation = param_val(params, "modulation", 50.0) as f64 / 100.0; // 0..1
        let high_cut_hz = param_val(params, "high_cut", 20000.0) as f64;
        let high_xover_hz = param_val(params, "high_xover", 5000.0) as f64;
        let high_mult = param_val(params, "high_mult", 0.5) as f64;
        let low_cut_hz = param_val(params, "low_cut", 200.0) as f64;
        let low_xover_hz = param_val(params, "low_xover", 600.0) as f64;
        let low_mult = param_val(params, "low_mult", 1.5) as f64;

        // ── Input tone shaping ───────────────────────────────────────
        // High-pass (low_cut) to remove sub-bass from reverb tail
        let hp_c = Self::hp_coeff(low_cut_hz, sr);
        // Simple 1-pole HP: y[n] = x[n] - x_prev + hp_c * y_prev
        let process_hp = |state_l: &mut f64, state_r: &mut f64, l: f64, r: f64| -> (f64, f64) {
            let yl = l - *state_l + hp_c * *state_l;
            let yr = r - *state_r + hp_c * *state_r;
            // Actually store prev_input for DC-blocking style HP
            *state_l = l;
            *state_r = r;
            (yl, yr)
        };
        let (hp_l, hp_r) = process_hp(&mut self.hp_state_l, &mut self.hp_state_r, left, right);

        // Low-pass (high_cut) to remove extreme highs from reverb tail
        let lp_c = Self::lp_coeff(high_cut_hz, sr);
        self.lp_state_l = (1.0 - lp_c) * hp_l + lp_c * self.lp_state_l;
        self.lp_state_r = (1.0 - lp_c) * hp_r + lp_c * self.lp_state_r;
        let shaped_l = self.lp_state_l;
        let shaped_r = self.lp_state_r;

        let mono_in = (shaped_l + shaped_r) * 0.5;

        // ── Predelay ─────────────────────────────────────────────────
        let pre_len = self.pre_buf.len();
        let pre_delayed = if pre_len > 1 {
            self.pre_buf[self.pre_head] = mono_in as f32;
            self.pre_head = (self.pre_head + 1) % pre_len;
            let pre_samples = (predelay_ms * sr / 1000.0 + 1.0).clamp(1.0, (pre_len - 1) as f64);
            let rp =
                (self.pre_head as f64 + pre_len as f64 - pre_samples).rem_euclid(pre_len as f64);
            let i0 = rp as usize % pre_len;
            let i1 = (i0 + 1) % pre_len;
            let frac = rp - rp.floor();
            self.pre_buf[i0] as f64 * (1.0 - frac) + self.pre_buf[i1] as f64 * frac
        } else {
            mono_in
        };

        // ── Spin LFO ─────────────────────────────────────────────────
        self.lfo_phase += spin_hz / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo_mod = fast_sin_phase(self.lfo_phase) * modulation;

        // ── Early reflections ─────────────────────────────────────────
        let (early_l, early_r) = if early_level > 0.001 || early_send > 0.001 {
            self.process_early(pre_delayed, size_m)
        } else {
            (0.0, 0.0)
        };

        // ── Late tail ─────────────────────────────────────────────────
        // Feed: predelayed mono + early_send fraction of early output
        let late_in = pre_delayed + (early_l + early_r) * 0.5 * early_send;
        let (late_l, late_r) = if late_level > 0.001 {
            self.process_late(
                late_in,
                size_m,
                decay_s,
                diffuse,
                lfo_mod,
                wander_ms,
                low_xover_hz,
                low_mult,
                high_xover_hz,
                high_mult,
            )
        } else {
            (0.0, 0.0)
        };

        // ── Mix early + late ──────────────────────────────────────────
        let wet_l = early_l * early_level + late_l * late_level;
        let wet_r = early_r * early_level + late_r * late_level;

        // ── Stereo width (mid/side) ───────────────────────────────────
        let mid = (wet_l + wet_r) * 0.5;
        let side = (wet_l - wet_r) * 0.5;
        let w_l = mid + side * width;
        let w_r = mid - side * width;

        // ── Dry/wet blend ─────────────────────────────────────────────
        (
            left * (1.0 - mix) + w_l * mix,
            right * (1.0 - mix) + w_r * mix,
        )
    }

    fn reset(&mut self) {
        for b in &mut self.pre_buf {
            *b = 0.0;
        }
        for b in &mut self.early_buf_l {
            *b = 0.0;
        }
        for b in &mut self.early_buf_r {
            *b = 0.0;
        }
        self.hp_state_l = 0.0;
        self.hp_state_r = 0.0;
        self.lp_state_l = 0.0;
        self.lp_state_r = 0.0;
        self.lfo_phase = 0.0;
        for i in 0..4 {
            for b in &mut self.comb_buf_l[i] {
                *b = 0.0;
            }
            for b in &mut self.comb_buf_r[i] {
                *b = 0.0;
            }
            self.comb_lp_l[i] = 0.0;
            self.comb_lp_r[i] = 0.0;
            self.comb_hp_l[i] = 0.0;
            self.comb_hp_r[i] = 0.0;
            for b in &mut self.ap_buf_l[i] {
                *b = 0.0;
            }
            for b in &mut self.ap_buf_r[i] {
                *b = 0.0;
            }
        }
    }

    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxReverb::new(self.sr as u32))
    }
}

// ── Chorus ──────────────────────────────────────────────────────────

pub struct FxChorus {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    phase: f64,
}
impl FxChorus {
    pub fn new(sr: u32) -> Self {
        Self {
            buf_l: vec![0.0; sr as usize],
            buf_r: vec![0.0; sr as usize],
            write_pos: 0,
            phase: 0.0,
        }
    }
}

static CHORUS_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "rate",
        name: "Rate",
        default: 0.5,
        min: 0.01,
        max: 5.0,
        options: None,
    },
    ParamDesc {
        id: "depth",
        name: "Depth",
        default: 0.005,
        min: 0.0,
        max: 0.02,
        options: None,
    },
    ParamDesc {
        id: "mix",
        name: "Mix",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxChorus {
    fn name(&self) -> &'static str {
        "Chorus"
    }
    fn params(&self) -> &'static [ParamDesc] {
        CHORUS_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let rate = param_val(params, "rate", 0.5) as f64;
        let depth = param_val(params, "depth", 0.005) as f64;
        let mix = param_val(params, "mix", 0.5) as f64;
        let len = self.buf_l.len();
        if len == 0 {
            return (left, right);
        }
        self.buf_l[self.write_pos] = left as f32;
        self.buf_r[self.write_pos] = right as f32;
        self.write_pos = (self.write_pos + 1) % len;
        let lfo = fast_sin_phase(self.phase);
        let lfo_r = fast_sin_phase((self.phase + 0.25) % 1.0); // 90° offset for stereo
        self.phase += rate / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let ds_l = (depth * sr * (1.0 + lfo * 0.5)).max(1.0);
        let ds_r = (depth * sr * (1.0 + lfo_r * 0.5)).max(1.0);
        let rp_l = (self.write_pos as f64 + len as f64 - ds_l) % len as f64;
        let rp_r = (self.write_pos as f64 + len as f64 - ds_r) % len as f64;
        let i0_l = rp_l as usize % len;
        let i1_l = (i0_l + 1) % len;
        let f_l = rp_l - rp_l.floor();
        let del_l = self.buf_l[i0_l] as f64 * (1.0 - f_l) + self.buf_l[i1_l] as f64 * f_l;
        let i0_r = rp_r as usize % len;
        let i1_r = (i0_r + 1) % len;
        let f_r = rp_r - rp_r.floor();
        let del_r = self.buf_r[i0_r] as f64 * (1.0 - f_r) + self.buf_r[i1_r] as f64 * f_r;
        (
            left * (1.0 - mix) + del_l * mix,
            right * (1.0 - mix) + del_r * mix,
        )
    }
    fn reset(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos = 0;
        self.phase = 0.0;
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxChorus::new(self.buf_l.len() as u32))
    }
}

// ── Distortion ──────────────────────────────────────────────────────

pub struct FxDistortion;

static DISTORTION_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "drive",
        name: "Drive",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "type",
        name: "Type",
        default: 0.0,
        min: 0.0,
        max: 3.0,
        options: None,
    },
    ParamDesc {
        id: "mix",
        name: "Mix",
        default: 1.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

fn distort_sample(input: f64, drive: f64, dtype: usize) -> f64 {
    match dtype {
        0 => {
            let d = 1.0 + drive * 15.0;
            let x = input * d;
            let tanh_x = fast_tanh(x);
            let tanh_d = fast_tanh(d);
            if tanh_d.abs() < 1e-9 {
                input
            } else {
                tanh_x / tanh_d
            }
        }
        1 => {
            let th = (1.0 - drive * 0.85).max(0.01);
            input.clamp(-th, th) / th
        }
        2 => fast_sin(input * (1.0 + drive * 5.0) * std::f64::consts::PI),
        3 => {
            let st = fast_pow2(14.0 - drive * 12.0).max(1.0);
            (input * st + 0.5).floor() / st
        }
        _ => input,
    }
}

impl EffectModule for FxDistortion {
    fn name(&self) -> &'static str {
        "Distortion"
    }
    fn params(&self) -> &'static [ParamDesc] {
        DISTORTION_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], _sr: f64) -> (f64, f64) {
        let drive = param_val(params, "drive", 0.5) as f64;
        let dtype = param_val(params, "type", 0.0) as usize;
        let mix = param_val(params, "mix", 1.0) as f64;
        if drive < 0.001 {
            return (left, right);
        }
        let dist_l = distort_sample(left, drive, dtype);
        let dist_r = distort_sample(right, drive, dtype);
        (
            left * (1.0 - mix) + dist_l * mix,
            right * (1.0 - mix) + dist_r * mix,
        )
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxDistortion)
    }
}

// ── Compressor ──────────────────────────────────────────────────────

pub struct FxCompressor {
    env: f64,
}
impl FxCompressor {
    pub fn new() -> Self {
        Self { env: 0.0 }
    }
}

static COMPRESSOR_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "threshold",
        name: "Threshold",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "ratio",
        name: "Ratio",
        default: 0.3,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "attack",
        name: "Attack",
        default: 0.01,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "release",
        name: "Release",
        default: 0.1,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "makeup",
        name: "Makeup",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxCompressor {
    fn name(&self) -> &'static str {
        "Compressor"
    }
    fn params(&self) -> &'static [ParamDesc] {
        COMPRESSOR_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        self.process_sidechain(left, right, left, right, params, sr)
    }
    fn process_sidechain(
        &mut self,
        left: f64,
        right: f64,
        key_l: f64,
        key_r: f64,
        params: &[(String, f32)],
        sr: f64,
    ) -> (f64, f64) {
        let threshold = param_val(params, "threshold", 0.5) as f64;
        let ratio = param_val(params, "ratio", 0.3) as f64;
        // Attack/release stored as 0–1 knob values; map to log-scale seconds.
        // attack 0..1 → ~0.3ms .. 300ms (log)
        // release 0..1 → ~5ms .. 2000ms (log)
        let attack_knob = param_val(params, "attack", 0.01) as f64;
        let release_knob = param_val(params, "release", 0.1) as f64;
        let attack = 0.0003 * fast_pow2(attack_knob * 9.965784284662087); // 0.3ms–300ms (log2(1000)≈9.966)
        let release = 0.005 * fast_pow2(release_knob * 8.643856189774724); // 5ms–2000ms (log2(400)≈8.644)
        let makeup = param_val(params, "makeup", 0.0) as f64;
        let thresh_db = -60.0 + threshold * 60.0;
        let ratio_val = 1.0 + ratio * 19.0;
        let makeup_db = makeup * 24.0;
        // Key signal: from sidechain source (or self if no sidechain)
        let target = key_l.abs().max(key_r.abs());
        let coeff = if target > self.env {
            fast_exp(-1.0 / (attack.max(0.001) * sr))
        } else {
            fast_exp(-1.0 / (release.max(0.001) * sr))
        };
        self.env = coeff * self.env + (1.0 - coeff) * target;
        let env_db = if self.env > 1e-10 {
            20.0 * fast_log10(self.env)
        } else {
            -120.0
        };
        let gain_db = if env_db > thresh_db {
            let over = env_db - thresh_db;
            -(over - over / ratio_val)
        } else {
            0.0
        };
        let lin = db_to_lin(gain_db + makeup_db);
        (left * lin, right * lin)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxCompressor::new())
    }
    fn gain_reduction_db(&self) -> f32 {
        // Compressor GR: env tracks the signal level in linear domain.
        // We can't recompute exact GR without params, so we store a
        // rough indicator. The actual GR is computed in process() and
        // would need an extra field to store. For now, return 0.0 and
        // let the visualization estimate GR from pre/post RMS.
        0.0
    }
}
// Brick-wall limiter: applies input gain, then hard-clips at ceiling.
// Ceiling is in dBFS (default 0 dBFS = no headroom above digital zero).
// Gain input allows loudness maximisation before the ceiling kicks in.

/// Lookahead brickwall limiter.
///
/// Uses a lookahead buffer so gain reduction ramps smoothly BEFORE
/// peaks arrive, preventing any overshoot.  Inspired by the LSP
/// limiter design (attack ramp over the lookahead window, smooth
/// log-domain release).
pub struct FxLimiter {
    /// Ring buffer for delayed left/right samples (interleaved: L0 R0 L1 R1 …)
    delay_buf: Vec<f64>,
    /// Ring buffer for per-sample gain reduction (linear, ≤1.0)
    gr_buf: Vec<f64>,
    /// Current write position in the ring buffers
    write_pos: usize,
    /// Lookahead length in samples (set from sample rate)
    lookahead: usize,
    /// Smoothed envelope (log-domain) for release
    env: f64,
}

impl FxLimiter {
    pub fn new() -> Self {
        // Start with empty buffers — they are resized on first process call
        Self {
            delay_buf: Vec::new(),
            gr_buf: Vec::new(),
            write_pos: 0,
            lookahead: 0,
            env: 1.0, // 1.0 = no gain reduction (NOT 0.0 which would silence everything)
        }
    }

    /// Ensure internal buffers match the current sample rate.
    fn ensure_buffers(&mut self, sr: f64) {
        // 5 ms lookahead (common for transparent limiting)
        let la = ((sr * 0.005).round() as usize).max(1);
        if la != self.lookahead {
            self.lookahead = la;
            self.delay_buf = vec![0.0; la * 2]; // interleaved L/R
            self.gr_buf = vec![1.0; la]; // gain reduction (1.0 = no reduction)
            self.write_pos = 0;
            self.env = 1.0; // reset to no reduction on SR change
        }
    }
}

static LIMITER_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "gain_db",
        name: "Input Gain",
        default: 0.0,
        min: 0.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "ceiling_db",
        name: "Ceiling",
        default: 0.0,
        min: -12.0,
        max: 0.0,
        options: None,
    },
    ParamDesc {
        id: "release",
        name: "Release",
        default: 0.05,
        min: 0.001,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxLimiter {
    fn name(&self) -> &'static str {
        "Limiter"
    }
    fn params(&self) -> &'static [ParamDesc] {
        LIMITER_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        self.ensure_buffers(sr);

        let gain_db = param_val(params, "gain_db", 0.0) as f64;
        let ceiling_db = param_val(params, "ceiling_db", 0.0) as f64;
        let release_knob = param_val(params, "release", 0.05) as f64;

        let input_gain = db_to_lin(gain_db);
        let ceiling_lin = db_to_lin(ceiling_db);
        let release_coeff = fast_exp(-1.0 / (release_knob.max(0.001) * sr));

        let la = self.lookahead;

        // Apply input gain
        let il = left * input_gain;
        let ir = right * input_gain;

        // Peak of incoming sample
        let peak = il.abs().max(ir.abs());

        // Compute required gain reduction for this sample
        let needed_gr = if peak > ceiling_lin && peak > 1e-10 {
            ceiling_lin / peak
        } else {
            1.0
        };

        // If this peak needs limiting, ramp gain reduction over the
        // lookahead window so it's fully applied by the time the
        // delayed audio arrives at the output.
        if needed_gr < 1.0 {
            // Linear attack ramp: from 1.0 down to needed_gr over `la` samples
            for k in 0..la {
                let t = (k + 1) as f64 / la as f64; // 1/la .. 1.0
                let ramped = 1.0 - t * (1.0 - needed_gr); // ramp from 1.0 to needed_gr
                let idx = (self.write_pos + k) % la;
                if ramped < self.gr_buf[idx] {
                    self.gr_buf[idx] = ramped;
                }
            }
        }

        // Read the oldest sample from the delay buffer (= lookahead delay)
        let read_pos = self.write_pos;
        let dl = self.delay_buf[read_pos * 2];
        let dr = self.delay_buf[read_pos * 2 + 1];

        // Read gain reduction for this output sample
        let mut gr = self.gr_buf[read_pos];

        // Smooth release: don't let gain reduction jump back to 1.0 instantly
        self.env = if gr < self.env {
            gr // attack: follow instantly (already ramped by lookahead)
        } else {
            release_coeff * self.env + (1.0 - release_coeff) * gr
        };
        gr = self.env;

        // Write new sample into the delay buffer
        self.delay_buf[self.write_pos * 2] = il;
        self.delay_buf[self.write_pos * 2 + 1] = ir;
        // Reset gain reduction buffer for future use (will be written again by future peaks)
        self.gr_buf[self.write_pos] = 1.0;

        // Advance write position
        self.write_pos = (self.write_pos + 1) % la;

        // Apply gain reduction and hard-clip as safety net
        let ol = (dl * gr).clamp(-ceiling_lin, ceiling_lin);
        let or_ = (dr * gr).clamp(-ceiling_lin, ceiling_lin);
        (ol, or_)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxLimiter::new())
    }
    fn gain_reduction_db(&self) -> f32 {
        // env is linear gain (1.0 = no reduction, <1.0 = reducing)
        if self.env > 1e-10 && self.env < 1.0 {
            (20.0 * self.env.log10()) as f32
        } else if self.env <= 1e-10 {
            -60.0
        } else {
            0.0
        }
    }
}

pub struct FxEq {
    lo_ic1_l: f64,
    lo_ic2_l: f64,
    hi_ic1_l: f64,
    hi_ic2_l: f64,
    lo_ic1_r: f64,
    lo_ic2_r: f64,
    hi_ic1_r: f64,
    hi_ic2_r: f64,
}
impl FxEq {
    pub fn new() -> Self {
        Self {
            lo_ic1_l: 0.0,
            lo_ic2_l: 0.0,
            hi_ic1_l: 0.0,
            hi_ic2_l: 0.0,
            lo_ic1_r: 0.0,
            lo_ic2_r: 0.0,
            hi_ic1_r: 0.0,
            hi_ic2_r: 0.0,
        }
    }
}

static EQ_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "lo_gain",
        name: "Lo Gain",
        default: 0.0,
        min: -12.0,
        max: 12.0,
        options: None,
    },
    ParamDesc {
        id: "mid_gain",
        name: "Mid Gain",
        default: 0.0,
        min: -12.0,
        max: 12.0,
        options: None,
    },
    ParamDesc {
        id: "hi_gain",
        name: "Hi Gain",
        default: 0.0,
        min: -12.0,
        max: 12.0,
        options: None,
    },
    ParamDesc {
        id: "lo_freq",
        name: "Lo Freq",
        default: 200.0,
        min: 20.0,
        max: 500.0,
        options: None,
    },
    ParamDesc {
        id: "hi_freq",
        name: "Hi Freq",
        default: 4000.0,
        min: 1000.0,
        max: 16000.0,
        options: None,
    },
];

impl EffectModule for FxEq {
    fn name(&self) -> &'static str {
        "EQ"
    }
    fn params(&self) -> &'static [ParamDesc] {
        EQ_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let lo_g = param_val(params, "lo_gain", 0.0) as f64;
        let mid_g = param_val(params, "mid_gain", 0.0) as f64;
        let hi_g = param_val(params, "hi_gain", 0.0) as f64;
        let lo_f = (param_val(params, "lo_freq", 200.0) as f64).clamp(20.0, sr * 0.49);
        let hi_f = (param_val(params, "hi_freq", 4000.0) as f64).clamp(20.0, sr * 0.49);
        let lo_gain = db_to_lin(lo_g);
        let mid_gain = db_to_lin(mid_g);
        let hi_gain = db_to_lin(hi_g);
        // Left channel
        let (lo_l, _, _) = svf_tick(left, lo_f, 0.5, sr, &mut self.lo_ic1_l, &mut self.lo_ic2_l);
        let (_, _, hi_l) = svf_tick(left, hi_f, 0.5, sr, &mut self.hi_ic1_l, &mut self.hi_ic2_l);
        let mid_l = left - lo_l - hi_l;
        let out_l = lo_l * lo_gain + mid_l * mid_gain + hi_l * hi_gain;
        // Right channel
        let (lo_r, _, _) = svf_tick(right, lo_f, 0.5, sr, &mut self.lo_ic1_r, &mut self.lo_ic2_r);
        let (_, _, hi_r) = svf_tick(right, hi_f, 0.5, sr, &mut self.hi_ic1_r, &mut self.hi_ic2_r);
        let mid_r = right - lo_r - hi_r;
        let out_r = lo_r * lo_gain + mid_r * mid_gain + hi_r * hi_gain;
        (out_l, out_r)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxEq::new())
    }
}

// ── Gain ─────────────────────────────────────────────────────────────

pub struct FxGain;

static GAIN_PARAMS: &[ParamDesc] = &[ParamDesc {
    id: "gain_db",
    name: "Gain dB",
    default: 0.0,
    min: -60.0,
    max: 24.0,
    options: None,
}];

impl EffectModule for FxGain {
    fn name(&self) -> &'static str {
        "Gain"
    }
    fn params(&self) -> &'static [ParamDesc] {
        GAIN_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], _sr: f64) -> (f64, f64) {
        let db = param_val(params, "gain_db", 0.0) as f64;
        let g = db_to_lin(db);
        (left * g, right * g)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxGain)
    }
}

// ── Utility (Gain + Pan + Phase Invert + DC Offset) ─────────────────

pub struct FxUtility;

static UTILITY_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "gain_db",
        name: "Gain dB",
        default: 0.0,
        min: -60.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "pan",
        name: "Pan",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "phase",
        name: "Phase",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "dc_offset",
        name: "DC Offset",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxUtility {
    fn name(&self) -> &'static str {
        "Utility"
    }
    fn params(&self) -> &'static [ParamDesc] {
        UTILITY_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], _sr: f64) -> (f64, f64) {
        let db = param_val(params, "gain_db", 0.0) as f64;
        let pan = param_val(params, "pan", 0.0) as f64;
        let phase_inv = param_val(params, "phase", 0.0);
        let dc = param_val(params, "dc_offset", 0.0) as f64;
        let gain = db_to_lin(db);
        let polarity = if phase_inv > 0.5 { -1.0 } else { 1.0 };

        // Balance law: preserve stereo image, just attenuate one side.
        // pan = -1 → cut right fully, 0 → unity both sides, +1 → cut left fully.
        // Use a linear balance (simple L/R attenuation), not equal-power pan.
        let pan_l = if pan > 0.0 { 1.0 - pan } else { 1.0 };
        let pan_r = if pan < 0.0 { 1.0 + pan } else { 1.0 };

        let out_l = (left + dc) * gain * polarity * pan_l;
        let out_r = (right + dc) * gain * polarity * pan_r;
        (out_l, out_r)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxUtility)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Registry — maps names to constructors
// ═══════════════════════════════════════════════════════════════════

pub fn create_instrument(name: &str) -> Option<Box<dyn InstrumentModule>> {
    match name {
        "Analog" => Some(Box::new(SubtractiveSynth)),
        "HyperSaw" => Some(Box::new(SuperSawSynth)),
        "Sampler" => Some(Box::new(Sampler)),
        "Monolith" => Some(Box::new(HeavySynth)),
        _ => None,
    }
}

pub fn create_effect(name: &str, sr: u32) -> Option<Box<dyn EffectModule>> {
    match name {
        "LP Filter" => Some(Box::new(FxLpFilter::new())),
        "HP Filter" => Some(Box::new(FxHpFilter::new())),
        "Delay" => Some(Box::new(FxDelay::new(sr))),
        "Reverb" => Some(Box::new(FxReverb::new(sr))),
        "Chorus" => Some(Box::new(FxChorus::new(sr))),
        "Distortion" => Some(Box::new(FxDistortion)),
        "Compressor" => Some(Box::new(FxCompressor::new())),
        "EQ" => Some(Box::new(FxEq::new())),
        "Gain" => Some(Box::new(FxGain)),
        "Utility" => Some(Box::new(FxUtility)),
        "Limiter" => Some(Box::new(FxLimiter::new())),
        _ => None,
    }
}

pub fn is_instrument(name: &str) -> bool {
    matches!(name, "Analog" | "HyperSaw" | "Sampler" | "Monolith")
}

pub fn is_effect(name: &str) -> bool {
    matches!(
        name,
        "LP Filter"
            | "HP Filter"
            | "Delay"
            | "Reverb"
            | "Chorus"
            | "Distortion"
            | "Compressor"
            | "EQ"
            | "Gain"
            | "Utility"
            | "Limiter"
    )
}

pub fn is_midi_effect(name: &str) -> bool {
    matches!(name, "Arpeggiator" | "Chord" | "Transpose" | "Velocity")
}

pub fn get_param_descs(name: &str) -> &'static [ParamDesc] {
    match name {
        "Analog" => SUBTRACTIVE_PARAMS,
        "HyperSaw" => SUPERSAW_PARAMS,
        "Sampler" => SAMPLER_PARAMS,
        "Monolith" => HEAVY_PARAMS,
        "LP Filter" => LP_FILTER_PARAMS,
        "HP Filter" => HP_FILTER_PARAMS,
        "Delay" => DELAY_PARAMS,
        "Reverb" => REVERB_PARAMS,
        "Chorus" => CHORUS_PARAMS,
        "Distortion" => DISTORTION_PARAMS,
        "Compressor" => COMPRESSOR_PARAMS,
        "EQ" => EQ_PARAMS,
        "Gain" => GAIN_PARAMS,
        "Utility" => UTILITY_PARAMS,
        "Limiter" => LIMITER_PARAMS,
        _ => &[],
    }
}
