// Eden DAW — Chorus effect

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

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

pub(crate) static CHORUS_PARAMS: &[ParamDesc] = &[
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
        let lfo_r = fast_sin_phase((self.phase + 0.25) % 1.0);
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
