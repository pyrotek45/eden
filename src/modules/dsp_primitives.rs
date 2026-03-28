// Eden DAW — DSP primitives shared by all modules
//
// Fast math, oscillators, envelopes, filters, parameter smoothing.
// These are low-level building blocks; module implementations import them via super::.

// ── Denormal protection ──
pub(crate) const DENORMAL_FIX: f64 = 1.0e-18;

// ── Sine lookup table ──
const SINE_TABLE_SIZE: usize = 2048;
static SINE_TABLE: std::sync::LazyLock<[f64; SINE_TABLE_SIZE + 1]> =
    std::sync::LazyLock::new(|| {
        let mut table = [0.0f64; SINE_TABLE_SIZE + 1];
        for (i, entry) in table.iter_mut().enumerate() {
            *entry = (i as f64 / SINE_TABLE_SIZE as f64 * std::f64::consts::TAU).sin();
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
    let x2 = x * x;
    x * (15.0 - x2) / (15.0 - 6.0 * x2)
}

/// Fast exponential approximation.
#[inline(always)]
pub fn fast_exp(x: f64) -> f64 {
    let x = x.clamp(-700.0, 700.0);
    if x < -20.0 {
        return x.exp();
    }
    x.exp()
}

/// Fast 2^x approximation for frequency calculations.
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
    let frac = 1.0 + xf * (std::f64::consts::LN_2 + xf * (0.2402265 + xf * 0.0554961));
    frac * (2.0_f64).powi(xi)
}

/// Fast dB to linear conversion: 10^(db/20) = 2^(db * log2(10) / 20)
#[inline(always)]
pub fn db_to_lin(db: f64) -> f64 {
    fast_pow2(db * 0.16609640474436813)
}

/// Fast tanh approximation (Padé approximant).
#[inline(always)]
pub fn fast_tanh(x: f64) -> f64 {
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Fast log10 approximation for envelope dB calculation.
#[inline(always)]
pub fn fast_log10(x: f64) -> f64 {
    x.log2() * 0.30102999566398114
}

pub fn param_val(params: &[(String, f32)], id: &str, default: f32) -> f32 {
    params
        .iter()
        .find(|(k, _)| k == id)
        .map(|(_, v)| *v)
        .unwrap_or(default)
}

// ── Per-sample parameter smoother ──────────────────────────────────
//
// RULE: Every continuous parameter in every effect that can be
// automated MUST use a SmoothedParam.

/// One-pole parameter smoother.
#[derive(Clone, Copy, Debug)]
pub struct SmoothedParam {
    /// Current (smoothed) value output each sample.
    current: f64,
    /// Target value set from the param list each sample.
    target: f64,
    /// One-pole coefficient.
    coeff: f64,
}

impl SmoothedParam {
    /// Create a new smoother starting at `initial` with a ~5 ms ramp.
    #[inline]
    pub fn new(initial: f64, sr: f64) -> Self {
        Self {
            current: initial,
            target: initial,
            coeff: Self::coeff_for_ms(5.0, sr),
        }
    }

    #[inline]
    fn coeff_for_ms(ms: f64, sr: f64) -> f64 {
        if ms <= 0.0 || sr <= 0.0 {
            return 1.0;
        }
        let samples = ms * 0.001 * sr;
        1.0 - fast_exp(-1.0 / samples)
    }

    /// Set the target and advance one sample.  Returns the smoothed value.
    #[inline(always)]
    pub fn tick(&mut self, target: f64) -> f64 {
        self.target = target;
        self.current += self.coeff * (self.target - self.current);
        self.current
    }

    /// Snap immediately to a value (used on reset / fresh).
    #[inline]
    pub fn snap(&mut self, val: f64) {
        self.current = val;
        self.target = val;
    }

    /// Re-calibrate the smoothing coefficient for a new sample rate.
    #[inline]
    pub fn set_sample_rate(&mut self, sr: f64) {
        self.coeff = Self::coeff_for_ms(5.0, sr);
    }
}

/// Convert dB to linear gain.  Used by output gain knobs on effects and synths.
#[inline(always)]
pub fn db_to_linear(db: f64) -> f64 {
    if db <= -60.0 {
        0.0
    } else {
        db_to_lin(db)
    }
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
            let mut s = *noise;
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            *noise = s;
            let out = (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64;
            out * 2.0 - 1.0
        }
    }
}

/// Morphing oscillator: shape is 0.0–4.0 continuous.
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

use super::EnvStage;

#[allow(clippy::too_many_arguments)]
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
            let coeff = fast_exp(-dt / (r * 0.3));
            *level *= coeff;
            if *level <= 0.00001 {
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
