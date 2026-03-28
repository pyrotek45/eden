// Eden DAW — EQ effect (3-band parametric)

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

const DENORMAL_FIX: f64 = 1e-18;

#[derive(Debug, Clone)]
struct BiquadState {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}
impl BiquadState {
    fn new() -> Self {
        Self {
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
    fn tick(&mut self, x: f64, b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> f64 {
        let y = b0 * x + b1 * self.x1 + b2 * self.x2 - a1 * self.y1 - a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y + DENORMAL_FIX;
        y
    }
}

#[derive(Clone, Copy)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl BiquadCoeffs {
    pub fn low_shelf(freq: f64, gain_db: f64, sr: f64) -> Self {
        let a = 10.0_f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / 2.0 * (2.0_f64).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        Self {
            b0: (a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0,
            b1: (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0,
            b2: (a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0,
            a1: (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0,
            a2: ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
        }
    }

    pub fn high_shelf(freq: f64, gain_db: f64, sr: f64) -> Self {
        let a = 10.0_f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / 2.0 * (2.0_f64).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        Self {
            b0: (a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0,
            b1: (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0,
            b2: (a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0,
            a1: (2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0,
            a2: ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
        }
    }

    pub fn peaking(freq: f64, gain_db: f64, q: f64, sr: f64) -> Self {
        let a = 10.0_f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * freq / sr;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos_w0) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    pub fn magnitude_at(&self, omega: f64) -> f64 {
        let cos1 = omega.cos();
        let cos2 = (2.0 * omega).cos();
        let sin1 = omega.sin();
        let sin2 = (2.0 * omega).sin();
        let num_re = self.b0 + self.b1 * cos1 + self.b2 * cos2;
        let num_im = -(self.b1 * sin1 + self.b2 * sin2);
        let den_re = 1.0 + self.a1 * cos1 + self.a2 * cos2;
        let den_im = -(self.a1 * sin1 + self.a2 * sin2);
        let num_sq = num_re * num_re + num_im * num_im;
        let den_sq = den_re * den_re + den_im * den_im;
        if den_sq > 1e-30 {
            (num_sq / den_sq).sqrt()
        } else {
            1.0
        }
    }
}

pub struct FxEq {
    lo_l: BiquadState,
    lo_r: BiquadState,
    mid_l: BiquadState,
    mid_r: BiquadState,
    hi_l: BiquadState,
    hi_r: BiquadState,
    sm_lo_gain: SmoothedParam,
    sm_mid_gain: SmoothedParam,
    sm_hi_gain: SmoothedParam,
    sm_lo_freq: SmoothedParam,
    sm_mid_freq: SmoothedParam,
    sm_hi_freq: SmoothedParam,
    sm_output: SmoothedParam,
    lo_coeffs: BiquadCoeffs,
    mid_coeffs: BiquadCoeffs,
    hi_coeffs: BiquadCoeffs,
    last_lo_f: f64,
    last_lo_g: f64,
    last_mid_f: f64,
    last_mid_g: f64,
    last_hi_f: f64,
    last_hi_g: f64,
    last_sr: f64,
}
impl FxEq {
    pub fn new() -> Self {
        let unity = BiquadCoeffs {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        };
        Self {
            lo_l: BiquadState::new(),
            lo_r: BiquadState::new(),
            mid_l: BiquadState::new(),
            mid_r: BiquadState::new(),
            hi_l: BiquadState::new(),
            hi_r: BiquadState::new(),
            sm_lo_gain: SmoothedParam::new(0.0, 44100.0),
            sm_mid_gain: SmoothedParam::new(0.0, 44100.0),
            sm_hi_gain: SmoothedParam::new(0.0, 44100.0),
            sm_lo_freq: SmoothedParam::new(200.0, 44100.0),
            sm_mid_freq: SmoothedParam::new(1000.0, 44100.0),
            sm_hi_freq: SmoothedParam::new(4000.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
            lo_coeffs: unity,
            mid_coeffs: unity,
            hi_coeffs: unity,
            last_lo_f: 0.0,
            last_lo_g: -999.0,
            last_mid_f: 0.0,
            last_mid_g: -999.0,
            last_hi_f: 0.0,
            last_hi_g: -999.0,
            last_sr: 0.0,
        }
    }
}

pub(crate) static EQ_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "lo_gain",
        name: "Lo Gain",
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
        id: "mid_gain",
        name: "Mid Gain",
        default: 0.0,
        min: -12.0,
        max: 12.0,
        options: None,
    },
    ParamDesc {
        id: "mid_freq",
        name: "Mid Freq",
        default: 1000.0,
        min: 100.0,
        max: 10000.0,
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
        let mid_f = self
            .sm_mid_freq
            .tick(param_val(params, "mid_freq", 1000.0) as f64)
            .clamp(20.0, sr * 0.49);
        let hi_f = self
            .sm_hi_freq
            .tick(param_val(params, "hi_freq", 4000.0) as f64)
            .clamp(20.0, sr * 0.49);

        let thresh_f = 0.5;
        let thresh_g = 0.01;
        if (lo_f - self.last_lo_f).abs() > thresh_f
            || (lo_g - self.last_lo_g).abs() > thresh_g
            || (sr - self.last_sr).abs() > 1.0
        {
            self.lo_coeffs = if lo_g.abs() > 0.01 {
                BiquadCoeffs::low_shelf(lo_f, lo_g, sr)
            } else {
                BiquadCoeffs {
                    b0: 1.0,
                    b1: 0.0,
                    b2: 0.0,
                    a1: 0.0,
                    a2: 0.0,
                }
            };
            self.last_lo_f = lo_f;
            self.last_lo_g = lo_g;
        }
        if (mid_f - self.last_mid_f).abs() > thresh_f
            || (mid_g - self.last_mid_g).abs() > thresh_g
            || (sr - self.last_sr).abs() > 1.0
        {
            self.mid_coeffs = if mid_g.abs() > 0.01 {
                BiquadCoeffs::peaking(mid_f, mid_g, 0.7, sr)
            } else {
                BiquadCoeffs {
                    b0: 1.0,
                    b1: 0.0,
                    b2: 0.0,
                    a1: 0.0,
                    a2: 0.0,
                }
            };
            self.last_mid_f = mid_f;
            self.last_mid_g = mid_g;
        }
        if (hi_f - self.last_hi_f).abs() > thresh_f
            || (hi_g - self.last_hi_g).abs() > thresh_g
            || (sr - self.last_sr).abs() > 1.0
        {
            self.hi_coeffs = if hi_g.abs() > 0.01 {
                BiquadCoeffs::high_shelf(hi_f, hi_g, sr)
            } else {
                BiquadCoeffs {
                    b0: 1.0,
                    b1: 0.0,
                    b2: 0.0,
                    a1: 0.0,
                    a2: 0.0,
                }
            };
            self.last_hi_f = hi_f;
            self.last_hi_g = hi_g;
        }
        self.last_sr = sr;

        let c = &self.lo_coeffs;
        let mut l = self.lo_l.tick(left, c.b0, c.b1, c.b2, c.a1, c.a2);
        let mut r = self.lo_r.tick(right, c.b0, c.b1, c.b2, c.a1, c.a2);
        let c = &self.mid_coeffs;
        l = self.mid_l.tick(l, c.b0, c.b1, c.b2, c.a1, c.a2);
        r = self.mid_r.tick(r, c.b0, c.b1, c.b2, c.a1, c.a2);
        let c = &self.hi_coeffs;
        l = self.hi_l.tick(l, c.b0, c.b1, c.b2, c.a1, c.a2);
        r = self.hi_r.tick(r, c.b0, c.b1, c.b2, c.a1, c.a2);

        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (l, r)
        } else {
            let g = db_to_linear(out_db);
            (l * g, r * g)
        }
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxEq::new())
    }
}
