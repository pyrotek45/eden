// Eden DAW — Sampler instrument

use crate::modules::dsp_primitives::*;
use crate::modules::{EnvStage, InstrumentModule, ModuleExtra, ModuleVoice, ParamDesc};

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

pub struct Sampler;

pub(crate) static SAMPLER_PARAMS: &[ParamDesc] = &[
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
