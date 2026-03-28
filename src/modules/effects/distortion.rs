// Eden DAW — Distortion effect

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

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

pub(crate) static DISTORTION_PARAMS: &[ParamDesc] = &[
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
