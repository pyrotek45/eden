// Eden DAW — DSP core helpers
//
// Constants, DC-HP filter, MIDI helpers, panning, parameter smoothing.

use crate::modules::{MidiContext, MidiEffect, MidiEvent};

// ── Constants ────────────────────────────────────────────────────────────────

/// DC-offset one-pole HP filter coefficient (fc ≈ 20 Hz)
pub const DC_HP_R: f64 = 0.99972;

/// Equal-power micro-fade length at clip boundaries (~5 ms = 220 samples at 44.1kHz)
pub(crate) const CLIP_FADE_SAMPLES: usize = 220;

// ── MIDI effect chain helper ─────────────────────────────────────────────────

/// Run `events` through every MIDI effect instance in `chain`, using the
/// matching param slices from `param_slices`.  Returns the final event list.
pub fn run_midi_chain<'a>(
    mut events: Vec<MidiEvent>,
    chain: &'a mut [Box<dyn MidiEffect>],
    param_slices: impl Iterator<Item = &'a Vec<(String, f32)>>,
    pos_beats: f64,
    prev_beats: f64,
    bpm: f64,
    sample_rate: f64,
) -> Vec<MidiEvent> {
    for (fx, params) in chain.iter_mut().zip(param_slices) {
        let ctx = MidiContext {
            pos_beats,
            prev_beats,
            bpm,
            sample_rate,
            params: params.as_slice(),
        };
        events = fx.process(events, &ctx);
    }
    events
}

// ── MIDI → frequency ─────────────────────────────────────────────────────────

#[inline]
pub fn midi_to_freq(pitch: u8) -> f64 {
    440.0 * crate::modules::fast_pow2((pitch as f64 - 69.0) / 12.0)
}

// ── DC HP filter state ───────────────────────────────────────────────────────

/// State for the DC-offset one-pole high-pass filter (stereo).
pub struct DcHpState {
    pub x_l: f64,
    pub y_l: f64,
    pub x_r: f64,
    pub y_r: f64,
}

impl DcHpState {
    pub fn new() -> Self {
        Self {
            x_l: 0.0,
            y_l: 0.0,
            x_r: 0.0,
            y_r: 0.0,
        }
    }

    /// Apply one-pole HP filter (fc ≈ 20 Hz). Removes slowly-drifting DC bias.
    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let new_l = l - self.x_l + DC_HP_R * self.y_l;
        self.x_l = l;
        self.y_l = new_l;
        let new_r = r - self.x_r + DC_HP_R * self.y_r;
        self.x_r = r;
        self.y_r = new_r;
        (new_l, new_r)
    }
}

// ── Panning ──────────────────────────────────────────────────────────────────

/// Apply equal-power panning and volume to a track's signal and add to stereo mix.
/// Returns the contribution (left, right) for this track.
#[inline]
pub fn pan_and_mix(signal: (f64, f64), pan: f64, volume: f64) -> (f64, f64) {
    let theta = (pan + 1.0) * 0.5 * std::f64::consts::FRAC_PI_2;
    let pan_l = crate::modules::fast_cos(theta) * std::f64::consts::SQRT_2;
    let pan_r = crate::modules::fast_sin(theta) * std::f64::consts::SQRT_2;
    (signal.0 * pan_l * volume, signal.1 * pan_r * volume)
}

// ── Parameter smoothing ──────────────────────────────────────────────────────

/// Smooth a flat parameter list toward its target values using an
/// exponential moving average.  New parameters are initialised to their
/// target value (no ramp from zero on first appearance).
///
/// Used for per-track effect params, master rack params, and CStrip2
/// channel-strip params.
pub fn smooth_params(cache: &mut Vec<(String, f32)>, target: &[(String, f32)], coeff: f32) {
    // Grow the cache if new params appeared
    if cache.len() < target.len() {
        for item in target.iter().skip(cache.len()) {
            cache.push(item.clone());
        }
    }
    // Smooth each value toward target
    for (pi, p) in target.iter().enumerate() {
        if pi < cache.len() {
            cache[pi].1 += (p.1 - cache[pi].1) * coeff;
        }
    }
}
