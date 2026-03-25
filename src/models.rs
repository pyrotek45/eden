// Eden DAW — Core data models
// These are the project-level types that get serialized / deserialized.

use serde::{Deserialize, Serialize};

// ── Track types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackType {
    Midi,
    Audio,
    Automation,
}

// ── MIDI ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiNote {
    pub pitch: u8,    // 0–127
    pub velocity: u8, // 0–127
    pub start: f64,   // beats
    pub length: f64,  // beats
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiClip {
    pub notes: Vec<MidiNote>,
    pub start_time: f64, // beats on timeline
    pub length: f64,     // beats
    pub name: String,
    pub color: [u8; 4],
}

// ── Audio ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioClip {
    pub source_file: String,
    pub start_time: f64, // beats on timeline
    pub offset: f64,     // where in source file to start (seconds)
    pub length: f64,     // beats
    pub gain: f32,
    pub name: String,
    pub color: [u8; 4],
    /// Fade-in duration in seconds (applied from the start of the clip)
    #[serde(default)]
    pub fade_in: f64,
    /// Fade-out duration in seconds (applied at the end of the clip)
    #[serde(default)]
    pub fade_out: f64,
}

// ── Automation ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub time: f64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationClip {
    pub points: Vec<AutomationPoint>,
    pub start_time: f64,
    pub length: f64,
    pub target_param: String,
    pub name: String,
    pub color: [u8; 4],
}

// ── Clip enum ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Clip {
    Midi(MidiClip),
    Audio(AudioClip),
    Automation(AutomationClip),
}

impl Clip {
    pub fn start_time(&self) -> f64 {
        match self {
            Clip::Midi(c) => c.start_time,
            Clip::Audio(c) => c.start_time,
            Clip::Automation(c) => c.start_time,
        }
    }

    pub fn length(&self) -> f64 {
        match self {
            Clip::Midi(c) => c.length,
            Clip::Audio(c) => c.length,
            Clip::Automation(c) => c.length,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Clip::Midi(c) => &c.name,
            Clip::Audio(c) => &c.name,
            Clip::Automation(c) => &c.name,
        }
    }

    pub fn color(&self) -> [u8; 4] {
        match self {
            Clip::Midi(c) => c.color,
            Clip::Audio(c) => c.color,
            Clip::Automation(c) => c.color,
        }
    }

    pub fn set_start_time(&mut self, time: f64) {
        match self {
            Clip::Midi(c) => c.start_time = time,
            Clip::Audio(c) => c.start_time = time,
            Clip::Automation(c) => c.start_time = time,
        }
    }

    pub fn set_length(&mut self, len: f64) {
        match self {
            Clip::Midi(c) => c.length = len,
            Clip::Audio(c) => c.length = len,
            Clip::Automation(c) => c.length = len,
        }
    }
}

// ── Track ────────────────────────────────────────────────────────────

// ── Rack / Effects chain ─────────────────────────────────────────────

/// A single automatable parameter in a rack slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RackParam {
    pub id: String,   // e.g. "cutoff", "resonance", "gain"
    pub name: String, // display label
    pub value: f32,   // current value (0.0–1.0 normalized)
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl RackParam {
    pub fn new(id: &str, name: &str, default: f32, min: f32, max: f32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value: default,
            min,
            max,
            default,
        }
    }
}

/// One slot in a track's rack (instrument or effect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RackSlot {
    pub slot_id: u32,
    pub plugin_name: String, // "Sine Osc", "Filter", "Delay", etc.
    pub enabled: bool,
    pub params: Vec<RackParam>,
    /// For effects that support sidechain (currently Compressor):
    /// the track ID to use as sidechain input for level detection.
    /// None = use the track's own signal (internal key, normal behaviour).
    #[serde(default)]
    pub sidechain_track_id: Option<u32>,
}

impl RackSlot {
    /// Create a simple sine oscillator slot (built-in).
    pub fn sine_osc(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Sine Osc".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("gain", "Gain", 0.8, 0.0, 1.0),
                RackParam::new("detune", "Detune", 0.0, -100.0, 100.0),
            ],
        }
    }

    /// Create a simple low-pass filter slot.
    pub fn lpfilter(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "LP Filter".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("cutoff", "Cutoff", 1.0, 0.0, 1.0),
                RackParam::new("resonance", "Resonance", 0.0, 0.0, 1.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a delay effect slot.
    pub fn delay(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Delay".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("time_l", "Time L", 5.0, 0.0, 9.0),
                RackParam::new("time_r", "Time R", 3.0, 0.0, 9.0),
                RackParam::new("feedback", "Feedback", 0.3, 0.0, 0.99),
                RackParam::new("mix", "Mix", 0.3, 0.0, 1.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a square oscillator slot (built-in).
    pub fn square_osc(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Square Osc".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("gain", "Gain", 0.8, 0.0, 1.0),
                RackParam::new("detune", "Detune", 0.0, -100.0, 100.0),
            ],
        }
    }

    /// Create a saw oscillator slot (built-in).
    pub fn saw_osc(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Saw Osc".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("gain", "Gain", 0.8, 0.0, 1.0),
                RackParam::new("detune", "Detune", 0.0, -100.0, 100.0),
            ],
        }
    }

    /// Create a triangle oscillator slot (built-in).
    pub fn triangle_osc(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Triangle Osc".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("gain", "Gain", 0.8, 0.0, 1.0),
                RackParam::new("detune", "Detune", 0.0, -100.0, 100.0),
            ],
        }
    }

    /// Create a SubtractiveSynth slot — the main built-in 2-osc synth.
    pub fn subtractive_synth(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Analog".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                // ── Oscillators ──
                RackParam::new("osc1_wave", "Osc1 Shape", 1.0, 0.0, 4.0),
                RackParam::new("osc2_wave", "Osc2 Shape", 1.0, 0.0, 4.0),
                RackParam::new("osc_mix", "Osc Mix", 0.0, 0.0, 1.0),
                RackParam::new("gain", "Gain", 0.0, -60.0, 24.0),
                // ── Oscillator tuning ──
                RackParam::new("osc2_semi", "Semi", 0.0, -24.0, 24.0),
                RackParam::new("osc2_fine", "Fine", 0.0, -100.0, 100.0),
                RackParam::new("filter_type", "Filt Type", 0.0, 0.0, 2.0),
                RackParam::new("filter_cutoff", "Cutoff", 0.8, 0.0, 1.0),
                // ── Filter ──
                RackParam::new("filter_reso", "Reso", 0.0, 0.0, 1.0),
                RackParam::new("filter_env", "Env Amt", 0.0, -1.0, 1.0),
                RackParam::new("filter_a", "F.Atk", 0.01, 0.001, 8.0),
                RackParam::new("filter_d", "F.Dec", 0.2, 0.001, 8.0),
                // ── Filter env cont + Amp ADSR ──
                RackParam::new("filter_s", "F.Sus", 0.4, 0.0, 1.0),
                RackParam::new("filter_r", "F.Rel", 0.3, 0.001, 8.0),
                RackParam::new("amp_a", "A.Atk", 0.01, 0.001, 8.0),
                RackParam::new("amp_d", "A.Dec", 0.1, 0.001, 8.0),
                // ── Amp ADSR cont ──
                RackParam::new("amp_s", "A.Sus", 0.8, 0.0, 1.0),
                RackParam::new("amp_r", "A.Rel", 0.3, 0.001, 8.0),
            ],
        }
    }

    /// Create a SuperSaw instrument slot (JP-8000-style dual 7-osc detuned saw).
    pub fn supersaw(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "HyperSaw".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                // ── Oscillator 1 ──
                RackParam::new("osc1_detune", "O1 Detune", 0.00, 0.0, 0.015),
                RackParam::new("osc1_mix", "O1 Mix", 0.75, 0.0, 1.0),
                RackParam::new("osc1_width", "O1 Width", 0.5, 0.0, 1.0),
                // ── Oscillator 2 ──
                RackParam::new("osc2_detune", "O2 Detune", 0.00, 0.0, 0.015),
                RackParam::new("osc2_mix", "O2 Mix", 0.75, 0.0, 1.0),
                RackParam::new("osc2_width", "O2 Width", 0.5, 0.0, 1.0),
                // ── Oscillator blend + tuning ──
                RackParam::new("osc_blend", "Osc Blend", 0.0, 0.0, 1.0),
                RackParam::new("osc2_semi", "O2 Semi", 0.0, -24.0, 24.0),
                RackParam::new("osc2_fine", "O2 Fine", 0.0, -100.0, 100.0),
                RackParam::new("gain", "Gain", 0.0, -60.0, 24.0),
                RackParam::new("noise_gain", "Noise", 0.0, 0.0, 1.0),
                RackParam::new("noise_hp", "Noise HP", 0.15, 0.0, 1.0),
                // ── Filter ──
                RackParam::new("filter_cutoff", "Cutoff", 0.9, 0.0, 1.0),
                RackParam::new("filter_reso", "Reso", 0.1, 0.0, 1.0),
                RackParam::new("filter_env", "Env Amt", 0.0, -1.0, 1.0),
                RackParam::new("filter_a", "F.Atk", 0.01, 0.001, 8.0),
                RackParam::new("filter_d", "F.Dec", 0.3, 0.001, 8.0),
                // ── Filter env cont + Amp ADSR ──
                RackParam::new("filter_s", "F.Sus", 0.3, 0.0, 1.0),
                RackParam::new("filter_r", "F.Rel", 0.4, 0.001, 8.0),
                RackParam::new("amp_a", "A.Atk", 0.01, 0.001, 8.0),
                RackParam::new("amp_d", "A.Dec", 0.1, 0.001, 8.0),
                // ── Amp ADSR cont ──
                RackParam::new("amp_s", "A.Sus", 0.8, 0.0, 1.0),
                RackParam::new("amp_r", "A.Rel", 0.3, 0.001, 8.0),
            ],
        }
    }

    /// Create a Sampler instrument slot.
    pub fn sampler(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Sampler".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("gate", "Gate Mode", 1.0, 0.0, 1.0), // 0=one-shot, 1=gate
                RackParam::new("root_note", "Root Note", 60.0, 0.0, 127.0),
                RackParam::new("pitch_track", "Pitch Track", 1.0, 0.0, 1.0),
                RackParam::new("start", "Start", 0.0, 0.0, 1.0),
                RackParam::new("end", "End", 1.0, 0.0, 1.0),
                // ── Amp ADSR ──
                RackParam::new("amp_a", "Amp A", 0.005, 0.001, 5.0),
                RackParam::new("amp_d", "Amp D", 0.05, 0.001, 5.0),
                RackParam::new("amp_s", "Amp S", 1.0, 0.0, 1.0),
                RackParam::new("amp_r", "Amp R", 0.1, 0.001, 5.0),
                // ── Master ──
                RackParam::new("gain", "Gain", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a HeavySynth instrument slot (advanced 1-osc + sub + noise + distortion).
    pub fn heavy_synth(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Monolith".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                // ── Oscillator ──
                RackParam::new("osc_shape", "Shape", 1.0, 0.0, 7.0),
                RackParam::new("sub_level", "Sub", 0.0, 0.0, 1.0),
                RackParam::new("noise_mix", "Noise", 0.0, 0.0, 1.0),
                RackParam::new("gain", "Gain", 0.0, -60.0, 24.0),
                // ── Filter ──
                RackParam::new("filter_cutoff", "Cutoff", 0.8, 0.0, 1.0),
                RackParam::new("filter_reso", "Reso", 0.0, 0.0, 1.0),
                RackParam::new("filter_env", "Env Amt", 0.0, -1.0, 1.0),
                RackParam::new("filter_a", "F.Atk", 0.01, 0.001, 8.0),
                RackParam::new("filter_d", "F.Dec", 0.2, 0.001, 8.0),
                RackParam::new("filter_s", "F.Sus", 0.4, 0.0, 1.0),
                RackParam::new("filter_r", "F.Rel", 0.3, 0.001, 8.0),
                // ── Amp ADSR ──
                RackParam::new("amp_a", "A.Atk", 0.01, 0.001, 8.0),
                RackParam::new("amp_d", "A.Dec", 0.1, 0.001, 8.0),
                RackParam::new("amp_s", "A.Sus", 0.8, 0.0, 1.0),
                RackParam::new("amp_r", "A.Rel", 0.3, 0.001, 8.0),
                // ── Distortion ──
                RackParam::new("dist_drive", "Drive", 0.0, 0.0, 1.0),
                RackParam::new("dist_type", "Dist Type", 0.0, 0.0, 3.0),
            ],
        }
    }

    /// Create an HP Filter effect slot.
    pub fn hpfilter(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "HP Filter".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("cutoff", "Cutoff", 0.0, 0.0, 1.0),
                RackParam::new("resonance", "Resonance", 0.0, 0.0, 1.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a Reverb effect slot (Dragonfly Hall Reverb parameters).
    pub fn reverb(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Reverb".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("mix", "Mix", 70.0, 0.0, 100.0),
                RackParam::new("dry", "Dry", 80.0, 0.0, 100.0),
                RackParam::new("early", "Early", 25.0, 0.0, 100.0),
                RackParam::new("early_send", "Early Send", 30.0, 0.0, 100.0),
                RackParam::new("late", "Late", 40.0, 0.0, 100.0),
                RackParam::new("size", "Size", 24.0, 8.0, 60.0),
                RackParam::new("width", "Width", 100.0, 0.0, 100.0),
                RackParam::new("predelay", "Predelay", 14.0, 0.0, 100.0),
                RackParam::new("decay", "Decay", 3.0, 0.1, 10.0),
                RackParam::new("diffuse", "Diffuse", 80.0, 0.0, 100.0),
                RackParam::new("modulation", "Modulation", 10.0, 0.0, 100.0),
                RackParam::new("spin", "Spin", 0.40, 0.0, 5.0),
                RackParam::new("wander", "Wander", 12.0, 0.0, 40.0),
                RackParam::new("high_cut", "High Cut", 16000.0, 1000.0, 16000.0),
                RackParam::new("high_xover", "High Xover", 5600.0, 1000.0, 16000.0),
                RackParam::new("high_mult", "High Mult", 0.5, 0.2, 2.5),
                RackParam::new("low_cut", "Low Cut", 0.0, 0.0, 200.0),
                RackParam::new("low_xover", "Low Xover", 500.0, 50.0, 1000.0),
                RackParam::new("low_mult", "Low Mult", 1.0, 0.5, 2.5),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a Chorus effect slot.
    pub fn chorus(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Chorus".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("rate", "Rate", 0.5, 0.01, 5.0),
                RackParam::new("depth", "Depth", 0.005, 0.0, 0.02),
                RackParam::new("mix", "Mix", 0.5, 0.0, 1.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a Distortion effect slot.
    pub fn distortion(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Distortion".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("drive", "Drive", 0.5, 0.0, 1.0),
                RackParam::new("type", "Type", 0.0, 0.0, 3.0), // 0=soft,1=hard,2=fold,3=crush
                RackParam::new("mix", "Mix", 1.0, 0.0, 1.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a Compressor effect slot.
    pub fn compressor(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Compressor".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("threshold", "Threshold", -18.0, -60.0, 0.0),
                RackParam::new("ratio", "Ratio", 4.0, 1.0, 20.0),
                RackParam::new("knee", "Knee", 6.0, 0.0, 24.0),
                RackParam::new("attack", "Attack", 5.0, 0.1, 200.0),
                RackParam::new("release", "Release", 100.0, 5.0, 2000.0),
                RackParam::new("hold", "Hold", 0.0, 0.0, 500.0),
                RackParam::new("makeup", "Makeup", 0.0, -24.0, 24.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create an EQ effect slot.
    /// Create an EQ effect slot.
    pub fn eq(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "EQ".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("lo_gain", "Lo Gain", 0.0, -12.0, 12.0),
                RackParam::new("mid_gain", "Mid Gain", 0.0, -12.0, 12.0),
                RackParam::new("hi_gain", "Hi Gain", 0.0, -12.0, 12.0),
                RackParam::new("lo_freq", "Lo Freq", 200.0, 20.0, 500.0),
                RackParam::new("hi_freq", "Hi Freq", 4000.0, 1000.0, 16000.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create a Gain effect slot.
    pub fn gain(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Gain".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![RackParam::new("gain_db", "Gain dB", 0.0, -60.0, 24.0)],
        }
    }

    /// Create a Utility effect slot (gain + pan + phase invert + DC offset).
    pub fn utility(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Utility".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("gain_db", "Gain dB", 0.0, -60.0, 24.0),
                RackParam::new("pan", "Pan", 0.0, -1.0, 1.0),
                RackParam::new("phase", "Phase", 0.0, 0.0, 1.0),
                RackParam::new("dc_offset", "DC Offset", 0.0, -1.0, 1.0),
            ],
        }
    }

    /// Create a Limiter effect slot (brick-wall, zero-latency).
    pub fn limiter(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Limiter".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("gain_db", "Input Gain", 0.0, 0.0, 24.0),
                RackParam::new("ceiling_db", "Ceiling", 0.0, -12.0, 0.0),
                RackParam::new("release", "Release", 0.05, 0.001, 1.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create an Autoduck effect slot (tempo-synced volume ducking).
    pub fn autoduck(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Autoduck".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("duck_db", "Duck", -12.0, -60.0, 0.0),
                RackParam::new("attack", "Attack", 5.0, 0.1, 200.0),
                RackParam::new("hold", "Hold", 50.0, 0.0, 500.0),
                RackParam::new("release", "Release", 100.0, 1.0, 1000.0),
                RackParam::new("period", "Period", 500.0, 50.0, 4000.0),
                RackParam::new("shift", "Shift", 0.0, 0.0, 100.0),
                RackParam::new("curve", "Curve", 50.0, 0.0, 100.0),
                RackParam::new("output_db", "Output", 0.0, -60.0, 24.0),
            ],
        }
    }

    /// Create an Arpeggiator MIDI effect slot.
    pub fn arpeggiator(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Arpeggiator".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("rate", "Rate", 0.25, 0.0625, 2.0), // beats per step
                RackParam::new("octaves", "Octaves", 1.0, 1.0, 4.0),
                RackParam::new("pattern", "Pattern", 0.0, 0.0, 3.0), // 0=up,1=down,2=updown,3=random
                RackParam::new("gate_len", "Gate", 0.8, 0.1, 1.0),
            ],
        }
    }

    /// Create a Chord MIDI effect slot.
    pub fn chord(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Chord".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("type", "Type", 0.0, 0.0, 5.0), // 0=maj,1=min,2=7th,3=min7,4=sus4,5=dim
                RackParam::new("voicing", "Voicing", 0.0, 0.0, 2.0), // 0=close,1=open,2=spread
            ],
        }
    }

    /// Create a Transpose MIDI effect slot.
    pub fn transpose(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Transpose".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("semitones", "Semitones", 0.0, -24.0, 24.0),
                RackParam::new("octave", "Octave", 0.0, -3.0, 3.0),
            ],
        }
    }

    /// Create a Velocity MIDI effect slot.
    pub fn velocity(slot_id: u32) -> Self {
        Self {
            slot_id,
            plugin_name: "Velocity".into(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![
                RackParam::new("amount", "Amount", 0.0, -1.0, 1.0),
                RackParam::new("curve", "Curve", 0.5, 0.0, 1.0),
                RackParam::new("min_vel", "Min Vel", 1.0, 0.0, 127.0),
                RackParam::new("max_vel", "Max Vel", 127.0, 0.0, 127.0),
            ],
        }
    }
}

/// Create a RackSlot for a given module name.
pub fn create_rack_slot_for_module(name: &str, slot_id: u32) -> RackSlot {
    match name {
        "Analog" => RackSlot::subtractive_synth(slot_id),
        "HyperSaw" => RackSlot::supersaw(slot_id),
        "Sampler" => RackSlot::sampler(slot_id),
        "Monolith" => RackSlot::heavy_synth(slot_id),
        "Sine Osc" => RackSlot::sine_osc(slot_id),
        "Square Osc" => RackSlot::square_osc(slot_id),
        "Saw Osc" => RackSlot::saw_osc(slot_id),
        "Triangle Osc" => RackSlot::triangle_osc(slot_id),
        "LP Filter" => RackSlot::lpfilter(slot_id),
        "HP Filter" => RackSlot::hpfilter(slot_id),
        "Delay" => RackSlot::delay(slot_id),
        "Reverb" => RackSlot::reverb(slot_id),
        "Chorus" => RackSlot::chorus(slot_id),
        "Distortion" => RackSlot::distortion(slot_id),
        "Compressor" => RackSlot::compressor(slot_id),
        "EQ" => RackSlot::eq(slot_id),
        "Gain" => RackSlot::gain(slot_id),
        "Utility" => RackSlot::utility(slot_id),
        "Limiter" => RackSlot::limiter(slot_id),
        "Autoduck" => RackSlot::autoduck(slot_id),
        "Arpeggiator" => RackSlot::arpeggiator(slot_id),
        "Chord" => RackSlot::chord(slot_id),
        "Transpose" => RackSlot::transpose(slot_id),
        "Velocity" => RackSlot::velocity(slot_id),
        // For unknown modules, create a generic slot
        _ => RackSlot {
            slot_id,
            plugin_name: name.to_string(),
            enabled: true,
            sidechain_track_id: None,
            params: vec![],
        },
    }
}

/// Fully qualified reference to an automatable rack parameter.
/// Used by AutomationClip to know what it's controlling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationTarget {
    pub track_id: u32,
    pub slot_id: u32,
    pub param_id: String,
}

impl AutomationTarget {
    pub fn to_key(&self) -> String {
        format!("{}:{}:{}", self.track_id, self.slot_id, self.param_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: u32,
    pub name: String,
    pub track_type: TrackType,
    pub volume: f32, // 0.0–1.0
    pub pan: f32,    // -1.0–1.0
    pub mute: bool,
    pub solo: bool,
    pub clips: Vec<Clip>,
    pub color: [u8; 4],
    pub height: i32, // pixel height in arrangement
    /// Effects/instrument rack for this track.
    #[serde(default)]
    pub rack: Vec<RackSlot>,
    /// Selected instrument index for MIDI tracks (index into INSTRUMENT_LIST)
    #[serde(default)]
    pub instrument_idx: usize,
    /// Path to sample file for sampler-type instruments.
    #[serde(default)]
    pub sampler_file: Option<String>,
    /// Whether automation on this track is enabled (only for Automation tracks).
    #[serde(default = "default_true")]
    pub automation_enabled: bool,
    /// Channel strip (CStrip2) parameters. Empty = default values (strip is bypassed when
    /// all params are at their defaults, but still applies if any are adjusted).
    /// Stored as (param_id, value) pairs matching CStrip2's CSTRIP2_PARAMS order.
    #[serde(default)]
    pub cstrip2_params: Vec<(String, f32)>,
}

fn default_true() -> bool {
    true
}

/// Available instruments for MIDI tracks
pub fn instrument_list() -> Vec<&'static str> {
    vec![
        "Sine Osc",
        "Square Osc",
        "Saw Osc",
        "Triangle Osc",
        "FM Synth",
        "Sampler",
        "Pluck",
        "Pad",
    ]
}

impl Track {
    pub fn new(id: u32, name: &str, track_type: TrackType) -> Self {
        let color = match track_type {
            TrackType::Midi => [100, 160, 255, 200],
            TrackType::Audio => [100, 220, 130, 200],
            TrackType::Automation => [220, 180, 80, 200],
        };
        let rack = match track_type {
            TrackType::Midi => vec![RackSlot::subtractive_synth(1)],
            TrackType::Audio => vec![],
            TrackType::Automation => vec![],
        };
        Self {
            id,
            name: name.to_string(),
            track_type,
            volume: 0.8,
            pan: 0.0,
            mute: false,
            solo: false,
            clips: Vec::new(),
            color,
            height: 80,
            rack,
            instrument_idx: 0,
            sampler_file: None,
            automation_enabled: true,
            cstrip2_params: Vec::new(),
        }
    }
}

// ── Tempo map ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoChange {
    pub beat: f64,
    pub bpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoMap {
    pub changes: Vec<TempoChange>,
}

impl Default for TempoMap {
    fn default() -> Self {
        Self {
            changes: vec![TempoChange {
                beat: 0.0,
                bpm: 128.0,
            }],
        }
    }
}

impl TempoMap {
    pub fn bpm_at(&self, _beat: f64) -> f64 {
        // For now just return the first tempo
        self.changes.first().map(|t| t.bpm).unwrap_or(120.0)
    }

    pub fn beats_to_seconds(&self, beats: f64) -> f64 {
        let bpm = self.bpm_at(0.0);
        beats * 60.0 / bpm
    }

    pub fn seconds_to_beats(&self, seconds: f64) -> f64 {
        let bpm = self.bpm_at(0.0);
        seconds * bpm / 60.0
    }
}

// ── Transport ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopRegion {
    pub start: f64, // beats
    pub end: f64,   // beats
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transport {
    pub playing: bool,
    pub recording: bool,
    pub position: f64, // current playback position in beats
    pub loop_enabled: bool,
    pub loop_region: LoopRegion,
}

impl Default for Transport {
    fn default() -> Self {
        Self {
            playing: false,
            recording: false,
            position: 0.0,
            loop_enabled: false,
            loop_region: LoopRegion {
                start: 0.0,
                end: 8.0,
            },
        }
    }
}

// ── Project ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub sample_rate: u32,
    pub tracks: Vec<Track>,
    pub tempo_map: TempoMap,
    pub transport: Transport,
    /// Time signature: (numerator, denominator), e.g. (4, 4) for 4/4 time.
    #[serde(default = "default_time_signature")]
    pub time_signature: (u8, u8),
    /// Master output effects chain (applied after mixing all tracks).
    #[serde(default)]
    pub master_rack: Vec<RackSlot>,
}

fn default_time_signature() -> (u8, u8) {
    (4, 4)
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            sample_rate: 44100,
            tracks: Vec::new(),
            tempo_map: TempoMap::default(),
            transport: Transport::default(),
            time_signature: (4, 4),
            master_rack: vec![RackSlot::limiter(1)],
        }
    }
}

impl Project {
    /// Create a demo project with a few tracks for testing.
    pub fn demo() -> Self {
        let mut p = Self {
            name: "Demo Project".into(),
            ..Default::default()
        };

        // MIDI track with a clip
        let mut t1 = Track::new(1, "Synth Lead", TrackType::Midi);
        t1.clips.push(Clip::Midi(MidiClip {
            notes: vec![
                MidiNote {
                    pitch: 60,
                    velocity: 100,
                    start: 0.0,
                    length: 1.0,
                },
                MidiNote {
                    pitch: 64,
                    velocity: 90,
                    start: 1.0,
                    length: 1.0,
                },
                MidiNote {
                    pitch: 67,
                    velocity: 95,
                    start: 2.0,
                    length: 1.0,
                },
                MidiNote {
                    pitch: 72,
                    velocity: 100,
                    start: 3.0,
                    length: 2.0,
                },
            ],
            start_time: 0.0,
            length: 8.0,
            name: "Intro".into(),
            color: [100, 160, 255, 200],
        }));
        p.tracks.push(t1);

        // Second MIDI track
        let mut t2 = Track::new(2, "Bass", TrackType::Midi);
        t2.color = [80, 200, 120, 200];
        t2.clips.push(Clip::Midi(MidiClip {
            notes: vec![
                MidiNote {
                    pitch: 36,
                    velocity: 110,
                    start: 0.0,
                    length: 2.0,
                },
                MidiNote {
                    pitch: 36,
                    velocity: 110,
                    start: 2.0,
                    length: 2.0,
                },
                MidiNote {
                    pitch: 31,
                    velocity: 100,
                    start: 4.0,
                    length: 4.0,
                },
            ],
            start_time: 0.0,
            length: 8.0,
            name: "Bass Line".into(),
            color: [80, 200, 120, 200],
        }));
        p.tracks.push(t2);

        // Audio track (placeholder)
        let mut t3 = Track::new(3, "Drums", TrackType::Audio);
        t3.color = [220, 140, 60, 200];
        t3.clips.push(Clip::Audio(AudioClip {
            source_file: "drums.wav".into(),
            start_time: 0.0,
            offset: 0.0,
            length: 16.0,
            gain: 1.0,
            name: "Drum Loop".into(),
            color: [220, 140, 60, 200],
            fade_in: 0.0,
            fade_out: 0.0,
        }));
        p.tracks.push(t3);

        // Automation track
        let mut t4 = Track::new(4, "Filter Auto", TrackType::Automation);
        t4.clips.push(Clip::Automation(AutomationClip {
            points: vec![
                AutomationPoint {
                    time: 0.0,
                    value: 0.2,
                },
                AutomationPoint {
                    time: 4.0,
                    value: 0.8,
                },
                AutomationPoint {
                    time: 8.0,
                    value: 0.2,
                },
            ],
            start_time: 0.0,
            length: 8.0,
            target_param: "filter_cutoff".into(),
            name: "Filter Sweep".into(),
            color: [220, 180, 80, 200],
        }));
        p.tracks.push(t4);

        p
    }

    pub fn next_track_id(&self) -> u32 {
        self.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1
    }
}

/// Import a standard MIDI file and return a list of (track_name, MidiClip) pairs.
/// Uses the `midly` crate for parsing. Converts ticks to beats using the file's
/// ticks-per-beat (PPQ) timing.
pub fn import_midi_file(path: &str, project_bpm: f64) -> Result<Vec<(String, MidiClip)>, String> {
    let data = std::fs::read(path).map_err(|e| format!("Failed to read MIDI file: {}", e))?;
    let smf = midly::Smf::parse(&data).map_err(|e| format!("Failed to parse MIDI: {}", e))?;

    let ticks_per_beat: f64 = match smf.header.timing {
        midly::Timing::Metrical(ppq) => ppq.as_int() as f64,
        midly::Timing::Timecode(fps, sub) => {
            // Convert to approximate ticks-per-beat using project BPM
            let frames_per_sec = match fps {
                midly::Fps::Fps24 => 24.0,
                midly::Fps::Fps25 => 25.0,
                midly::Fps::Fps29 => 29.97,
                midly::Fps::Fps30 => 30.0,
            };
            frames_per_sec * sub as f64 * 60.0 / project_bpm
        }
    };

    let mut result: Vec<(String, MidiClip)> = Vec::new();

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut notes: Vec<MidiNote> = Vec::new();
        let mut pending: Vec<(u8, u8, f64)> = Vec::new(); // (pitch, velocity, start_tick)
        let mut abs_tick: u64 = 0;
        let mut track_name = format!("MIDI {}", track_idx + 1);

        for event in track {
            abs_tick += event.delta.as_int() as u64;
            match event.kind {
                midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(name)) => {
                    if let Ok(s) = std::str::from_utf8(name) {
                        if !s.trim().is_empty() {
                            track_name = s.trim().to_string();
                        }
                    }
                }
                midly::TrackEventKind::Midi {
                    channel: _,
                    message,
                } => match message {
                    midly::MidiMessage::NoteOn { key, vel } => {
                        if vel.as_int() == 0 {
                            // NoteOn with vel=0 is NoteOff
                            if let Some(pos) =
                                pending.iter().position(|(p, _, _)| *p == key.as_int())
                            {
                                let (pitch, velocity, start_tick) = pending.remove(pos);
                                let start_beats = start_tick / ticks_per_beat;
                                let end_beats = abs_tick as f64 / ticks_per_beat;
                                let length = (end_beats - start_beats).max(0.01);
                                notes.push(MidiNote {
                                    pitch,
                                    velocity,
                                    start: start_beats,
                                    length,
                                });
                            }
                        } else {
                            pending.push((key.as_int(), vel.as_int(), abs_tick as f64));
                        }
                    }
                    midly::MidiMessage::NoteOff { key, vel: _ } => {
                        if let Some(pos) = pending.iter().position(|(p, _, _)| *p == key.as_int()) {
                            let (pitch, velocity, start_tick) = pending.remove(pos);
                            let start_beats = start_tick / ticks_per_beat;
                            let end_beats = abs_tick as f64 / ticks_per_beat;
                            let length = (end_beats - start_beats).max(0.01);
                            notes.push(MidiNote {
                                pitch,
                                velocity,
                                start: start_beats,
                                length,
                            });
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Close any remaining pending notes
        for (pitch, velocity, start_tick) in pending {
            let start_beats = start_tick / ticks_per_beat;
            let end_beats = abs_tick as f64 / ticks_per_beat;
            let length = (end_beats - start_beats).max(0.01);
            notes.push(MidiNote {
                pitch,
                velocity,
                start: start_beats,
                length,
            });
        }

        if notes.is_empty() {
            continue;
        }

        // Determine clip length from the latest note end
        let clip_length = notes
            .iter()
            .map(|n| n.start + n.length)
            .fold(0.0_f64, f64::max)
            .max(1.0);

        result.push((
            track_name,
            MidiClip {
                notes,
                start_time: 0.0,
                length: clip_length,
                name: String::new(), // will be set by caller
                color: [100, 160, 255, 200],
            },
        ));
    }

    if result.is_empty() {
        Err("No MIDI notes found in file".to_string())
    } else {
        Ok(result)
    }
}

/// Export a MidiClip to a standard MIDI file (.mid).
/// Notes are in beats; we use 480 ticks per quarter note (beat = 4 ticks at 1/4 resolution).
/// The BPM is embedded as a tempo meta-event so other DAWs interpret timing correctly.
pub fn export_midi_file(
    clip: &MidiClip,
    path: &str,
    bpm: f64,
    _clip_name: &str,
) -> Result<(), String> {
    use midly::num::{u15, u24, u28, u4, u7};
    use midly::{
        Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    };

    let ppq: u16 = 480; // ticks per quarter note (= per beat in 4/4)

    // Convert beats to ticks
    let beat_to_tick = |b: f64| -> u64 { (b * ppq as f64).round() as u64 };

    // Build a list of (absolute_tick, event_kind) pairs
    let mut events: Vec<(u64, TrackEventKind<'static>)> = Vec::new();

    // Tempo meta event at tick 0: microseconds per quarter note
    let us_per_beat = (60_000_000.0 / bpm).round() as u32;
    let _tempo_bytes: [u8; 3] = [
        ((us_per_beat >> 16) & 0xFF) as u8,
        ((us_per_beat >> 8) & 0xFF) as u8,
        (us_per_beat & 0xFF) as u8,
    ];
    events.push((
        0,
        TrackEventKind::Meta(MetaMessage::Tempo(u24::from(us_per_beat))),
    ));

    // Note on / note off pairs
    for note in &clip.notes {
        let on_tick = beat_to_tick(note.start);
        let off_tick = beat_to_tick(note.start + note.length);

        events.push((
            on_tick,
            TrackEventKind::Midi {
                channel: u4::from(0),
                message: MidiMessage::NoteOn {
                    key: u7::from(note.pitch.min(127)),
                    vel: u7::from(note.velocity.min(127)),
                },
            },
        ));
        events.push((
            off_tick,
            TrackEventKind::Midi {
                channel: u4::from(0),
                message: MidiMessage::NoteOff {
                    key: u7::from(note.pitch.min(127)),
                    vel: u7::from(0),
                },
            },
        ));
    }

    // End of track
    let max_tick = events.iter().map(|(t, _)| *t).max().unwrap_or(0);
    events.push((max_tick, TrackEventKind::Meta(MetaMessage::EndOfTrack)));

    // Sort by absolute tick (stable sort keeps note-off before note-on at same tick)
    events.sort_by_key(|(t, _)| *t);

    // Convert absolute ticks to delta ticks
    let mut track: Vec<TrackEvent<'static>> = Vec::new();
    let mut prev_tick: u64 = 0;
    for (abs_tick, kind) in events {
        let delta = abs_tick.saturating_sub(prev_tick);
        track.push(TrackEvent {
            delta: u28::from(delta as u32),
            kind,
        });
        prev_tick = abs_tick;
    }

    let smf = Smf {
        header: Header::new(Format::SingleTrack, Timing::Metrical(u15::from(ppq))),
        tracks: vec![track],
    };

    smf.save(path)
        .map_err(|e| format!("Failed to save MIDI file: {}", e))
}
