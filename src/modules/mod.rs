// Eden DAW — Module System
//
// Every module (instrument or effect) implements a trait.  The audio
// engine never matches on module names — it only calls trait methods.
//
// ╔══════════════════════════════════════════════════════════════════╗
// ║  Adding a new effect:                                           ║
// ║  1. Create effects/my_fx.rs implementing EffectModule trait.    ║
// ║  2. Add `pub mod my_fx;` + `pub use my_fx::*;` in effects/mod. ║
// ║  3. Add its name to EFFECT_NAMES below.                         ║
// ║  4. Add a match arm in create_effect() below.                   ║
// ║  5. Add a match arm in get_param_descs() below.                 ║
// ║  That's it — the UI, is_effect(), and module browser all        ║
// ║  derive from EFFECT_NAMES automatically.                        ║
// ║                                                                 ║
// ║  Adding a new instrument: same pattern with INSTRUMENT_NAMES,   ║
// ║  create_instrument(), and get_param_descs().                    ║
// ╚══════════════════════════════════════════════════════════════════╝

pub mod dsp_primitives;
pub mod effects;
pub mod instruments;
pub mod midi_effects;

// Re-export everything from sub-modules so existing `use crate::modules::*` works.
pub use dsp_primitives::*;
pub use effects::*;
#[allow(unused_imports)]
pub use instruments::*;
pub use midi_effects::*;

use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════
// MIDI Event
// ═══════════════════════════════════════════════════════════════════

/// A single MIDI note event flowing through the MIDI effect chain.
/// pitch: 0–127, velocity: 0.0–1.0 (normalised), original_pitch: the
/// pitch before any prior effects (used for note-off tracking).
#[derive(Clone, Debug)]
pub struct MidiEvent {
    pub pitch: u8,
    pub velocity: f32, // 0.0–1.0
    /// The source clip/keyboard pitch — set once, never modified by effects.
    pub original_pitch: u8,
}

impl MidiEvent {
    pub fn new(pitch: u8, velocity: f32) -> Self {
        Self {
            pitch,
            velocity,
            original_pitch: pitch,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// MIDI Context
// ═══════════════════════════════════════════════════════════════════

/// Read-only context passed to every MIDI effect process call.
pub struct MidiContext<'a> {
    /// Current playback position in beats.
    pub pos_beats: f64,
    /// Previous position in beats (for edge detection).
    pub prev_beats: f64,
    /// Tempo in BPM (needed for time-based effects like the Arpeggiator).
    pub bpm: f64,
    /// Sample rate (for sample-accurate timing).
    pub sample_rate: f64,
    /// Flattened (param_id, value) slice from the rack slot.
    pub params: &'a [(String, f32)],
}

impl MidiContext<'_> {
    /// Helper: look up a param by id, returning `default` if not found.
    pub fn get(&self, key: &str) -> f32 {
        self.params
            .iter()
            .find(|(id, _)| id == key)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    }
}

// ═══════════════════════════════════════════════════════════════════
// MIDI Effect trait
// ═══════════════════════════════════════════════════════════════════

pub trait MidiEffect: Send + Sync {
    fn name(&self) -> &'static str;
    /// Process one batch of events.  `ctx` supplies position/BPM/params.
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent>;
    /// Reset internal state (called on seek / loop boundary).
    fn reset(&mut self) {}
    /// True if this effect manages its own voice lifetime (like Arpeggiator).
    fn manages_voices(&self) -> bool {
        false
    }
    /// Clone into a fresh instance with zeroed state (for render pipeline).
    fn fresh(&self) -> Box<dyn MidiEffect>;
}

// ═══════════════════════════════════════════════════════════════════
// Voice & State types
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Debug)]
pub struct ModuleVoice {
    pub freq: f64,
    pub velocity: f32, // 0..1
    pub track_idx: usize,
    pub released: bool,
    pub pitch: u8,
    /// The original (pre-MIDI-effect) MIDI pitch used for note-off matching.
    pub original_pitch: u8,
    /// Opaque per-voice state owned by the instrument module.
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
    /// Noise highpass filter state (SuperSaw noise_hp knob).
    pub noise_hp_ic1: f64,
    pub noise_hp_ic2: f64,
    /// Second noise HP filter state (for synths with two oscillators).
    pub noise_hp_ic1b: f64,
    pub noise_hp_ic2b: f64,
}

impl Default for VoiceState {
    fn default() -> Self {
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        Self::with_seed(seed)
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

impl VoiceState {
    /// Create a VoiceState with a deterministic seed. Used for offline rendering
    /// so that two renders of the same project produce identical output.
    pub fn with_seed(seed: u64) -> Self {
        let h0 = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let h1 = h0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let p0 = h0 as f64 / u64::MAX as f64;
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
            noise_hp_ic1: 0.0,
            noise_hp_ic2: 0.0,
            noise_hp_ic1b: 0.0,
            noise_hp_ic2b: 0.0,
        }
    }
}

impl ModuleVoice {
    pub fn new(freq: f64, velocity: f32, track_idx: usize, pitch: u8) -> Self {
        Self {
            freq,
            velocity,
            track_idx,
            released: false,
            pitch,
            original_pitch: pitch,
            state: VoiceState::with_seed(track_idx as u64 * 2053 + pitch as u64 * 6271),
            preview_samples_remaining: None,
        }
    }
}

pub fn voice_is_done(v: &ModuleVoice) -> bool {
    v.state.amp_stage == EnvStage::Off
}

// ═══════════════════════════════════════════════════════════════════
// Param descriptor
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

// ═══════════════════════════════════════════════════════════════════
// Traits — Instrument & Effect
// ═══════════════════════════════════════════════════════════════════

/// An instrument module generates audio from MIDI voices.
pub trait InstrumentModule: Send + Sync {
    fn name(&self) -> &'static str;
    fn params(&self) -> &'static [ParamDesc];
    /// Process a single voice for one sample.  Returns `(left, right)`.
    fn process_voice(
        &self,
        voice: &mut ModuleVoice,
        params: &[(String, f32)],
        sample_rate: f64,
        extra: &ModuleExtra,
    ) -> (f64, f64);
}

/// An effect module processes one sample of audio (stereo).
pub trait EffectModule: Send + Sync {
    fn name(&self) -> &'static str;
    fn params(&self) -> &'static [ParamDesc];
    fn process(
        &mut self,
        left: f64,
        right: f64,
        params: &[(String, f32)],
        sample_rate: f64,
    ) -> (f64, f64);
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
    fn fresh(&self) -> Box<dyn EffectModule>;
    fn gain_reduction_db(&self) -> f32 {
        0.0
    }
    fn reset(&mut self) {}
    fn has_tail(&self) -> bool {
        false
    }
    fn set_bpm(&mut self, _bpm: f64) {}
}

/// Extra data passed into instrument processing (e.g. sampler buffers).
#[derive(Clone, Default, Debug)]
pub struct ModuleExtra {
    pub sample_data: Option<Arc<Vec<f32>>>,
    pub sample_sr: u32,
}

// ═══════════════════════════════════════════════════════════════════
// Registry — factory functions
// ═══════════════════════════════════════════════════════════════════

/// Canonical list of instrument names.  Add new instruments here.
pub const INSTRUMENT_NAMES: &[&str] = &["Analog", "HyperSaw", "Monolith", "Sampler"];

/// Canonical list of effect names.  Add new effects here.
pub const EFFECT_NAMES: &[&str] = &[
    "LP Filter",
    "HP Filter",
    "Delay",
    "Reverb",
    "Chorus",
    "Distortion",
    "Compressor",
    "EQ",
    "Gain",
    "Utility",
    "Limiter",
    "Autoduck",
    "CStrip2",
];

/// Canonical list of MIDI effect names.
pub const MIDI_EFFECT_NAMES: &[&str] = &["Arpeggiator", "Chord", "Transpose", "Velocity"];

pub fn create_instrument(name: &str) -> Option<Box<dyn InstrumentModule>> {
    match name {
        "Analog" => Some(Box::new(instruments::SubtractiveSynth)),
        "HyperSaw" => Some(Box::new(instruments::SuperSawSynth)),
        "Sampler" => Some(Box::new(instruments::Sampler)),
        "Monolith" => Some(Box::new(instruments::HeavySynth)),
        _ => None,
    }
}

pub fn create_effect(name: &str, sr: u32) -> Option<Box<dyn EffectModule>> {
    match name {
        "LP Filter" => Some(Box::new(effects::FxLpFilter::new())),
        "HP Filter" => Some(Box::new(effects::FxHpFilter::new())),
        "Delay" => Some(Box::new(effects::FxDelay::new(sr))),
        "Reverb" => Some(Box::new(effects::FxReverb::new(sr))),
        "Chorus" => Some(Box::new(effects::FxChorus::new(sr))),
        "Distortion" => Some(Box::new(effects::FxDistortion::new())),
        "Compressor" => Some(Box::new(effects::FxCompressor::new())),
        "EQ" => Some(Box::new(effects::FxEq::new())),
        "Gain" => Some(Box::new(effects::FxGain::new())),
        "Utility" => Some(Box::new(effects::FxUtility::new())),
        "Limiter" => Some(Box::new(effects::FxLimiter::new())),
        "Autoduck" => Some(Box::new(effects::FxAutoduck::new())),
        "CStrip2" => Some(Box::new(effects::CStrip2::new())),
        _ => None,
    }
}

pub fn is_instrument(name: &str) -> bool {
    INSTRUMENT_NAMES.contains(&name)
}

pub fn is_effect(name: &str) -> bool {
    EFFECT_NAMES.contains(&name)
}

pub fn is_midi_effect(name: &str) -> bool {
    MIDI_EFFECT_NAMES.contains(&name)
}

pub fn get_param_descs(name: &str) -> &'static [ParamDesc] {
    match name {
        "Analog" => instruments::SUBTRACTIVE_PARAMS,
        "HyperSaw" => instruments::SUPERSAW_PARAMS,
        "Sampler" => instruments::SAMPLER_PARAMS,
        "Monolith" => instruments::HEAVY_PARAMS,
        "LP Filter" => effects::LP_FILTER_PARAMS,
        "HP Filter" => effects::HP_FILTER_PARAMS,
        "Delay" => effects::DELAY_PARAMS,
        "Reverb" => effects::REVERB_PARAMS,
        "Chorus" => effects::CHORUS_PARAMS,
        "Distortion" => effects::DISTORTION_PARAMS,
        "Compressor" => effects::COMPRESSOR_PARAMS,
        "EQ" => effects::EQ_PARAMS,
        "Gain" => effects::GAIN_PARAMS,
        "Utility" => effects::UTILITY_PARAMS,
        "Limiter" => effects::LIMITER_PARAMS,
        "Autoduck" => effects::AUTODUCK_PARAMS,
        "CStrip2" => effects::CSTRIP2_PARAMS,
        _ => &[],
    }
}
