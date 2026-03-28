// Eden DAW — Limiter effect (lookahead brickwall)

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

pub struct FxLimiter {
    delay_buf: Vec<f64>,
    gain_buf: Vec<f64>,
    write_pos: usize,
    lookahead: usize,
    smooth_gr: f64,
    sm_gain: SmoothedParam,
    sm_output: SmoothedParam,
    last_sr: f64,
}

impl FxLimiter {
    pub fn new() -> Self {
        Self {
            delay_buf: Vec::new(),
            gain_buf: Vec::new(),
            write_pos: 0,
            lookahead: 0,
            smooth_gr: 1.0,
            sm_gain: SmoothedParam::new(0.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
            last_sr: 44100.0,
        }
    }

    fn ensure_buffers(&mut self, sr: f64) {
        let la = ((sr * 0.005).round() as usize).max(1);
        if la != self.lookahead {
            self.lookahead = la;
            self.delay_buf = vec![0.0; la * 2];
            self.gain_buf = vec![1.0; la];
            self.write_pos = 0;
            self.smooth_gr = 1.0;
        }
        if (sr - self.last_sr).abs() > 1.0 {
            self.last_sr = sr;
            self.sm_gain.set_sample_rate(sr);
            self.sm_output.set_sample_rate(sr);
        }
    }
}

pub(crate) static LIMITER_PARAMS: &[ParamDesc] = &[
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
        let ceiling_db = param_val(params, "ceiling_db", 0.0) as f64;
        let release_knob = param_val(params, "release", 0.05) as f64;

        let input_gain = db_to_lin(gain_db);
        let ceiling_lin = db_to_lin(ceiling_db);
        let release_coeff = fast_exp(-1.0 / (release_knob.max(0.001) * sr));

        let la = self.lookahead;
        let il = left * input_gain;
        let ir = right * input_gain;
        let peak = il.abs().max(ir.abs());
        let target_gr = if peak > ceiling_lin && peak > 1e-10 {
            ceiling_lin / peak
        } else {
            1.0
        };

        self.delay_buf[self.write_pos * 2] = il;
        self.delay_buf[self.write_pos * 2 + 1] = ir;
        self.gain_buf[self.write_pos] = target_gr;

        let read_pos = (self.write_pos + 1) % la;
        let dl = self.delay_buf[read_pos * 2];
        let dr = self.delay_buf[read_pos * 2 + 1];

        let mut read_gr = 1.0_f64;
        for k in 0..la {
            let idx = (read_pos + k) % la;
            let tgr = self.gain_buf[idx];
            if tgr < read_gr {
                read_gr = tgr;
            }
        }

        self.smooth_gr = if read_gr < self.smooth_gr {
            read_gr
        } else {
            self.smooth_gr + (read_gr - self.smooth_gr) * (1.0 - release_coeff)
        };
        let gr = self.smooth_gr;
        self.write_pos = (self.write_pos + 1) % la;

        let ol = dl * gr;
        let or_ = dr * gr;
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        let (mut fl, mut fr) = if out_db.abs() < 0.001 {
            (ol, or_)
        } else {
            let g = db_to_linear(out_db);
            (ol * g, or_ * g)
        };
        fl = fl.clamp(-ceiling_lin, ceiling_lin);
        fr = fr.clamp(-ceiling_lin, ceiling_lin);
        (fl, fr)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxLimiter::new())
    }
    fn gain_reduction_db(&self) -> f32 {
        if self.smooth_gr > 1e-10 && self.smooth_gr < 1.0 {
            (20.0 * self.smooth_gr.log10()) as f32
        } else if self.smooth_gr <= 1e-10 {
            -60.0
        } else {
            0.0
        }
    }
}
