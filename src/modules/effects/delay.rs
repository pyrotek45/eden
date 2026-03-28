// Eden DAW — Delay effect (stereo beat-synced)

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

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
        let len = (sr as usize) * 4;
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

    fn division_to_seconds(div_idx: usize, bpm: f64) -> f64 {
        let beats = if div_idx < DELAY_DIVISIONS.len() {
            DELAY_DIVISIONS[div_idx].1
        } else {
            1.0
        };
        let bps = bpm.max(20.0) / 60.0;
        beats / bps
    }
}

pub(crate) static DELAY_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "time_l",
        name: "Time L",
        default: 5.0,
        min: 0.0,
        max: 9.0,
        options: Some(DELAY_DIVISION_LABELS),
    },
    ParamDesc {
        id: "time_r",
        name: "Time R",
        default: 3.0,
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
        let ds_l = (time_l * sr).max(1.0).min((len - 1) as f64);
        let rp_l = self.write_pos_l as f64 + len as f64 - ds_l;
        let i0_l = rp_l as usize % len;
        let i1_l = (i0_l + 1) % len;
        let frac_l = rp_l - rp_l.floor();
        let del_l = self.buf_l[i0_l] as f64 * (1.0 - frac_l) + self.buf_l[i1_l] as f64 * frac_l;
        self.buf_l[self.write_pos_l] = (left + del_l * feedback) as f32;
        self.write_pos_l = (self.write_pos_l + 1) % len;

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
