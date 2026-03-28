// Eden DAW — Compressor effect

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

pub struct FxCompressor {
    env_db: f64,
    peak_db: f64,
    hold_counter: u32,
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

    fn compute_gr_db(in_db: f64, thresh_db: f64, ratio: f64, knee_db: f64) -> f64 {
        let slope = 1.0 - 1.0 / ratio;
        let half_knee = knee_db * 0.5;
        let over = in_db - thresh_db;
        if over <= -half_knee {
            0.0
        } else if over >= half_knee {
            -slope * over
        } else {
            let x = over + half_knee;
            let t = x / knee_db;
            -slope * knee_db * t * t * 0.5
        }
    }
}

pub(crate) static COMPRESSOR_PARAMS: &[ParamDesc] = &[
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
        let thresh_db = self
            .sm_threshold
            .tick(param_val(params, "threshold", -18.0) as f64);
        let ratio = self.sm_ratio.tick(param_val(params, "ratio", 4.0) as f64);
        let knee_db = self.sm_knee.tick(param_val(params, "knee", 6.0) as f64);
        let attack_ms = (param_val(params, "attack", 5.0) as f64).max(0.1);
        let release_ms = (param_val(params, "release", 100.0) as f64).max(1.0);
        let hold_ms = param_val(params, "hold", 0.0) as f64;
        let makeup_db = self.sm_makeup.tick(param_val(params, "makeup", 0.0) as f64);

        let attack_coeff = fast_exp(-1.0 / (attack_ms * sr / 1000.0));
        let release_coeff = fast_exp(-1.0 / (release_ms * sr / 1000.0));
        let hold_samples = (hold_ms * sr / 1000.0) as u32;

        let key = key_l.abs().max(key_r.abs());
        let key_db = if key > 1e-10 {
            20.0 * fast_log10(key)
        } else {
            -120.0
        };

        if key_db > self.env_db {
            self.env_db = attack_coeff * self.env_db + (1.0 - attack_coeff) * key_db;
            if self.env_db >= self.peak_db {
                self.peak_db = self.env_db;
                self.hold_counter = hold_samples;
            }
        } else if self.hold_counter > 0 {
            self.hold_counter -= 1;
            self.env_db = self.peak_db;
        } else {
            self.env_db = release_coeff * self.env_db + (1.0 - release_coeff) * key_db;
            self.peak_db = self.env_db;
        }

        let gr_db = Self::compute_gr_db(self.env_db, thresh_db, ratio, knee_db);
        self.last_gr_db = gr_db as f32;

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
