// Eden DAW — Reverb effect (Dragonfly-inspired two-stage hall reverb)

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

pub struct FxReverb {
    sr: f64,
    pre_buf: Vec<f32>,
    pre_head: usize,
    early_buf_l: Vec<f32>,
    early_buf_r: Vec<f32>,
    early_head: usize,
    comb_buf_l: [Vec<f32>; 4],
    comb_buf_r: [Vec<f32>; 4],
    comb_head_l: [usize; 4],
    comb_head_r: [usize; 4],
    comb_filt_l: [f64; 4],
    comb_filt_r: [f64; 4],
    ap_buf_l: [Vec<f32>; 4],
    ap_buf_r: [Vec<f32>; 4],
    ap_head_l: [usize; 4],
    ap_head_r: [usize; 4],
    lfo_phase: f64,
    lfo_wander_phase: f64,
    hc_state_l: f64,
    hc_state_r: f64,
    lc_state_l: f64,
    lc_state_r: f64,
    hx_state_l: [f64; 4],
    hx_state_r: [f64; 4],
    lx_state_l: [f64; 4],
    lx_state_r: [f64; 4],
    sm_mix: SmoothedParam,
    sm_decay: SmoothedParam,
    sm_output: SmoothedParam,
}

impl FxReverb {
    const EARLY_TAP_PRIMES: [f64; 8] = [2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 17.0, 19.0];
    const COMB_MS_L: [f64; 4] = [29.13, 34.07, 38.93, 43.11];
    const COMB_MS_R: [f64; 4] = [30.61, 35.29, 40.37, 44.71];
    const AP_MS_L: [f64; 4] = [5.02, 1.68, 4.01, 1.24];
    const AP_MS_R: [f64; 4] = [5.31, 1.83, 3.78, 1.41];

    pub fn new(sr: u32) -> Self {
        let sr = sr as f64;
        let max_ms = |ms: f64| -> usize { ((sr * ms / 1000.0) as usize + 4).max(8) };

        let pre_len = max_ms(110.0);
        let early_len = max_ms(30.0 * 19.0);

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

    #[inline]
    fn lp_coeff(freq: f64, sr: f64) -> f64 {
        let w = (std::f64::consts::TAU * freq / sr).min(0.99);
        w / (1.0 + w)
    }

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
        let rt60 = decay.clamp(0.1, 10.0);
        let size_factor = 0.5 + (size / 60.0).clamp(0.0, 1.0) * 2.5;

        let mut sum_l = 0.0_f64;
        let mut sum_r = 0.0_f64;

        for i in 0..4 {
            let comb_ms = Self::COMB_MS_L[i] * size_factor;
            let comb_s = comb_ms / 1000.0;
            let fb_base = (10.0_f64).powf(-3.0 * comb_s / rt60).clamp(0.0, 0.9995);
            let lfo_offset = lfo_mod * (1.0 + wander_samples * 0.5) * (i as f64 * 1.3 + 0.1).sin();
            let delay_samples = (comb_ms * self.sr / 1000.0 + lfo_offset)
                .clamp(1.0, (self.comb_buf_l[i].len() - 2) as f64);
            let len = self.comb_buf_l[i].len();
            if len == 0 {
                continue;
            }
            let delayed =
                Self::read_interp(&self.comb_buf_l[i], self.comb_head_l[i], delay_samples);

            let hx_a = Self::lp_coeff(high_xover, self.sr);
            self.hx_state_l[i] += hx_a * (delayed - self.hx_state_l[i]);
            let low_band = self.hx_state_l[i];
            let high_band = delayed - low_band;

            let lx_a = Self::lp_coeff(low_xover, self.sr);
            self.lx_state_l[i] += lx_a * (low_band - self.lx_state_l[i]);
            let very_low = self.lx_state_l[i];
            let mid_band = low_band - very_low;

            let fb_low = fb_base * low_mult.clamp(0.2, 2.5);
            let fb_high = fb_base * high_mult.clamp(0.2, 2.5);
            let fb_mid = fb_base;
            let filtered = very_low * fb_low + mid_band * fb_mid + high_band * fb_high;

            let damping = 1.0 - high_mult.clamp(0.2, 2.5).min(1.0) * 0.3;
            self.comb_filt_l[i] =
                filtered * (1.0 - damping * 0.4) + self.comb_filt_l[i] * (damping * 0.4);

            let new_val = input + self.comb_filt_l[i];
            self.comb_buf_l[i][self.comb_head_l[i]] = new_val as f32;
            self.comb_head_l[i] = (self.comb_head_l[i] + 1) % len;
            sum_l += delayed;
        }

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

        let ap_fb = 0.3 + (diffuse / 100.0).clamp(0.0, 1.0) * 0.4;
        for i in 0..4 {
            let len_l = self.ap_buf_l[i].len();
            if len_l > 0 {
                let h_l = self.ap_head_l[i];
                let delayed_l = self.ap_buf_l[i][h_l] as f64;
                let new_l = sum_l + delayed_l * ap_fb;
                self.ap_buf_l[i][h_l] = new_l as f32;
                sum_l = delayed_l - new_l * ap_fb;
                self.ap_head_l[i] = (h_l + 1) % len_l;
            }
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

pub(crate) static REVERB_PARAMS: &[ParamDesc] = &[
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

        let mix_pct = self.sm_mix.tick(param_val(params, "mix", 50.0) as f64);
        let _dry_pct = param_val(params, "dry", 80.0) as f64;
        let early_pct = param_val(params, "early", 10.0) as f64;
        let early_send = param_val(params, "early_send", 20.0) as f64;
        let late_pct = param_val(params, "late", 20.0) as f64;
        let size = param_val(params, "size", 24.0) as f64;
        let width = param_val(params, "width", 100.0) as f64;
        let predelay = param_val(params, "predelay", 14.0) as f64;
        let decay = self.sm_decay.tick(param_val(params, "decay", 1.6) as f64);
        let diffuse = param_val(params, "diffuse", 80.0) as f64;
        let modulation = param_val(params, "modulation", 10.0) as f64;
        let spin = param_val(params, "spin", 0.40) as f64;
        let wander = param_val(params, "wander", 12.0) as f64;
        let high_cut = param_val(params, "high_cut", 16000.0) as f64;
        let high_xover = param_val(params, "high_xover", 5600.0) as f64;
        let high_mult = param_val(params, "high_mult", 0.5) as f64;
        let low_cut = param_val(params, "low_cut", 0.0) as f64;
        let low_xover = param_val(params, "low_xover", 500.0) as f64;
        let low_mult = param_val(params, "low_mult", 1.0) as f64;

        let early_gain = early_pct / 100.0;
        let early_send_gain = early_send / 100.0;
        let late_gain = late_pct / 100.0;
        let width_factor = width / 100.0;
        let mod_depth = modulation / 100.0;

        let mono_in = (left + right) * 0.5;

        // Predelay
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

        // LFOs
        self.lfo_phase += spin / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo_mod = fast_sin_phase(self.lfo_phase) * mod_depth;

        self.lfo_wander_phase += (spin * 0.23) / sr;
        if self.lfo_wander_phase >= 1.0 {
            self.lfo_wander_phase -= 1.0;
        }
        let wander_samples =
            fast_sin_phase(self.lfo_wander_phase) * (wander * sr / 1000.0) * mod_depth;

        // Early reflections
        let (early_l, early_r) = if early_gain > 0.001 || early_send_gain > 0.001 {
            self.process_early(pre_delayed, size)
        } else {
            (0.0, 0.0)
        };

        // Late tail
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

        let mut wet_l = early_l * early_gain + late_l * late_gain;
        let mut wet_r = early_r * early_gain + late_r * late_gain;

        // High cut filter
        if high_cut < 15900.0 {
            let hc_a = Self::lp_coeff(high_cut, sr);
            self.hc_state_l += hc_a * (wet_l - self.hc_state_l);
            self.hc_state_r += hc_a * (wet_r - self.hc_state_r);
            wet_l = self.hc_state_l;
            wet_r = self.hc_state_r;
        }

        // Low cut filter
        if low_cut > 1.0 {
            let (out_l, ns_l) = Self::hp_tick(wet_l, self.lc_state_l, low_cut, sr);
            let (out_r, ns_r) = Self::hp_tick(wet_r, self.lc_state_r, low_cut, sr);
            self.lc_state_l = ns_l;
            self.lc_state_r = ns_r;
            wet_l = out_l;
            wet_r = out_r;
        }

        // Width (mid/side)
        let mid = (wet_l + wet_r) * 0.5;
        let side = (wet_l - wet_r) * 0.5;
        let w_l = mid + side * width_factor;
        let w_r = mid - side * width_factor;

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
