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
// MIDI Effect trait — modular, chainable MIDI processing
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

/// A MIDI effect processes a list of MIDI events and returns a (possibly
/// different) list.  Implementations may hold internal state (e.g. the
/// Arpeggiator step counter).
///
/// # Adding a new MIDI effect
/// 1. Create a struct implementing this trait.
/// 2. Register it in `create_midi_effect()` below.
/// 3. Add a `RackSlot` constructor in `models.rs` + `create_rack_slot_for_module`.
/// 4. Add it to the "MIDI Effects" category list in `views.rs`.
///
/// That's it — **no changes to audio.rs or render.rs**.
pub trait MidiEffect: Send + Sync {
    fn name(&self) -> &'static str;
    /// Process one batch of events.  `ctx` supplies position/BPM/params.
    /// Returns the transformed event list (may be larger, smaller, or empty).
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent>;
    /// Reset internal state (called on seek / loop boundary).
    fn reset(&mut self) {}
    /// True if this effect manages its own voice lifetime (like Arpeggiator).
    /// The audio engine skips the normal still_active check for these tracks.
    fn manages_voices(&self) -> bool {
        false
    }
    /// Clone into a fresh instance with zeroed state (for render pipeline).
    fn fresh(&self) -> Box<dyn MidiEffect>;
}

/// Instantiate a MIDI effect by name.  Returns `None` for unknown names.
pub fn create_midi_effect(name: &str) -> Option<Box<dyn MidiEffect>> {
    match name {
        "Transpose" => Some(Box::new(MfxTranspose)),
        "Velocity" => Some(Box::new(MfxVelocity)),
        "Chord" => Some(Box::new(MfxChord)),
        "Arpeggiator" => Some(Box::new(MfxArpeggiator::new())),
        _ => None,
    }
}

// ── Transpose ────────────────────────────────────────────────────────

pub struct MfxTranspose;

impl MidiEffect for MfxTranspose {
    fn name(&self) -> &'static str {
        "Transpose"
    }
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
        let semitones = ctx.get("semitones") as i32;
        let octave = ctx.get("octave") as i32;
        let shift = semitones + octave * 12;
        events
            .into_iter()
            .map(|mut e| {
                e.pitch = (e.pitch as i32 + shift).clamp(0, 127) as u8;
                e
            })
            .collect()
    }
    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxTranspose)
    }
}

// ── Velocity ─────────────────────────────────────────────────────────

pub struct MfxVelocity;

impl MidiEffect for MfxVelocity {
    fn name(&self) -> &'static str {
        "Velocity"
    }
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
        let amount = ctx.get("amount");
        let curve = ctx.get("curve");
        let min_vel = ctx.get("min_vel") / 127.0;
        let max_vel = ctx.get("max_vel") / 127.0;
        events
            .into_iter()
            .map(|mut e| {
                let curved = if curve > 0.5 {
                    e.velocity.powf(1.0 / (curve * 2.0).max(0.01))
                } else {
                    e.velocity.powf((1.0 - curve) * 2.0)
                };
                e.velocity = (curved + amount).clamp(min_vel, max_vel);
                e
            })
            .collect()
    }
    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxVelocity)
    }
}

// ── Chord ─────────────────────────────────────────────────────────────

pub struct MfxChord;

impl MidiEffect for MfxChord {
    fn name(&self) -> &'static str {
        "Chord"
    }
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
        // chord_type: 0=maj,1=min,2=dom7,3=min7,4=sus4,5=dim
        let chord_type = ctx.get("type") as i32;
        let voicing = ctx.get("voicing") as i32;
        let intervals: &[i32] = match chord_type {
            0 => &[4i32, 7],
            1 => &[3i32, 7],
            2 => &[4i32, 7, 10],
            3 => &[3i32, 7, 10],
            4 => &[5i32, 7],
            5 => &[3i32, 6],
            _ => &[4i32, 7],
        };
        let mut out = Vec::with_capacity(events.len() * (1 + intervals.len()));
        for e in &events {
            out.push(e.clone()); // keep root
            for (idx, &interval) in intervals.iter().enumerate() {
                let octave_shift = match voicing {
                    0 => 0,
                    1 => {
                        if idx % 2 == 1 {
                            12
                        } else {
                            0
                        }
                    }
                    2 => (idx as i32) * 12,
                    _ => 0,
                };
                let new_pitch = (e.pitch as i32 + interval + octave_shift).clamp(0, 127) as u8;
                out.push(MidiEvent {
                    pitch: new_pitch,
                    velocity: e.velocity,
                    original_pitch: e.original_pitch,
                });
            }
        }
        out
    }
    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxChord)
    }
}

// ── Arpeggiator ───────────────────────────────────────────────────────

/// Arpeggiator state.  One instance lives per track (not per note).
pub struct MfxArpeggiator {
    /// Current step index into the note pool.
    pub step: usize,
    /// Beat position when the last step fired (-999 = not yet fired).
    pub last_beat: f64,
}

impl MfxArpeggiator {
    pub fn new() -> Self {
        Self {
            step: 0,
            last_beat: -999.0,
        }
    }
}

impl MidiEffect for MfxArpeggiator {
    fn name(&self) -> &'static str {
        "Arpeggiator"
    }

    /// For the Arpeggiator, `events` is the set of *currently held* notes.
    /// Returns at most ONE event per call — the next arp step to play —
    /// or an empty vec if it is not yet time to fire or no notes are held.
    /// The engine must call this every sample with the held-note set.
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
        if events.is_empty() {
            self.step = 0;
            self.last_beat = -999.0;
            return Vec::new();
        }

        let rate_beats = ctx.get("rate").max(0.0625) as f64;
        let octaves = ctx.get("octaves").max(1.0) as i32;
        let pattern = ctx.get("pattern") as i32;
        let vel = ctx.get("vel").clamp(0.0, 1.0); // optional velocity override (0 = use source)

        // Build note pool (sorted ascending pitches × octaves)
        let mut pool: Vec<MidiEvent> = Vec::new();
        let mut pitches: Vec<u8> = events.iter().map(|e| e.pitch).collect();
        pitches.sort_unstable();
        pitches.dedup();
        let base_vel = events.first().map(|e| e.velocity).unwrap_or(0.8);
        let final_vel = if vel > 0.0 { vel } else { base_vel };

        for oct in 0..octaves {
            for &p in &pitches {
                let shifted = (p as i32 + oct * 12).clamp(0, 127) as u8;
                pool.push(MidiEvent {
                    pitch: shifted,
                    velocity: final_vel,
                    original_pitch: p,
                });
            }
        }

        // Apply pattern ordering
        match pattern {
            1 => pool.reverse(),
            2 => {
                let mut down = pool.clone();
                down.reverse();
                if down.len() > 1 {
                    pool.extend_from_slice(&down[1..down.len() - 1]);
                }
            }
            3 => {
                // Deterministic shuffle keyed by beat position
                let seed = (ctx.pos_beats * 1000.0) as u64;
                for i in (1..pool.len()).rev() {
                    let j = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407)
                        .wrapping_shr(33) as usize
                        % (i + 1);
                    pool.swap(i, j);
                }
            }
            _ => {} // 0 = up (default, already sorted)
        }

        // Check if a new step should fire
        let fire = if self.last_beat < 0.0 {
            true // first-ever fire
        } else {
            let steps_now = (ctx.pos_beats / rate_beats).floor() as usize;
            let steps_last = (self.last_beat / rate_beats).floor() as usize;
            steps_now > steps_last
        };

        if fire {
            let idx = self.step % pool.len();
            let event = pool[idx].clone();
            self.step = (self.step + 1) % pool.len().max(1);
            self.last_beat = ctx.pos_beats;
            vec![event]
        } else {
            Vec::new()
        }
    }

    fn reset(&mut self) {
        self.step = 0;
        self.last_beat = -999.0;
    }

    /// Arpeggiator manages its own voice lifetime — skip still_active checks.
    fn manages_voices(&self) -> bool {
        true
    }

    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxArpeggiator::new())
    }
}

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
    /// The original (pre-MIDI-effect) MIDI pitch used for note-off matching.
    pub original_pitch: u8,
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
    /// Noise highpass filter state (SuperSaw noise_hp knob).
    pub noise_hp_ic1: f64,
    pub noise_hp_ic2: f64,
    /// Second noise HP filter state (for synths with two oscillators, e.g. Analog osc2).
    pub noise_hp_ic1b: f64,
    pub noise_hp_ic2b: f64,
}

impl Default for VoiceState {
    fn default() -> Self {
        // Live-playing voices get random-ish phases to prevent unison phase lock.
        // Use time-based seed (only for live playback; render uses with_seed for determinism).
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
    /// Returns `true` for time-based effects (reverb, delay, chorus) that produce
    /// output even after the input goes silent (i.e. they have a "tail").
    /// The audio engine uses this to avoid skipping the effect chain when input is zero.
    fn has_tail(&self) -> bool {
        false
    }
    /// Called by the audio engine to provide the current BPM so beat-synced effects
    /// (e.g. delay) can compute their timing.  Default implementation is a no-op.
    fn set_bpm(&mut self, _bpm: f64) {}
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
    let x = x.clamp(-700.0, 700.0);
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
    let frac = 1.0 + xf * (std::f64::consts::LN_2 + xf * (0.2402265 + xf * 0.0554961));
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

// ── Per-sample parameter smoother ──────────────────────────────────
//
// One-pole exponential smoother that prevents discontinuities when
// automation (or manual knob tweaks) change a parameter value between
// samples.  Without this, filter coefficients (and other DSP state
// derived from parameters) jump instantaneously, producing audible
// clicks and pops.
//
// ─── SMOOTHED PARAM ───────────────────────────────────────────────
// RULE: Every continuous parameter in every effect that can be
// automated MUST use a SmoothedParam.  This prevents the audible
// clicks and pops that occur when a parameter value changes
// instantaneously between samples (coefficient discontinuity).
//
// When adding a NEW effect:
//   1. Add a `SmoothedParam` field for each continuous knob
//      (e.g. sm_cutoff, sm_gain, sm_mix, sm_output …).
//   2. Initialise them in the constructor with SmoothedParam::new().
//   3. In process(), call  sm_xxx.tick(param_val(…))  per‐sample.
//   4. In fresh(), return a default‐constructed instance so the
//      smoothers reset.
//
// Default smoothing time: ~5 ms — fast enough to track even quick
// automation ramps, slow enough to eliminate any click.

/// One-pole parameter smoother.
#[derive(Clone, Copy, Debug)]
pub struct SmoothedParam {
    /// Current (smoothed) value output each sample.
    current: f64,
    /// Target value set from the param list each sample.
    target: f64,
    /// One-pole coefficient: `current += coeff * (target - current)`.
    /// Computed from sample rate and smoothing time in `new()`.
    coeff: f64,
}

impl SmoothedParam {
    /// Create a new smoother starting at `initial` with a ~5 ms ramp.
    /// `sr` is the sample rate.
    #[inline]
    pub fn new(initial: f64, sr: f64) -> Self {
        Self {
            current: initial,
            target: initial,
            coeff: Self::coeff_for_ms(5.0, sr),
        }
    }

    /// Compute the one-pole coefficient for a given smoothing time in ms.
    #[inline]
    fn coeff_for_ms(ms: f64, sr: f64) -> f64 {
        if ms <= 0.0 || sr <= 0.0 {
            return 1.0; // instant
        }
        let samples = ms * 0.001 * sr;
        // 1 - exp(-1/n) ≈ fraction moved per sample toward target
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
}

/// Convert dB to linear gain.  Used by output gain knobs on effects and synths.
/// Range: -60 dB → ~0.001, 0 dB → 1.0, +24 dB → ~15.85
/// Uses fast_pow2 (same math as db_to_lin) instead of 10^(x/20).
#[inline(always)]
pub fn db_to_linear(db: f64) -> f64 {
    if db <= -60.0 {
        0.0
    } else {
        db_to_lin(db)
    }
}

/// Read "output_db" param and apply as linear gain to a stereo pair.
#[inline(always)]
pub fn apply_output_gain(l: f64, r: f64, params: &[(String, f32)]) -> (f64, f64) {
    let db = param_val(params, "output_db", 0.0) as f64;
    if db.abs() < 0.001 {
        return (l, r); // 0 dB → unity, skip multiplication
    }
    let g = db_to_linear(db);
    (l * g, r * g)
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
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let gain = db_to_lin(param_val(params, "gain", 0.0) as f64);

        // ── Oscillators with morphing ──
        let osc1_inc = voice.freq / sample_rate;
        let mut osc1 = osc_morph(osc1_shape, st.phase0, osc1_inc, &mut st.noise_seed);
        // Default HP filter on noise component to remove sub-bass crackle (~80 Hz)
        if osc1_shape >= 3.0 {
            let noise_frac = (osc1_shape - 3.0).min(1.0);
            let (_lp, _bp, hp) = svf_tick(
                osc1,
                80.0,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1,
                &mut st.noise_hp_ic2,
            );
            osc1 = osc1 * (1.0 - noise_frac) + hp * noise_frac;
        }

        let detune = fast_pow2((osc2_semi + osc2_fine / 100.0) / 12.0);
        let osc2_freq = voice.freq * detune;
        let osc2_inc = osc2_freq / sample_rate;
        let mut osc2 = osc_morph(osc2_shape, st.phase1, osc2_inc, &mut st.noise_seed);
        // Default HP filter on noise component to remove sub-bass crackle (~80 Hz)
        if osc2_shape >= 3.0 {
            let noise_frac = (osc2_shape - 3.0).min(1.0);
            let (_lp, _bp, hp) = svf_tick(
                osc2,
                80.0,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1b,
                &mut st.noise_hp_ic2b,
            );
            osc2 = osc2 * (1.0 - noise_frac) + hp * noise_frac;
        }

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
    std::f64::consts::FRAC_1_SQRT_2, // center: cos(π/4)
    1.0,                             // voice 1 (pan -1.0): cos(0)
    0.0,                             // voice 2 (pan +1.0): cos(π/2)
    0.891006524188368,               // voice 3 (pan -0.6): cos(0.2*π/2)
    0.45399049973954675,             // voice 4 (pan +0.6): cos(0.8*π/2)
    0.7933533402912352,              // voice 5 (pan -0.3): cos(0.35*π/2)
    0.6087614290087207,              // voice 6 (pan +0.3): cos(0.65*π/2)
];
const SUPERSAW_PAN_R: [f64; 7] = [
    std::f64::consts::FRAC_1_SQRT_2, // center: sin(π/4)
    0.0,                             // voice 1 (pan -1.0): sin(0)
    1.0,                             // voice 2 (pan +1.0): sin(π/2)
    0.45399049973954675,             // voice 3 (pan -0.6): sin(0.2*π/2)
    0.891006524188368,               // voice 4 (pan +0.6): sin(0.8*π/2)
    0.6087614290087207,              // voice 5 (pan -0.3): sin(0.35*π/2)
    0.7933533402912352,              // voice 6 (pan +0.3): sin(0.65*π/2)
];
/// Center gain for width=0 (mono): both L and R are 1/√2
const PAN_CENTER: f64 = std::f64::consts::FRAC_1_SQRT_2;

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
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
    ParamDesc {
        id: "noise_hp",
        name: "Noise HP",
        default: 0.15,
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
        let gain = db_to_lin(param_val(params, "gain", 0.0) as f64);
        let noise_gain = param_val(params, "noise_gain", 0.0) as f64;
        let noise_hp_norm = param_val(params, "noise_hp", 0.15) as f64;
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

        // ── White noise with highpass filter ──
        // The noise HP knob (0..1) maps to 20Hz..8000Hz cutoff.
        // Default 0.15 ≈ 120Hz — removes low-frequency crackle while keeping brightness.
        let noise = if noise_gain > 0.001 {
            let mut s = st.noise_seed;
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            st.noise_seed = s;
            let raw_noise =
                (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
            // Apply highpass to noise to remove low-frequency crackle
            let noise_hp_hz = 20.0 * fast_pow2(noise_hp_norm * 8.64); // 20Hz..8000Hz
            let (_, _, hp_noise) = svf_tick(
                raw_noise,
                noise_hp_hz,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1,
                &mut st.noise_hp_ic2,
            );
            hp_noise
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
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let gain = db_to_lin(param_val(params, "gain", 0.0) as f64);

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
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let gain = db_to_lin(param_val(params, "gain", 0.0) as f64);
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
            let raw =
                (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
            // Default HP filter on noise to remove sub-bass crackle (~80 Hz)
            let (_lp, _bp, hp) = svf_tick(
                raw,
                80.0,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1,
                &mut st.noise_hp_ic2,
            );
            hp
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
    sm_cutoff: SmoothedParam,
    sm_resonance: SmoothedParam,
    sm_output: SmoothedParam,
}
impl FxLpFilter {
    pub fn new() -> Self {
        // Use a default SR of 44100 for initial smoothing; the first process()
        // call will provide the real SR, and the smoother converges within 1 ms anyway.
        Self {
            ic1_l: 0.0,
            ic2_l: 0.0,
            ic1_r: 0.0,
            ic2_r: 0.0,
            sm_cutoff: SmoothedParam::new(1.0, 44100.0),
            sm_resonance: SmoothedParam::new(0.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
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
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let c = self.sm_cutoff.tick(param_val(params, "cutoff", 1.0) as f64);
        let r = self
            .sm_resonance
            .tick(param_val(params, "resonance", 0.0) as f64);
        let hz = (20.0 * fast_pow2(c * 9.965784284662087)).min(sr * 0.49);
        let (lp_l, _, _) = svf_tick(left, hz, r, sr, &mut self.ic1_l, &mut self.ic2_l);
        let (lp_r, _, _) = svf_tick(right, hz, r, sr, &mut self.ic1_r, &mut self.ic2_r);
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (lp_l, lp_r)
        } else {
            let g = db_to_linear(out_db);
            (lp_l * g, lp_r * g)
        }
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
    sm_cutoff: SmoothedParam,
    sm_resonance: SmoothedParam,
    sm_output: SmoothedParam,
}
impl FxHpFilter {
    pub fn new() -> Self {
        Self {
            ic1_l: 0.0,
            ic2_l: 0.0,
            ic1_r: 0.0,
            ic2_r: 0.0,
            sm_cutoff: SmoothedParam::new(0.0, 44100.0),
            sm_resonance: SmoothedParam::new(0.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
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
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let c = self.sm_cutoff.tick(param_val(params, "cutoff", 0.0) as f64);
        let r = self
            .sm_resonance
            .tick(param_val(params, "resonance", 0.0) as f64);
        let hz = (20.0 * fast_pow2(c * 9.965784284662087)).min(sr * 0.49);
        let (_, _, hp_l) = svf_tick(left, hz, r, sr, &mut self.ic1_l, &mut self.ic2_l);
        let (_, _, hp_r) = svf_tick(right, hz, r, sr, &mut self.ic1_r, &mut self.ic2_r);
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (hp_l, hp_r)
        } else {
            let g = db_to_linear(out_db);
            (hp_l * g, hp_r * g)
        }
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxHpFilter::new())
    }
}

// ── Delay (Stereo beat-synced) ────────────────────────────────────

/// Beat-division labels and their beat counts (at 1 beat = quarter note).
const DELAY_DIVISIONS: &[(&str, f64)] = &[
    ("1/1", 4.0),
    ("1/2", 2.0),
    ("1/2T", 4.0 / 3.0),
    ("1/4", 1.0),
    ("1/4T", 2.0 / 3.0),
    ("1/8", 0.5),
    ("1/8T", 1.0 / 3.0),
    ("1/16", 0.25),
    ("1/16T", 0.5 / 3.0),
    ("1/32", 0.125),
];

const DELAY_DIVISION_LABELS: &[&str] = &[
    "1/1", "1/2", "1/2T", "1/4", "1/4T", "1/8", "1/8T", "1/16", "1/16T", "1/32",
];

pub struct FxDelay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos_l: usize,
    write_pos_r: usize,
    sm_time_l: SmoothedParam,
    sm_time_r: SmoothedParam,
    sm_feedback: SmoothedParam,
    sm_mix: SmoothedParam,
    sm_output: SmoothedParam,
    bpm: f64,
}
impl FxDelay {
    pub fn new(sr: u32) -> Self {
        let len = (sr as usize) * 4; // 4 seconds max (covers 1/1 at low BPM)
        Self {
            buf_l: vec![0.0; len],
            buf_r: vec![0.0; len],
            write_pos_l: 0,
            write_pos_r: 0,
            sm_time_l: SmoothedParam::new(0.25, sr as f64),
            sm_time_r: SmoothedParam::new(0.25, sr as f64),
            sm_feedback: SmoothedParam::new(0.3, sr as f64),
            sm_mix: SmoothedParam::new(0.3, sr as f64),
            sm_output: SmoothedParam::new(0.0, sr as f64),
            bpm: 120.0,
        }
    }

    /// Convert a beat-division index to seconds given current BPM.
    fn division_to_seconds(div_idx: usize, bpm: f64) -> f64 {
        let beats = if div_idx < DELAY_DIVISIONS.len() {
            DELAY_DIVISIONS[div_idx].1
        } else {
            1.0
        };
        let bps = bpm.max(20.0) / 60.0; // beats per second
        beats / bps
    }
}

static DELAY_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "time_l",
        name: "Time L",
        default: 5.0, // index 5 = 1/8
        min: 0.0,
        max: 9.0,
        options: Some(DELAY_DIVISION_LABELS),
    },
    ParamDesc {
        id: "time_r",
        name: "Time R",
        default: 3.0, // index 3 = 1/4
        min: 0.0,
        max: 9.0,
        options: Some(DELAY_DIVISION_LABELS),
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
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
    fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm;
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let div_l = param_val(params, "time_l", 5.0).round() as usize;
        let div_r = param_val(params, "time_r", 3.0).round() as usize;
        let time_l_sec = FxDelay::division_to_seconds(div_l, self.bpm);
        let time_r_sec = FxDelay::division_to_seconds(div_r, self.bpm);
        let time_l = self.sm_time_l.tick(time_l_sec);
        let time_r = self.sm_time_r.tick(time_r_sec);
        let feedback = self
            .sm_feedback
            .tick(param_val(params, "feedback", 0.3) as f64);
        let mix = self.sm_mix.tick(param_val(params, "mix", 0.3) as f64);
        let len = self.buf_l.len();
        if len == 0 {
            return (left, right);
        }
        // Left channel delay with fractional interpolation
        let ds_l = (time_l * sr).max(1.0).min((len - 1) as f64);
        let rp_l = self.write_pos_l as f64 + len as f64 - ds_l;
        let i0_l = rp_l as usize % len;
        let i1_l = (i0_l + 1) % len;
        let frac_l = rp_l - rp_l.floor();
        let del_l = self.buf_l[i0_l] as f64 * (1.0 - frac_l) + self.buf_l[i1_l] as f64 * frac_l;
        self.buf_l[self.write_pos_l] = (left + del_l * feedback) as f32;
        self.write_pos_l = (self.write_pos_l + 1) % len;

        // Right channel delay with fractional interpolation
        let ds_r = (time_r * sr).max(1.0).min((len - 1) as f64);
        let rp_r = self.write_pos_r as f64 + len as f64 - ds_r;
        let i0_r = rp_r as usize % len;
        let i1_r = (i0_r + 1) % len;
        let frac_r = rp_r - rp_r.floor();
        let del_r = self.buf_r[i0_r] as f64 * (1.0 - frac_r) + self.buf_r[i1_r] as f64 * frac_r;
        self.buf_r[self.write_pos_r] = (right + del_r * feedback) as f32;
        self.write_pos_r = (self.write_pos_r + 1) % len;

        let out_l = left * (1.0 - mix) + del_l * mix;
        let out_r = right * (1.0 - mix) + del_r * mix;
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (out_l, out_r)
        } else {
            let g = db_to_linear(out_db);
            (out_l * g, out_r * g)
        }
    }
    fn reset(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos_l = 0;
        self.write_pos_r = 0;
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxDelay::new((self.buf_l.len() / 4) as u32))
    }
    fn has_tail(&self) -> bool {
        true
    }
}

// ── Reverb (Dragonfly-inspired two-stage hall reverb) ───────────────
//
// Stage 1 — Early reflections: an 8-tap multi-delay network that simulates
//   the initial room reflections.  The tap times are prime-number multiples of
//   a size-scaled unit so they never alias.
//
// Stage 2 — Late tail: 8 parallel feedback comb filters (4 per channel, with
//   decorrelated lengths for L vs R) followed by 4 Schroeder allpass sections.
//   Damping is a one-pole low-pass inside every comb loop.
//   A per-sample modulation LFO slightly varies the comb lengths to diffuse
//   the resonances and add the classic "shimmer" typical of plate/hall units.
//
// Parameters:
//   size      — room scale (0–1 → small..large; affects both early+late delays)
//   predelay  — ms of silence before anything starts (0–100 ms)
//   early     — level of the early reflection stage (0–1)
//   late      — level of the late reverb tail (0–1)
//   decay     — RT60 / tail length (0–1)
//   diffuse   — allpass feedback (0–1) controls density of the tail
//   damping   — high-frequency damping inside comb loops (0–1)
//   width     — stereo spread of the output (0–1)
//   mix       — dry/wet balance (0–1)

pub struct FxReverb {
    sr: f64,

    // ── Predelay line ────────────────────────────────────────────────
    pre_buf: Vec<f32>,
    pre_head: usize,

    // ── Early reflections (8 taps, stereo) ───────────────────────────
    early_buf_l: Vec<f32>,
    early_buf_r: Vec<f32>,
    early_head: usize,

    // ── Late reverb: 8 comb filters (4L + 4R) ───────────────────────
    comb_buf_l: [Vec<f32>; 4],
    comb_buf_r: [Vec<f32>; 4],
    comb_head_l: [usize; 4],
    comb_head_r: [usize; 4],
    comb_filt_l: [f64; 4],
    comb_filt_r: [f64; 4],

    // ── Late reverb: 4 allpass sections (L/R) ────────────────────────
    ap_buf_l: [Vec<f32>; 4],
    ap_buf_r: [Vec<f32>; 4],
    ap_head_l: [usize; 4],
    ap_head_r: [usize; 4],

    // ── Modulation LFOs ──────────────────────────────────────────────
    lfo_phase: f64,
    lfo_wander_phase: f64,

    // ── Filters ──────────────────────────────────────────────────────
    // High cut (low-pass on wet output)
    hc_state_l: f64,
    hc_state_r: f64,
    // Low cut (high-pass on wet output)
    lc_state_l: f64,
    lc_state_r: f64,
    // Crossover shelving states for frequency-dependent decay
    hx_state_l: [f64; 4],
    hx_state_r: [f64; 4],
    lx_state_l: [f64; 4],
    lx_state_r: [f64; 4],

    // ── Smoothed automation params ───────────────────────────────────
    sm_mix: SmoothedParam,
    sm_decay: SmoothedParam,
    sm_output: SmoothedParam,
}

impl FxReverb {
    const EARLY_TAP_PRIMES: [f64; 8] = [2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 17.0, 19.0];

    // Comb delays in ms — two decorrelated sets for L/R stereo imaging
    const COMB_MS_L: [f64; 4] = [29.13, 34.07, 38.93, 43.11];
    const COMB_MS_R: [f64; 4] = [30.61, 35.29, 40.37, 44.71];

    // Allpass delays in ms
    const AP_MS_L: [f64; 4] = [5.02, 1.68, 4.01, 1.24];
    const AP_MS_R: [f64; 4] = [5.31, 1.83, 3.78, 1.41];

    pub fn new(sr: u32) -> Self {
        let sr = sr as f64;
        let max_ms = |ms: f64| -> usize { ((sr * ms / 1000.0) as usize + 4).max(8) };

        // Predelay: up to 100 ms
        let pre_len = max_ms(110.0);
        // Early: max tap = prime[7] * unit at max size
        let early_len = max_ms(30.0 * 19.0); // ~570 ms max

        // Comb buffers: size can scale up to 4x, plus LFO wander headroom up to 40ms
        let comb_l: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; max_ms(Self::COMB_MS_L[i] * 5.0 + 50.0)]);
        let comb_r: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; max_ms(Self::COMB_MS_R[i] * 5.0 + 50.0)]);
        let ap_l: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; max_ms(Self::AP_MS_L[i] * 5.0 + 2.0)]);
        let ap_r: [Vec<f32>; 4] =
            std::array::from_fn(|i| vec![0.0; max_ms(Self::AP_MS_R[i] * 5.0 + 2.0)]);

        Self {
            sr,
            pre_buf: vec![0.0; pre_len],
            pre_head: 0,
            early_buf_l: vec![0.0; early_len],
            early_buf_r: vec![0.0; early_len],
            early_head: 0,
            comb_buf_l: comb_l,
            comb_buf_r: comb_r,
            comb_head_l: [0; 4],
            comb_head_r: [0; 4],
            comb_filt_l: [0.0; 4],
            comb_filt_r: [0.0; 4],
            ap_buf_l: ap_l,
            ap_buf_r: ap_r,
            ap_head_l: [0; 4],
            ap_head_r: [0; 4],
            lfo_phase: 0.0,
            lfo_wander_phase: 0.0,
            hc_state_l: 0.0,
            hc_state_r: 0.0,
            lc_state_l: 0.0,
            lc_state_r: 0.0,
            hx_state_l: [0.0; 4],
            hx_state_r: [0.0; 4],
            lx_state_l: [0.0; 4],
            lx_state_r: [0.0; 4],
            sm_mix: SmoothedParam::new(50.0, sr),
            sm_decay: SmoothedParam::new(1.6, sr),
            sm_output: SmoothedParam::new(0.0, sr),
        }
    }

    #[inline]
    fn read_interp(buf: &[f32], head: usize, offset_samples: f64) -> f64 {
        let len = buf.len();
        let rp = (head as f64 + len as f64 - offset_samples).rem_euclid(len as f64);
        let i0 = rp as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = rp - rp.floor();
        buf[i0] as f64 * (1.0 - frac) + buf[i1] as f64 * frac
    }

    /// One-pole low-pass coefficient from cutoff frequency
    #[inline]
    fn lp_coeff(freq: f64, sr: f64) -> f64 {
        let w = (std::f64::consts::TAU * freq / sr).min(0.99);
        w / (1.0 + w)
    }

    /// One-pole high-pass: returns (output, new_state)
    #[inline]
    fn hp_tick(input: f64, state: f64, freq: f64, sr: f64) -> (f64, f64) {
        let a = Self::lp_coeff(freq, sr);
        let new_state = state + a * (input - state);
        (input - new_state, new_state)
    }

    fn process_early(&mut self, input: f64, size: f64) -> (f64, f64) {
        let len = self.early_buf_l.len();
        if len == 0 {
            return (input, input);
        }
        self.early_buf_l[self.early_head] = input as f32;
        self.early_buf_r[(self.early_head + 1) % len] = input as f32;
        self.early_head = (self.early_head + 1) % len;

        // size 0..60 maps to unit delay 4..30 ms
        let unit_ms = 4.0 + (size / 60.0).clamp(0.0, 1.0) * 26.0;
        let unit_samples = unit_ms * self.sr / 1000.0;

        let mut out_l = 0.0_f64;
        let mut out_r = 0.0_f64;
        for (i, &prime) in Self::EARLY_TAP_PRIMES.iter().enumerate() {
            let tap = prime * unit_samples;
            if tap >= len as f64 {
                continue;
            }
            let gain = 1.0 / (i as f64 + 2.0);
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            out_l += sign * gain * Self::read_interp(&self.early_buf_l, self.early_head, tap);
            out_r += sign * gain * Self::read_interp(&self.early_buf_r, self.early_head, tap + 0.5);
        }
        let n = Self::EARLY_TAP_PRIMES.len() as f64;
        (out_l / n, out_r / n)
    }

    #[allow(clippy::too_many_arguments)]
    fn process_late(
        &mut self,
        input: f64,
        size: f64,
        decay: f64,
        diffuse: f64,
        high_xover: f64,
        high_mult: f64,
        low_xover: f64,
        low_mult: f64,
        lfo_mod: f64,
        wander_samples: f64,
    ) -> (f64, f64) {
        // RT60 in seconds: decay param is already in seconds (0.1 .. 10.0)
        let rt60 = decay.clamp(0.1, 10.0);

        // Size factor: size 0..60 m → delay scale 0.5x .. 3.0x
        let size_factor = 0.5 + (size / 60.0).clamp(0.0, 1.0) * 2.5;

        let mut sum_l = 0.0_f64;
        let mut sum_r = 0.0_f64;

        // ── 4 comb filters for L ──
        for i in 0..4 {
            let comb_ms = Self::COMB_MS_L[i] * size_factor;
            let comb_s = comb_ms / 1000.0;
            // Base feedback from RT60
            let fb_base = (10.0_f64).powf(-3.0 * comb_s / rt60).clamp(0.0, 0.9995);

            // LFO modulation: spin + wander create subtle delay time variation
            let lfo_offset = lfo_mod * (1.0 + wander_samples * 0.5) * (i as f64 * 1.3 + 0.1).sin();
            let delay_samples = (comb_ms * self.sr / 1000.0 + lfo_offset)
                .clamp(1.0, (self.comb_buf_l[i].len() - 2) as f64);
            let len = self.comb_buf_l[i].len();
            if len == 0 {
                continue;
            }
            let delayed =
                Self::read_interp(&self.comb_buf_l[i], self.comb_head_l[i], delay_samples);

            // Frequency-dependent decay via crossover shelving
            // High frequency: apply high_mult to feedback above high_xover
            let hx_a = Self::lp_coeff(high_xover, self.sr);
            self.hx_state_l[i] += hx_a * (delayed - self.hx_state_l[i]);
            let low_band = self.hx_state_l[i];
            let high_band = delayed - low_band;

            // Low frequency: apply low_mult to feedback below low_xover
            let lx_a = Self::lp_coeff(low_xover, self.sr);
            self.lx_state_l[i] += lx_a * (low_band - self.lx_state_l[i]);
            let very_low = self.lx_state_l[i];
            let mid_band = low_band - very_low;

            // Recombine with multiplied feedback per band
            let fb_low = fb_base * low_mult.clamp(0.2, 2.5);
            let fb_high = fb_base * high_mult.clamp(0.2, 2.5);
            let fb_mid = fb_base;
            let filtered = very_low * fb_low + mid_band * fb_mid + high_band * fb_high;

            // Standard damping one-pole
            let damping = 1.0 - high_mult.clamp(0.2, 2.5).min(1.0) * 0.3;
            self.comb_filt_l[i] =
                filtered * (1.0 - damping * 0.4) + self.comb_filt_l[i] * (damping * 0.4);

            let new_val = input + self.comb_filt_l[i];
            self.comb_buf_l[i][self.comb_head_l[i]] = new_val as f32;
            self.comb_head_l[i] = (self.comb_head_l[i] + 1) % len;
            sum_l += delayed;
        }

        // ── 4 comb filters for R ──
        for i in 0..4 {
            let comb_ms = Self::COMB_MS_R[i] * size_factor;
            let comb_s = comb_ms / 1000.0;
            let fb_base = (10.0_f64).powf(-3.0 * comb_s / rt60).clamp(0.0, 0.9995);

            let lfo_offset =
                lfo_mod * (1.0 + wander_samples * 0.5) * ((i as f64 + 0.5) * 1.7 + 0.2).sin();
            let delay_samples = (comb_ms * self.sr / 1000.0 + lfo_offset)
                .clamp(1.0, (self.comb_buf_r[i].len() - 2) as f64);
            let len = self.comb_buf_r[i].len();
            if len == 0 {
                continue;
            }
            let delayed =
                Self::read_interp(&self.comb_buf_r[i], self.comb_head_r[i], delay_samples);

            let hx_a = Self::lp_coeff(high_xover, self.sr);
            self.hx_state_r[i] += hx_a * (delayed - self.hx_state_r[i]);
            let low_band = self.hx_state_r[i];
            let high_band = delayed - low_band;

            let lx_a = Self::lp_coeff(low_xover, self.sr);
            self.lx_state_r[i] += lx_a * (low_band - self.lx_state_r[i]);
            let very_low = self.lx_state_r[i];
            let mid_band = low_band - very_low;

            let fb_low = fb_base * low_mult.clamp(0.2, 2.5);
            let fb_high = fb_base * high_mult.clamp(0.2, 2.5);
            let fb_mid = fb_base;
            let filtered = very_low * fb_low + mid_band * fb_mid + high_band * fb_high;

            let damping = 1.0 - high_mult.clamp(0.2, 2.5).min(1.0) * 0.3;
            self.comb_filt_r[i] =
                filtered * (1.0 - damping * 0.4) + self.comb_filt_r[i] * (damping * 0.4);

            let new_val = input + self.comb_filt_r[i];
            self.comb_buf_r[i][self.comb_head_r[i]] = new_val as f32;
            self.comb_head_r[i] = (self.comb_head_r[i] + 1) % len;
            sum_r += delayed;
        }

        sum_l *= 0.25;
        sum_r *= 0.25;

        // ── 4 allpass sections (L and R independently) ──
        let ap_fb = 0.3 + (diffuse / 100.0).clamp(0.0, 1.0) * 0.4;
        for i in 0..4 {
            // Left
            let len_l = self.ap_buf_l[i].len();
            if len_l > 0 {
                let h_l = self.ap_head_l[i];
                let delayed_l = self.ap_buf_l[i][h_l] as f64;
                let new_l = sum_l + delayed_l * ap_fb;
                self.ap_buf_l[i][h_l] = new_l as f32;
                sum_l = delayed_l - new_l * ap_fb;
                self.ap_head_l[i] = (h_l + 1) % len_l;
            }
            // Right
            let len_r = self.ap_buf_r[i].len();
            if len_r > 0 {
                let h_r = self.ap_head_r[i];
                let delayed_r = self.ap_buf_r[i][h_r] as f64;
                let new_r = sum_r + delayed_r * ap_fb;
                self.ap_buf_r[i][h_r] = new_r as f32;
                sum_r = delayed_r - new_r * ap_fb;
                self.ap_head_r[i] = (h_r + 1) % len_r;
            }
        }

        (sum_l, sum_r)
    }
}

// ── Dragonfly Hall Reverb parameter set ──────────────────────────────
// Matches the full Dragonfly Hall Reverb parameter surface:
// Size, Width, Predelay, Decay, Diffuse, Modulation, Spin, Wander,
// HighCut, HighXover, HighMult, LowCut, LowXover, LowMult,
// Dry, Early, EarlySend, Late
static REVERB_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "mix",
        name: "Mix",
        default: 70.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "dry",
        name: "Dry",
        default: 80.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "early",
        name: "Early",
        default: 25.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "early_send",
        name: "Early Send",
        default: 30.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "late",
        name: "Late",
        default: 40.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "size",
        name: "Size",
        default: 24.0,
        min: 8.0,
        max: 60.0,
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
        default: 14.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "decay",
        name: "Decay",
        default: 3.0,
        min: 0.1,
        max: 10.0,
        options: None,
    },
    ParamDesc {
        id: "diffuse",
        name: "Diffuse",
        default: 80.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "modulation",
        name: "Modulation",
        default: 10.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "spin",
        name: "Spin",
        default: 0.40,
        min: 0.0,
        max: 5.0,
        options: None,
    },
    ParamDesc {
        id: "wander",
        name: "Wander",
        default: 12.0,
        min: 0.0,
        max: 40.0,
        options: None,
    },
    ParamDesc {
        id: "high_cut",
        name: "High Cut",
        default: 16000.0,
        min: 1000.0,
        max: 16000.0,
        options: None,
    },
    ParamDesc {
        id: "high_xover",
        name: "High Xover",
        default: 5600.0,
        min: 1000.0,
        max: 16000.0,
        options: None,
    },
    ParamDesc {
        id: "high_mult",
        name: "High Mult",
        default: 0.5,
        min: 0.2,
        max: 2.5,
        options: None,
    },
    ParamDesc {
        id: "low_cut",
        name: "Low Cut",
        default: 0.0,
        min: 0.0,
        max: 200.0,
        options: None,
    },
    ParamDesc {
        id: "low_xover",
        name: "Low Xover",
        default: 500.0,
        min: 50.0,
        max: 1000.0,
        options: None,
    },
    ParamDesc {
        id: "low_mult",
        name: "Low Mult",
        default: 1.0,
        min: 0.5,
        max: 2.5,
        options: None,
    },
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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

        // ── Read all Dragonfly-style parameters ──
        let mix_pct = self.sm_mix.tick(param_val(params, "mix", 50.0) as f64); // 0..100 %
        let dry_pct = param_val(params, "dry", 80.0) as f64; // 0..100 %
        let early_pct = param_val(params, "early", 10.0) as f64; // 0..100 %
        let early_send = param_val(params, "early_send", 20.0) as f64; // 0..100 %
        let late_pct = param_val(params, "late", 20.0) as f64; // 0..100 %
        let size = param_val(params, "size", 24.0) as f64; // 8..60 m
        let width = param_val(params, "width", 100.0) as f64; // 0..100 %
        let predelay = param_val(params, "predelay", 14.0) as f64; // 0..100 ms
        let decay = self.sm_decay.tick(param_val(params, "decay", 1.6) as f64); // 0.1..10 s
        let diffuse = param_val(params, "diffuse", 80.0) as f64; // 0..100 %
        let modulation = param_val(params, "modulation", 10.0) as f64; // 0..100 %
        let spin = param_val(params, "spin", 0.40) as f64; // 0..5 Hz
        let wander = param_val(params, "wander", 12.0) as f64; // 0..40 ms
        let high_cut = param_val(params, "high_cut", 16000.0) as f64; // 1000..16000 Hz
        let high_xover = param_val(params, "high_xover", 5600.0) as f64; // 1000..16000 Hz
        let high_mult = param_val(params, "high_mult", 0.5) as f64; // 0.2..2.5
        let low_cut = param_val(params, "low_cut", 0.0) as f64; // 0..200 Hz
        let low_xover = param_val(params, "low_xover", 500.0) as f64; // 50..1000 Hz
        let low_mult = param_val(params, "low_mult", 1.0) as f64; // 0.5..2.5

        // Convert percentages to linear gains
        let _dry_gain = dry_pct / 100.0; // kept for internal use; mix knob controls wet/dry
        let early_gain = early_pct / 100.0;
        let early_send_gain = early_send / 100.0;
        let late_gain = late_pct / 100.0;
        let width_factor = width / 100.0;
        let mod_depth = modulation / 100.0;

        let mono_in = (left + right) * 0.5;

        // ── Predelay ──
        let pre_len = self.pre_buf.len();
        let pre_delayed = if pre_len > 1 {
            self.pre_buf[self.pre_head] = mono_in as f32;
            self.pre_head = (self.pre_head + 1) % pre_len;
            let pre_samples = (predelay * sr / 1000.0 + 1.0).clamp(1.0, (pre_len - 1) as f64);
            let rp =
                (self.pre_head as f64 + pre_len as f64 - pre_samples).rem_euclid(pre_len as f64);
            let i0 = rp as usize % pre_len;
            let i1 = (i0 + 1) % pre_len;
            let frac = rp - rp.floor();
            self.pre_buf[i0] as f64 * (1.0 - frac) + self.pre_buf[i1] as f64 * frac
        } else {
            mono_in
        };

        // ── Modulation LFOs ──
        // Spin controls LFO rate; Wander controls a secondary slower LFO depth
        self.lfo_phase += spin / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo_mod = fast_sin_phase(self.lfo_phase) * mod_depth;

        // Wander: secondary slow LFO that adds extra delay time variation
        self.lfo_wander_phase += (spin * 0.23) / sr; // slower than main spin
        if self.lfo_wander_phase >= 1.0 {
            self.lfo_wander_phase -= 1.0;
        }
        let wander_samples =
            fast_sin_phase(self.lfo_wander_phase) * (wander * sr / 1000.0) * mod_depth;

        // ── Early reflections ──
        let (early_l, early_r) = if early_gain > 0.001 || early_send_gain > 0.001 {
            self.process_early(pre_delayed, size)
        } else {
            (0.0, 0.0)
        };

        // ── Late tail ──
        // Feed: predelayed signal + early reflections scaled by early_send
        let late_in = pre_delayed + (early_l + early_r) * 0.5 * early_send_gain;
        let (late_l, late_r) = if late_gain > 0.001 {
            self.process_late(
                late_in,
                size,
                decay,
                diffuse,
                high_xover,
                high_mult,
                low_xover,
                low_mult,
                lfo_mod,
                wander_samples,
            )
        } else {
            (0.0, 0.0)
        };

        // ── Mix early + late ──
        let mut wet_l = early_l * early_gain + late_l * late_gain;
        let mut wet_r = early_r * early_gain + late_r * late_gain;

        // ── High cut filter (low-pass on wet output) ──
        if high_cut < 15900.0 {
            let hc_a = Self::lp_coeff(high_cut, sr);
            self.hc_state_l += hc_a * (wet_l - self.hc_state_l);
            self.hc_state_r += hc_a * (wet_r - self.hc_state_r);
            wet_l = self.hc_state_l;
            wet_r = self.hc_state_r;
        }

        // ── Low cut filter (high-pass on wet output) ──
        if low_cut > 1.0 {
            let (out_l, ns_l) = Self::hp_tick(wet_l, self.lc_state_l, low_cut, sr);
            let (out_r, ns_r) = Self::hp_tick(wet_r, self.lc_state_r, low_cut, sr);
            self.lc_state_l = ns_l;
            self.lc_state_r = ns_r;
            wet_l = out_l;
            wet_r = out_r;
        }

        // ── Width (mid/side) ──
        let mid = (wet_l + wet_r) * 0.5;
        let side = (wet_l - wet_r) * 0.5;
        let w_l = mid + side * width_factor;
        let w_r = mid - side * width_factor;

        // ── Mix knob: 0% = full dry, 100% = full wet ──
        // The dry/early/late knobs control internal reverb component balance.
        // The mix knob is the master wet/dry blend that users actually reach for.
        let mix_amt = mix_pct / 100.0;
        let dry_amt = 1.0 - mix_amt;
        let out_l = left * dry_amt + w_l * mix_amt;
        let out_r = right * dry_amt + w_r * mix_amt;

        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        let out_gain = db_to_lin(out_db);
        (out_l * out_gain, out_r * out_gain)
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
        for i in 0..4 {
            for b in &mut self.comb_buf_l[i] {
                *b = 0.0;
            }
            for b in &mut self.comb_buf_r[i] {
                *b = 0.0;
            }
            self.comb_filt_l[i] = 0.0;
            self.comb_filt_r[i] = 0.0;
            for b in &mut self.ap_buf_l[i] {
                *b = 0.0;
            }
            for b in &mut self.ap_buf_r[i] {
                *b = 0.0;
            }
            self.hx_state_l[i] = 0.0;
            self.hx_state_r[i] = 0.0;
            self.lx_state_l[i] = 0.0;
            self.lx_state_r[i] = 0.0;
        }
        self.lfo_phase = 0.0;
        self.lfo_wander_phase = 0.0;
        self.hc_state_l = 0.0;
        self.hc_state_r = 0.0;
        self.lc_state_l = 0.0;
        self.lc_state_r = 0.0;
    }

    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxReverb::new(self.sr as u32))
    }
    fn has_tail(&self) -> bool {
        true
    }
}

// ── Chorus ──────────────────────────────────────────────────────────

pub struct FxChorus {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    phase: f64,
    sm_rate: SmoothedParam,
    sm_depth: SmoothedParam,
    sm_mix: SmoothedParam,
    sm_output: SmoothedParam,
}
impl FxChorus {
    pub fn new(sr: u32) -> Self {
        Self {
            buf_l: vec![0.0; sr as usize],
            buf_r: vec![0.0; sr as usize],
            write_pos: 0,
            phase: 0.0,
            sm_rate: SmoothedParam::new(0.5, sr as f64),
            sm_depth: SmoothedParam::new(0.005, sr as f64),
            sm_mix: SmoothedParam::new(0.5, sr as f64),
            sm_output: SmoothedParam::new(0.0, sr as f64),
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
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let rate = self.sm_rate.tick(param_val(params, "rate", 0.5) as f64);
        let depth = self.sm_depth.tick(param_val(params, "depth", 0.005) as f64);
        let mix = self.sm_mix.tick(param_val(params, "mix", 0.5) as f64);
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
        let out_l = left * (1.0 - mix) + del_l * mix;
        let out_r = right * (1.0 - mix) + del_r * mix;
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (out_l, out_r)
        } else {
            let g = db_to_linear(out_db);
            (out_l * g, out_r * g)
        }
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
    fn has_tail(&self) -> bool {
        true
    }
}

// ── Distortion ──────────────────────────────────────────────────────

pub struct FxDistortion {
    sm_drive: SmoothedParam,
    sm_mix: SmoothedParam,
    sm_output: SmoothedParam,
}

impl FxDistortion {
    pub fn new() -> Self {
        Self {
            sm_drive: SmoothedParam::new(0.5, 44100.0),
            sm_mix: SmoothedParam::new(1.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
        }
    }
}

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
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let drive = self.sm_drive.tick(param_val(params, "drive", 0.5) as f64);
        let dtype = param_val(params, "type", 0.0) as usize;
        let mix = self.sm_mix.tick(param_val(params, "mix", 1.0) as f64);
        if drive < 0.001 {
            return (left, right);
        }
        let dist_l = distort_sample(left, drive, dtype);
        let dist_r = distort_sample(right, drive, dtype);
        let out_l = left * (1.0 - mix) + dist_l * mix;
        let out_r = right * (1.0 - mix) + dist_r * mix;
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (out_l, out_r)
        } else {
            let g = db_to_linear(out_db);
            (out_l * g, out_r * g)
        }
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxDistortion::new())
    }
}

// ── Compressor ──────────────────────────────────────────────────────
//
// Proper downward compressor modelled after the LSP algorithm:
//   • Exponential attack/release envelope follower in log (dB) domain
//   • Peak hold before release begins
//   • Soft-knee in log domain using quadratic Hermite interpolation
//   • Separate attack/release thresholds (release can trail lower)
//   • Makeup gain in dB
//   • Sidechain key input

pub struct FxCompressor {
    /// Current envelope level in dB (log domain follower)
    env_db: f64,
    /// Peak hold value in dB
    peak_db: f64,
    /// Hold counter (samples remaining at peak)
    hold_counter: u32,
    /// Last computed gain reduction (dB, ≤ 0), stored for GR meter display
    last_gr_db: f32,
    sm_threshold: SmoothedParam,
    sm_ratio: SmoothedParam,
    sm_knee: SmoothedParam,
    sm_makeup: SmoothedParam,
    sm_output: SmoothedParam,
}

impl FxCompressor {
    pub fn new() -> Self {
        Self {
            env_db: -120.0,
            peak_db: -120.0,
            hold_counter: 0,
            last_gr_db: 0.0,
            sm_threshold: SmoothedParam::new(-18.0, 44100.0),
            sm_ratio: SmoothedParam::new(4.0, 44100.0),
            sm_knee: SmoothedParam::new(6.0, 44100.0),
            sm_makeup: SmoothedParam::new(0.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
        }
    }

    /// Compute downward compression gain reduction in dB for a given input level.
    /// Uses a soft knee interpolated around the threshold.
    ///
    /// - `in_db`: input level in dB
    /// - `thresh_db`: threshold in dBFS
    /// - `ratio`: compression ratio (>1.0, e.g. 4.0 = 4:1)
    /// - `knee_db`: knee width in dB (0 = hard knee)
    ///
    /// Returns the gain adjustment in dB (≤ 0.0 for compression).
    fn compute_gr_db(in_db: f64, thresh_db: f64, ratio: f64, knee_db: f64) -> f64 {
        let slope = 1.0 - 1.0 / ratio; // 0.75 for 4:1
        let half_knee = knee_db * 0.5;
        let over = in_db - thresh_db;
        if over <= -half_knee {
            // Below knee: no reduction
            0.0
        } else if over >= half_knee {
            // Above knee: full ratio
            -slope * over
        } else {
            // In the knee: quadratic interpolation
            let x = over + half_knee; // 0..knee_db
            let t = x / knee_db; // 0..1
            -slope * knee_db * t * t * 0.5
        }
    }
}

static COMPRESSOR_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "threshold",
        name: "Threshold",
        default: -18.0,
        min: -60.0,
        max: 0.0,
        options: None,
    },
    ParamDesc {
        id: "ratio",
        name: "Ratio",
        default: 4.0,
        min: 1.0,
        max: 20.0,
        options: None,
    },
    ParamDesc {
        id: "knee",
        name: "Knee",
        default: 6.0,
        min: 0.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "attack",
        name: "Attack",
        default: 2.0,
        min: 0.1,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "release",
        name: "Release",
        default: 50.0,
        min: 1.0,
        max: 800.0,
        options: None,
    },
    ParamDesc {
        id: "hold",
        name: "Hold",
        default: 0.0,
        min: 0.0,
        max: 500.0,
        options: None,
    },
    ParamDesc {
        id: "makeup",
        name: "Makeup",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        // — Parameters (smoothed to prevent clicks during automation) —
        let thresh_db = self
            .sm_threshold
            .tick(param_val(params, "threshold", -18.0) as f64);
        let ratio = self.sm_ratio.tick(param_val(params, "ratio", 4.0) as f64);
        let knee_db = self.sm_knee.tick(param_val(params, "knee", 6.0) as f64);
        // Attack/release in milliseconds → per-sample coefficients (1-pole IIR)
        let attack_ms = (param_val(params, "attack", 5.0) as f64).max(0.1);
        let release_ms = (param_val(params, "release", 100.0) as f64).max(1.0);
        let hold_ms = param_val(params, "hold", 0.0) as f64;
        let makeup_db = self.sm_makeup.tick(param_val(params, "makeup", 0.0) as f64);

        // LSP-style: tau = 1 - exp(ln(1 - 1/sqrt(2)) / (ms_to_samples))
        // Simplified: standard one-pole: coeff = exp(-1 / (ms * sr / 1000))
        let attack_coeff = fast_exp(-1.0 / (attack_ms * sr / 1000.0));
        let release_coeff = fast_exp(-1.0 / (release_ms * sr / 1000.0));
        let hold_samples = (hold_ms * sr / 1000.0) as u32;

        // — Key signal: take the louder of L/R —
        let key = key_l.abs().max(key_r.abs());
        // Convert to dB (floor at -120 dB)
        let key_db = if key > 1e-10 {
            20.0 * fast_log10(key)
        } else {
            -120.0
        };

        // — Envelope follower (log domain, attack/release with hold) —
        if key_db > self.env_db {
            // Attack: signal rising
            self.env_db = attack_coeff * self.env_db + (1.0 - attack_coeff) * key_db;
            if self.env_db >= self.peak_db {
                self.peak_db = self.env_db;
                self.hold_counter = hold_samples;
            }
        } else {
            // Release: signal falling
            if self.hold_counter > 0 {
                self.hold_counter -= 1;
                // During hold, keep envelope at peak
                self.env_db = self.peak_db;
            } else {
                self.env_db = release_coeff * self.env_db + (1.0 - release_coeff) * key_db;
                self.peak_db = self.env_db;
            }
        }

        // — Compute gain reduction —
        let gr_db = Self::compute_gr_db(self.env_db, thresh_db, ratio, knee_db);
        self.last_gr_db = gr_db as f32;

        // — Apply gain (GR + makeup) —
        let total_db = gr_db + makeup_db;
        let lin = db_to_lin(total_db);
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        let (ol, or) = (left * lin, right * lin);
        if out_db.abs() < 0.001 {
            (ol, or)
        } else {
            let g = db_to_linear(out_db);
            (ol * g, or * g)
        }
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxCompressor::new())
    }
    fn gain_reduction_db(&self) -> f32 {
        self.last_gr_db
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
    sm_gain: SmoothedParam,
    sm_ceiling: SmoothedParam,
    sm_output: SmoothedParam,
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
            sm_gain: SmoothedParam::new(0.0, 44100.0),
            sm_ceiling: SmoothedParam::new(0.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
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
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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

        let gain_db = self.sm_gain.tick(param_val(params, "gain_db", 0.0) as f64);
        let ceiling_db = self
            .sm_ceiling
            .tick(param_val(params, "ceiling_db", 0.0) as f64);
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
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (ol, or_)
        } else {
            let g = db_to_linear(out_db);
            (ol * g, or_ * g)
        }
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
    sm_lo_gain: SmoothedParam,
    sm_mid_gain: SmoothedParam,
    sm_hi_gain: SmoothedParam,
    sm_lo_freq: SmoothedParam,
    sm_hi_freq: SmoothedParam,
    sm_output: SmoothedParam,
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
            sm_lo_gain: SmoothedParam::new(0.0, 44100.0),
            sm_mid_gain: SmoothedParam::new(0.0, 44100.0),
            sm_hi_gain: SmoothedParam::new(0.0, 44100.0),
            sm_lo_freq: SmoothedParam::new(200.0, 44100.0),
            sm_hi_freq: SmoothedParam::new(4000.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
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
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
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
        let lo_g = self
            .sm_lo_gain
            .tick(param_val(params, "lo_gain", 0.0) as f64);
        let mid_g = self
            .sm_mid_gain
            .tick(param_val(params, "mid_gain", 0.0) as f64);
        let hi_g = self
            .sm_hi_gain
            .tick(param_val(params, "hi_gain", 0.0) as f64);
        let lo_f = self
            .sm_lo_freq
            .tick(param_val(params, "lo_freq", 200.0) as f64)
            .clamp(20.0, sr * 0.49);
        let hi_f = self
            .sm_hi_freq
            .tick(param_val(params, "hi_freq", 4000.0) as f64)
            .clamp(20.0, sr * 0.49);
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
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (out_l, out_r)
        } else {
            let g = db_to_linear(out_db);
            (out_l * g, out_r * g)
        }
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxEq::new())
    }
}

// ── Gain ─────────────────────────────────────────────────────────────

pub struct FxGain {
    sm_gain: SmoothedParam,
}

impl FxGain {
    pub fn new() -> Self {
        Self {
            sm_gain: SmoothedParam::new(0.0, 44100.0),
        }
    }
}

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
        let db = self.sm_gain.tick(param_val(params, "gain_db", 0.0) as f64);
        let g = db_to_lin(db);
        (left * g, right * g)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxGain::new())
    }
}

// ── Utility (Gain + Pan + Phase Invert + DC Offset) ─────────────────

pub struct FxUtility {
    sm_gain: SmoothedParam,
    sm_pan: SmoothedParam,
    sm_dc: SmoothedParam,
}

impl FxUtility {
    pub fn new() -> Self {
        Self {
            sm_gain: SmoothedParam::new(0.0, 44100.0),
            sm_pan: SmoothedParam::new(0.0, 44100.0),
            sm_dc: SmoothedParam::new(0.0, 44100.0),
        }
    }
}

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
        let db = self.sm_gain.tick(param_val(params, "gain_db", 0.0) as f64);
        let pan = self.sm_pan.tick(param_val(params, "pan", 0.0) as f64);
        let phase_inv = param_val(params, "phase", 0.0);
        let dc = self.sm_dc.tick(param_val(params, "dc_offset", 0.0) as f64);
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
        Box::new(FxUtility::new())
    }
}

// ── Autoduck (tempo-synced volume ducking) ──────────────────────────

/// Free-running ducking effect.
/// Repeats a duck envelope every `period` ms.
/// Envelope shape: attack → hold → release → idle (duck off).
/// RULE: All continuous params use SmoothedParam to prevent clicks/pops during automation.
pub struct FxAutoduck {
    phase: f64, // 0..1 normalised position within the period
    sm_duck: SmoothedParam,
    sm_attack: SmoothedParam,
    sm_hold: SmoothedParam,
    sm_release: SmoothedParam,
    sm_period: SmoothedParam,
    sm_shift: SmoothedParam,
    sm_curve: SmoothedParam,
    sm_output: SmoothedParam,
}

impl FxAutoduck {
    pub fn new() -> Self {
        let sr = 44100.0;
        Self {
            phase: 0.0,
            sm_duck: SmoothedParam::new(-12.0, sr),
            sm_attack: SmoothedParam::new(5.0, sr),
            sm_hold: SmoothedParam::new(50.0, sr),
            sm_release: SmoothedParam::new(100.0, sr),
            sm_period: SmoothedParam::new(500.0, sr),
            sm_shift: SmoothedParam::new(0.0, sr),
            sm_curve: SmoothedParam::new(50.0, sr),
            sm_output: SmoothedParam::new(0.0, sr),
        }
    }
}

static AUTODUCK_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "duck_db",
        name: "Duck",
        default: -12.0,
        min: -60.0,
        max: 0.0,
        options: None,
    },
    ParamDesc {
        id: "attack",
        name: "Attack",
        default: 5.0,
        min: 0.1,
        max: 200.0,
        options: None,
    },
    ParamDesc {
        id: "hold",
        name: "Hold",
        default: 50.0,
        min: 0.0,
        max: 500.0,
        options: None,
    },
    ParamDesc {
        id: "release",
        name: "Release",
        default: 100.0,
        min: 1.0,
        max: 1000.0,
        options: None,
    },
    ParamDesc {
        id: "period",
        name: "Period",
        default: 500.0,
        min: 50.0,
        max: 4000.0,
        options: None,
    },
    ParamDesc {
        id: "shift",
        name: "Shift",
        default: 0.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "curve",
        name: "Curve",
        default: 50.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "output_db",
        name: "Output",
        default: 0.0,
        min: -60.0,
        max: 24.0,
        options: None,
    },
];

impl EffectModule for FxAutoduck {
    fn name(&self) -> &'static str {
        "Autoduck"
    }
    fn params(&self) -> &'static [ParamDesc] {
        AUTODUCK_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let duck_db = self
            .sm_duck
            .tick(param_val(params, "duck_db", -12.0) as f64);
        let attack_ms = self
            .sm_attack
            .tick(param_val(params, "attack", 5.0) as f64)
            .max(0.1);
        let hold_ms = self
            .sm_hold
            .tick(param_val(params, "hold", 50.0) as f64)
            .max(0.0);
        let release_ms = self
            .sm_release
            .tick(param_val(params, "release", 100.0) as f64)
            .max(1.0);
        let period_ms = self
            .sm_period
            .tick(param_val(params, "period", 500.0) as f64)
            .max(1.0);
        let shift_pct = self.sm_shift.tick(param_val(params, "shift", 0.0) as f64);
        let curve_pct = self.sm_curve.tick(param_val(params, "curve", 50.0) as f64);
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);

        // Advance phase (0..1 within the period)
        let phase_inc = 1000.0 / (period_ms * sr);
        self.phase += phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // Apply shift (wraps around)
        let shifted = (self.phase + shift_pct / 100.0) % 1.0;

        // Convert envelope segment boundaries to normalised phase values
        let total_env_ms = attack_ms + hold_ms + release_ms;
        let env_fraction = (total_env_ms / period_ms).min(1.0);

        let attack_end = (attack_ms / period_ms).min(1.0);
        let hold_end = ((attack_ms + hold_ms) / period_ms).min(1.0);
        let release_end = env_fraction;

        // Compute duck envelope: 0.0 = no ducking, 1.0 = full ducking
        let raw_env = if shifted < attack_end {
            // Attack: ramp from 0 to 1 (duck going down)
            if attack_end > 1e-9 {
                shifted / attack_end
            } else {
                1.0
            }
        } else if shifted < hold_end {
            // Hold: full ducking
            1.0
        } else if shifted < release_end {
            // Release: ramp from 1 to 0 (duck coming back up)
            let rel_phase = (shifted - hold_end) / (release_end - hold_end).max(1e-9);
            1.0 - rel_phase
        } else {
            // Idle: no ducking
            0.0
        };

        // Apply curve shaping: curve_pct=50 is linear, <50 = exponential, >50 = log
        let curve_norm = curve_pct / 100.0; // 0..1
        let shaped = if (curve_norm - 0.5).abs() < 0.01 {
            raw_env // linear
        } else if curve_norm < 0.5 {
            // Exponential curve (sharper)
            let exp = 1.0 + (0.5 - curve_norm) * 6.0; // 1..4
            raw_env.powf(exp)
        } else {
            // Logarithmic curve (softer)
            let exp = 1.0 / (1.0 + (curve_norm - 0.5) * 6.0); // 1..0.25
            raw_env.powf(exp)
        };

        // Convert duck amount to gain
        let duck_gain = db_to_lin(duck_db * shaped);

        let dl = left * duck_gain;
        let dr = right * duck_gain;

        // Output gain
        if out_db.abs() < 0.001 {
            (dl, dr)
        } else {
            let g = db_to_lin(out_db);
            (dl * g, dr * g)
        }
    }
    fn reset(&mut self) {
        self.phase = 0.0;
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxAutoduck::new())
    }
}

// ═══════════════════════════════════════════════════════════════════
// CStrip2 — Airwindows-style channel strip
// 3-band EQ (Triplet) + ButterComp + Spiral output saturation
// ═══════════════════════════════════════════════════════════════════

/// Per-channel IIR filter state for the 6-pole hi-pass and lo-pass caps.
#[derive(Clone, Default)]
struct CsHpLpState {
    hp: [f64; 6],
    lp: [f64; 6],
}

/// Butter-style compressor state (4-way dual-rail).
#[derive(Clone, Default)]
struct CsCompState {
    avg:   f64,
    nvg:   f64,
    tar_pos: f64,
    tar_neg: f64,
    ctrl_a_pos: f64,
    ctrl_b_pos: f64,
    ctrl_a_neg: f64,
    ctrl_b_neg: f64,
}

pub struct CStrip2 {
    // 6-pole hi-pass / lo-pass filter states (L, R)
    fl: CsHpLpState,
    fr: CsHpLpState,
    // Hi-shelf IIR (dual-rail, both channels)
    iir_hl: f64,
    iir_hr: f64,
    // Compressor state (L, R)
    cl: CsCompState,
    cr: CsCompState,
    // 3-band Triplet EQ
    tri_la: f64, tri_lb: f64, tri_lc: f64,
    tri_ra: f64, tri_rb: f64, tri_rc: f64,
    last_l: f64,  last2_l: f64,
    last_r: f64,  last2_r: f64,
    // Dithering seeds
    fpd_l: u32,
    fpd_r: u32,
    // flip / counter
    flip: bool,
    flip3: i32,
    count: i32,
}

impl CStrip2 {
    pub fn new() -> Self {
        Self {
            fl: CsHpLpState::default(),
            fr: CsHpLpState::default(),
            iir_hl: 0.0,
            iir_hr: 0.0,
            cl: CsCompState::default(),
            cr: CsCompState::default(),
            tri_la: 0.0, tri_lb: 0.0, tri_lc: 0.0,
            tri_ra: 0.0, tri_rb: 0.0, tri_rc: 0.0,
            last_l: 0.0, last2_l: 0.0,
            last_r: 0.0, last2_r: 0.0,
            fpd_l: 1,
            fpd_r: 1,
            flip: false,
            flip3: 0,
            count: 0,
        }
    }

    /// 6-pole RC hi-pass filter — state is the 6-element slice.
    #[inline]
    fn apply_hpcap(hp: &mut [f64; 6], inp: f64, coef: f64) -> f64 {
        hp[0] = (hp[0] * (1.0 - coef)) + (inp * coef);
        hp[1] = (hp[1] * (1.0 - coef)) + (hp[0] * coef);
        hp[2] = (hp[2] * (1.0 - coef)) + (hp[1] * coef);
        hp[3] = (hp[3] * (1.0 - coef)) + (hp[2] * coef);
        hp[4] = (hp[4] * (1.0 - coef)) + (hp[3] * coef);
        hp[5] = (hp[5] * (1.0 - coef)) + (hp[4] * coef);
        inp - hp[5]
    }

    /// 6-pole RC lo-pass filter.
    #[inline]
    fn apply_lpcap(lp: &mut [f64; 6], inp: f64, coef: f64) -> f64 {
        lp[0] = (lp[0] * (1.0 - coef)) + (inp * coef);
        lp[1] = (lp[1] * (1.0 - coef)) + (lp[0] * coef);
        lp[2] = (lp[2] * (1.0 - coef)) + (lp[1] * coef);
        lp[3] = (lp[3] * (1.0 - coef)) + (lp[2] * coef);
        lp[4] = (lp[4] * (1.0 - coef)) + (lp[3] * coef);
        lp[5] = (lp[5] * (1.0 - coef)) + (lp[4] * coef);
        lp[5]
    }

    /// Single-channel ButterComp tick.
    /// Returns the compressed output.
    #[inline]
    fn butter_comp(cs: &mut CsCompState, inp: f64, spd: f64, compress: f64) -> f64 {
        // Running average + variance
        cs.avg = cs.avg * (1.0 - spd) + inp * spd;
        cs.nvg = cs.nvg * (1.0 - spd) + (cs.avg - inp).abs() * spd;

        // Positive rail
        let pos_val = inp.max(0.0);
        cs.tar_pos = cs.tar_pos * (1.0 - spd) + pos_val * spd;
        let pos_gain = if cs.tar_pos > 0.0 {
            let ratio = cs.ctrl_b_pos / cs.tar_pos;
            cs.ctrl_a_pos = cs.ctrl_a_pos * (1.0 - spd) + ratio * spd;
            cs.ctrl_b_pos = cs.ctrl_b_pos * (1.0 - spd) + cs.ctrl_a_pos * spd;
            cs.ctrl_b_pos
        } else {
            1.0
        };

        // Negative rail
        let neg_val = (-inp).max(0.0);
        cs.tar_neg = cs.tar_neg * (1.0 - spd) + neg_val * spd;
        let neg_gain = if cs.tar_neg > 0.0 {
            let ratio = cs.ctrl_b_neg / cs.tar_neg;
            cs.ctrl_a_neg = cs.ctrl_a_neg * (1.0 - spd) + ratio * spd;
            cs.ctrl_b_neg = cs.ctrl_b_neg * (1.0 - spd) + cs.ctrl_a_neg * spd;
            cs.ctrl_b_neg
        } else {
            1.0
        };

        let gain = if inp >= 0.0 { pos_gain } else { neg_gain };
        // Mix dry and compressed
        let gain_clamped = gain.clamp(0.0, 2.0);
        inp * (1.0 - compress) + inp * gain_clamped * compress
    }

    /// Airwindows Spiral saturation (smooth soft-clipper).
    #[inline]
    fn spiral(x: f64) -> f64 {
        if x.abs() > 1.0 {
            x.signum()
        } else {
            x - (x * x * x) / 3.0
        }
    }
}

static CSTRIP2_PARAMS: &[ParamDesc] = &[
    ParamDesc { id: "treble",   name: "Treble",   default: 0.5, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "mid",      name: "Mid",      default: 0.5, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "bass",     name: "Bass",     default: 0.5, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "treb_frq", name: "TrebFreq", default: 0.5, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "bass_frq", name: "BassFreq", default: 0.5, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "lo_cap",   name: "LoCap",    default: 1.0, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "hi_cap",   name: "HiCap",    default: 0.0, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "compress", name: "Compress", default: 0.0, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "comp_spd", name: "CompSpd",  default: 0.0, min: 0.0, max: 1.0, options: None },
    ParamDesc { id: "output",   name: "Trim",     default: 0.5,  min: 0.0, max: 1.0, options: None },
];

impl EffectModule for CStrip2 {
    fn name(&self) -> &'static str { "CStrip2" }
    fn params(&self) -> &'static [ParamDesc] { CSTRIP2_PARAMS }

    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], _sr: f64) -> (f64, f64) {
        let treble   = param_val(params, "treble",   0.5) as f64;
        let mid      = param_val(params, "mid",      0.5) as f64;
        let bass     = param_val(params, "bass",     0.5) as f64;
        let treb_frq = param_val(params, "treb_frq", 0.5) as f64;
        let bass_frq = param_val(params, "bass_frq", 0.5) as f64;
        let lo_cap   = param_val(params, "lo_cap",   1.0) as f64;
        let hi_cap   = param_val(params, "hi_cap",   0.0) as f64;
        let compress = param_val(params, "compress", 0.0) as f64;
        let comp_spd = param_val(params, "comp_spd", 0.0) as f64;
        let output   = param_val(params, "output",   0.5) as f64;

        // ── Hi-pass cap (lo_cap) ─────────────────────────────────────────
        // lo_cap=1.0 → no HP filter; 0.0 → aggressive cut
        let hp_coef = if lo_cap < 1.0 {
            (1.0 - lo_cap).powf(2.0) * 0.4995 + 0.0001
        } else {
            0.0
        };
        let mut l = if hp_coef > 1e-6 {
            Self::apply_hpcap(&mut self.fl.hp, left, hp_coef)
        } else {
            left
        };
        let mut r = if hp_coef > 1e-6 {
            Self::apply_hpcap(&mut self.fr.hp, right, hp_coef)
        } else {
            right
        };

        // ── Lo-pass cap (hi_cap) ─────────────────────────────────────────
        // hi_cap=0.0 → no LP; 1.0 → aggressive cut
        let lp_coef = if hi_cap > 0.0 {
            hi_cap.powf(2.0) * 0.4995 + 0.0001
        } else {
            0.0
        };
        if lp_coef > 1e-6 {
            l = Self::apply_lpcap(&mut self.fl.lp, l, lp_coef);
            r = Self::apply_lpcap(&mut self.fr.lp, r, lp_coef);
        }

        // ── 3-band Triplet EQ ────────────────────────────────────────────
        // Bass: first-order IIR; treble is complementary; mid fills the gap
        // bass_frq 0..1 → LP cutoff coeff 0.001 .. 0.499
        let bass_coef = bass_frq * bass_frq * 0.499 + 0.001;
        let treb_coef = (1.0 - treb_frq) * (1.0 - treb_frq) * 0.499 + 0.001;

        // Low band (LP)
        self.tri_la = self.tri_la * (1.0 - bass_coef) + l * bass_coef;
        self.tri_ra = self.tri_ra * (1.0 - bass_coef) + r * bass_coef;
        // High band (complement of LP at treble freq)
        self.tri_lc = self.tri_lc * (1.0 - treb_coef) + l * treb_coef;
        self.tri_rc = self.tri_rc * (1.0 - treb_coef) + r * treb_coef;
        // Mid = input - low - high residual
        self.tri_lb = l - self.tri_la - (l - self.tri_lc);
        self.tri_rb = r - self.tri_ra - (r - self.tri_rc);

        // EQ gains: 0..1 → -6..+6 dB style (0.5=unity)
        let bass_g   = (bass   * 2.0 - 1.0) * 0.5 + 1.0; // 0.5 .. 1.5
        let mid_g    = (mid    * 2.0 - 1.0) * 0.5 + 1.0;
        let treble_g = (treble * 2.0 - 1.0) * 0.5 + 1.0;

        l = self.tri_la * bass_g + self.tri_lb * mid_g + (l - self.tri_lc) * treble_g + self.tri_lc;
        r = self.tri_ra * bass_g + self.tri_rb * mid_g + (r - self.tri_rc) * treble_g + self.tri_rc;

        // ── ButterComp ───────────────────────────────────────────────────
        if compress > 0.001 {
            // comp_spd 0..1 → attack coefficient ~0.001 .. 0.3
            let spd = comp_spd * comp_spd * 0.299 + 0.001;
            l = Self::butter_comp(&mut self.cl, l, spd, compress);
            r = Self::butter_comp(&mut self.cr, r, spd, compress);
        }

        // ── Output gain + Spiral saturation ─────────────────────────────
        // output 0..1 → -50..+50 dB trim (0.5 = unity)
        let trim_db = (output - 0.5) * 100.0; // -50..+50 dB
        let out_gain = 10.0_f64.powf(trim_db / 20.0);
        l = Self::spiral(l * out_gain);
        r = Self::spiral(r * out_gain);

        (l, r)
    }

    fn fresh(&self) -> Box<dyn EffectModule> { Box::new(CStrip2::new()) }
    fn reset(&mut self) { *self = CStrip2::new(); }
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
        "Distortion" => Some(Box::new(FxDistortion::new())),
        "Compressor" => Some(Box::new(FxCompressor::new())),
        "EQ" => Some(Box::new(FxEq::new())),
        "Gain" => Some(Box::new(FxGain::new())),
        "Utility" => Some(Box::new(FxUtility::new())),
        "Limiter" => Some(Box::new(FxLimiter::new())),
        "Autoduck" => Some(Box::new(FxAutoduck::new())),
        "CStrip2" => Some(Box::new(CStrip2::new())),
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
            | "Autoduck"
            | "CStrip2"
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
        "Autoduck" => AUTODUCK_PARAMS,
        "CStrip2" => CSTRIP2_PARAMS,
        _ => &[],
    }
}
