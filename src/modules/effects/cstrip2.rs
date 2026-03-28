// Eden DAW — CStrip2 effect (Airwindows-style channel strip)

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

#[derive(Clone, Default)]
struct CsHpLpState {
    hp: [f64; 6],
    lp: [f64; 6],
}

#[derive(Clone, Default)]
struct CsCompState {
    avg: f64,
    nvg: f64,
    tar_pos: f64,
    tar_neg: f64,
    ctrl_a_pos: f64,
    ctrl_b_pos: f64,
    ctrl_a_neg: f64,
    ctrl_b_neg: f64,
}

pub struct CStrip2 {
    fl: CsHpLpState,
    fr: CsHpLpState,
    iir_hl: f64,
    iir_hr: f64,
    cl: CsCompState,
    cr: CsCompState,
    tri_la: f64,
    tri_lb: f64,
    tri_lc: f64,
    tri_ra: f64,
    tri_rb: f64,
    tri_rc: f64,
    last_l: f64,
    last2_l: f64,
    last_r: f64,
    last2_r: f64,
    fpd_l: u32,
    fpd_r: u32,
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
            tri_la: 0.0,
            tri_lb: 0.0,
            tri_lc: 0.0,
            tri_ra: 0.0,
            tri_rb: 0.0,
            tri_rc: 0.0,
            last_l: 0.0,
            last2_l: 0.0,
            last_r: 0.0,
            last2_r: 0.0,
            fpd_l: 1,
            fpd_r: 1,
            flip: false,
            flip3: 0,
            count: 0,
        }
    }

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

    #[inline]
    fn butter_comp(cs: &mut CsCompState, inp: f64, spd: f64, compress: f64) -> f64 {
        cs.avg = cs.avg * (1.0 - spd) + inp * spd;
        cs.nvg = cs.nvg * (1.0 - spd) + (cs.avg - inp).abs() * spd;

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
        let gain_clamped = gain.clamp(0.0, 2.0);
        inp * (1.0 - compress) + inp * gain_clamped * compress
    }
}

pub(crate) static CSTRIP2_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "treble",
        name: "Treble",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "mid",
        name: "Mid",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "bass",
        name: "Bass",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "treb_frq",
        name: "TrebFreq",
        default: 0.55,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "bass_frq",
        name: "BassFreq",
        default: 0.15,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "lo_cap",
        name: "LoCap",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "hi_cap",
        name: "HiCap",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "compress",
        name: "Compress",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "comp_spd",
        name: "CompSpd",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "output",
        name: "Trim",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for CStrip2 {
    fn name(&self) -> &'static str {
        "CStrip2"
    }
    fn params(&self) -> &'static [ParamDesc] {
        CSTRIP2_PARAMS
    }

    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let treble = param_val(params, "treble", 0.5) as f64;
        let mid = param_val(params, "mid", 0.5) as f64;
        let bass = param_val(params, "bass", 0.5) as f64;
        let treb_frq = param_val(params, "treb_frq", 0.55) as f64;
        let bass_frq = param_val(params, "bass_frq", 0.15) as f64;
        let lo_cap = param_val(params, "lo_cap", 0.0) as f64;
        let hi_cap = param_val(params, "hi_cap", 0.0) as f64;
        let compress = param_val(params, "compress", 0.0) as f64;
        let comp_spd = param_val(params, "comp_spd", 0.0) as f64;
        let output = param_val(params, "output", 0.5) as f64;

        let sr_ratio = if sr > 0.0 { 44100.0 / sr } else { 1.0 };

        #[inline]
        fn warp(c: f64, sr_ratio: f64) -> f64 {
            if (sr_ratio - 1.0).abs() < 1e-6 {
                return c;
            }
            1.0 - (1.0 - c).powf(sr_ratio)
        }

        let hp_coef = if lo_cap > 0.0 {
            warp(lo_cap.powf(2.0) * 0.4995 + 0.0001, sr_ratio)
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

        let lp_coef = if hi_cap > 0.0 {
            warp(hi_cap.powf(2.0) * 0.4995 + 0.0001, sr_ratio)
        } else {
            0.0
        };
        if lp_coef > 1e-6 {
            l = Self::apply_lpcap(&mut self.fl.lp, l, lp_coef);
            r = Self::apply_lpcap(&mut self.fr.lp, r, lp_coef);
        }

        let bass_coef = warp(bass_frq * bass_frq * 0.499 + 0.001, sr_ratio);
        let treb_coef = warp(treb_frq * treb_frq * 0.499 + 0.001, sr_ratio);

        self.tri_la = self.tri_la * (1.0 - bass_coef) + l * bass_coef;
        self.tri_ra = self.tri_ra * (1.0 - bass_coef) + r * bass_coef;
        self.tri_lc = self.tri_lc * (1.0 - treb_coef) + l * treb_coef;
        self.tri_rc = self.tri_rc * (1.0 - treb_coef) + r * treb_coef;
        self.tri_lb = l - self.tri_la - (l - self.tri_lc);
        self.tri_rb = r - self.tri_ra - (r - self.tri_rc);

        let bass_g = (bass * 2.0 - 1.0) * 0.5 + 1.0;
        let mid_g = (mid * 2.0 - 1.0) * 0.5 + 1.0;
        let treble_g = (treble * 2.0 - 1.0) * 0.5 + 1.0;

        l = self.tri_la * bass_g + self.tri_lb * mid_g + (l - self.tri_lc) * treble_g;
        r = self.tri_ra * bass_g + self.tri_rb * mid_g + (r - self.tri_rc) * treble_g;

        if compress > 0.001 {
            let spd = warp(comp_spd * comp_spd * 0.299 + 0.001, sr_ratio);
            l = Self::butter_comp(&mut self.cl, l, spd, compress);
            r = Self::butter_comp(&mut self.cr, r, spd, compress);
        }

        let trim_db = (output - 0.5) * 100.0;
        let out_gain = 10.0_f64.powf(trim_db / 20.0);
        l *= out_gain;
        r *= out_gain;

        (l, r)
    }

    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(CStrip2::new())
    }
    fn reset(&mut self) {
        *self = CStrip2::new();
    }
}
