// Eden DAW — HeavySynth (Monolith) instrument

use crate::modules::dsp_primitives::*;
use crate::modules::{InstrumentModule, ModuleExtra, ModuleVoice, ParamDesc};

pub struct HeavySynth;

/// Oscillator shapes for HeavySynth.
#[inline]
fn heavy_osc_shape(shape: usize, phase: f64, dt: f64) -> f64 {
    match shape {
        0 => {
            let pw = 0.05;
            let mut s = if phase < pw { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 1.0 - pw) % 1.0, dt);
            s
        }
        1 => {
            let naive = 2.0 * phase - 1.0;
            naive - polyblep(phase, dt)
        }
        2 => {
            if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            }
        }
        3 => {
            let rise = 0.15;
            if phase < rise {
                phase / rise * 2.0 - 1.0
            } else {
                1.0 - 2.0 * (phase - rise) / (1.0 - rise)
            }
        }
        4 => {
            let mut s = if phase < 0.5 { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 0.5) % 1.0, dt);
            s
        }
        5 => {
            let pw = 0.30;
            let mut s = if phase < pw { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 1.0 - pw) % 1.0, dt);
            s
        }
        6 => {
            let pw = 0.45;
            let mut s = if phase < pw { 1.0 } else { -1.0 };
            s += polyblep(phase, dt);
            s -= polyblep((phase + 1.0 - pw) % 1.0, dt);
            s * 0.85
        }
        7 => {
            let mut sq = if phase < 0.5 { 1.0 } else { -1.0 };
            sq += polyblep(phase, dt);
            sq -= polyblep((phase + 0.5) % 1.0, dt);
            let tri = if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            };
            (sq + tri) * 0.5
        }
        _ => 0.0,
    }
}

pub(crate) static HEAVY_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "osc_shape",
        name: "Shape",
        default: 1.0,
        min: 0.0,
        max: 7.0,
        options: Some(&[
            "Impulse",
            "Saw",
            "Triangle",
            "Slope",
            "Square",
            "Sq Bright",
            "Sq Dark",
            "Sq-Tri",
        ]),
    },
    ParamDesc {
        id: "sub_level",
        name: "Sub",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "noise_mix",
        name: "Noise",
        default: 0.0,
        min: 0.0,
        max: 1.0,
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
    ParamDesc {
        id: "filter_cutoff",
        name: "Cutoff",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_reso",
        name: "Reso",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_env",
        name: "Env Amt",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_a",
        name: "F.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "filter_d",
        name: "F.Dec",
        default: 0.2,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "filter_s",
        name: "F.Sus",
        default: 0.4,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_r",
        name: "F.Rel",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_a",
        name: "A.Atk",
        default: 0.01,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_d",
        name: "A.Dec",
        default: 0.1,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "amp_s",
        name: "A.Sus",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "amp_r",
        name: "A.Rel",
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    ParamDesc {
        id: "dist_drive",
        name: "Drive",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "dist_type",
        name: "Dist Type",
        default: 0.0,
        min: 0.0,
        max: 3.0,
        options: Some(&["Tanh", "Clip", "Sine", "Bit"]),
    },
];

impl InstrumentModule for HeavySynth {
    fn name(&self) -> &'static str {
        "Monolith"
    }
    fn params(&self) -> &'static [ParamDesc] {
        HEAVY_PARAMS
    }

    fn process_voice(
        &self,
        voice: &mut ModuleVoice,
        params: &[(String, f32)],
        sample_rate: f64,
        _extra: &ModuleExtra,
    ) -> (f64, f64) {
        let dt = 1.0 / sample_rate;
        let st = &mut voice.state;

        let osc_shape = param_val(params, "osc_shape", 1.0) as usize;
        let sub_level = param_val(params, "sub_level", 0.0) as f64;
        let noise_mix = param_val(params, "noise_mix", 0.0) as f64;
        let gain = db_to_lin(param_val(params, "gain", 0.0) as f64);
        let filter_cutoff_norm = param_val(params, "filter_cutoff", 0.8) as f64;
        let filter_reso = param_val(params, "filter_reso", 0.0) as f64;
        let filter_env_amt = param_val(params, "filter_env", 0.0) as f64;
        let filter_a = param_val(params, "filter_a", 0.01) as f64;
        let filter_d = param_val(params, "filter_d", 0.2) as f64;
        let filter_s = param_val(params, "filter_s", 0.4) as f64;
        let filter_r = param_val(params, "filter_r", 0.3) as f64;
        let amp_a = param_val(params, "amp_a", 0.01) as f64;
        let amp_d = param_val(params, "amp_d", 0.1) as f64;
        let amp_s = param_val(params, "amp_s", 0.8) as f64;
        let amp_r = param_val(params, "amp_r", 0.3) as f64;
        let dist_drive = param_val(params, "dist_drive", 0.0) as f64;
        let dist_type = param_val(params, "dist_type", 0.0) as usize;

        let osc_inc = voice.freq / sample_rate;
        let main_osc = heavy_osc_shape(osc_shape.min(7), st.phase0, osc_inc);
        st.phase0 += osc_inc;
        if st.phase0 >= 1.0 {
            st.phase0 -= 1.0;
        }

        let sub_inc = osc_inc * 0.5;
        let mut sub_osc = if st.phase1 < 0.5 { 1.0 } else { -1.0 };
        sub_osc += polyblep(st.phase1, sub_inc);
        sub_osc -= polyblep((st.phase1 + 0.5) % 1.0, sub_inc);
        st.phase1 += sub_inc;
        if st.phase1 >= 1.0 {
            st.phase1 -= 1.0;
        }

        let noise = {
            let mut s = st.noise_seed;
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            st.noise_seed = s;
            let raw =
                (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
            let (_lp, _bp, hp) = svf_tick(
                raw,
                80.0,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1,
                &mut st.noise_hp_ic2,
            );
            hp
        };

        let osc_out = main_osc * (1.0 - noise_mix) + noise * noise_mix + sub_osc * sub_level;

        let filt_env = adsr_tick(
            &mut st.filt_stage,
            &mut st.filt_level,
            &mut st.filt_time,
            filter_a,
            filter_d,
            filter_s,
            filter_r,
            dt,
            voice.released,
        );
        let base_hz = 20.0 * fast_pow2(filter_cutoff_norm * 9.965784284662087);
        let env_octaves = filter_env_amt * filt_env * 8.0;
        let cutoff_hz = (base_hz * fast_pow2(env_octaves)).clamp(20.0, sample_rate * 0.49);

        let (lp, _bp, _hp) = svf_tick(
            osc_out,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1,
            &mut st.filt_ic2,
        );

        let filtered = if dist_drive > 0.001 {
            match dist_type {
                0 => {
                    let d = 1.0 + dist_drive * 15.0;
                    let x = lp * d;
                    let x2 = x * x;
                    let num = x * (27.0 + x2);
                    let den_val = 27.0 + 9.0 * x2;
                    let tanh_x = num / den_val;
                    let d2 = d * d;
                    let tanh_d = d * (27.0 + d2) / (27.0 + 9.0 * d2);
                    if tanh_d.abs() < 1e-9 {
                        lp
                    } else {
                        tanh_x / tanh_d
                    }
                }
                1 => {
                    let th = (1.0 - dist_drive * 0.85).max(0.01);
                    lp.clamp(-th, th) / th
                }
                2 => fast_sin(lp * (1.0 + dist_drive * 5.0) * std::f64::consts::PI),
                3 => {
                    let steps = fast_pow2(14.0 - dist_drive * 12.0).max(1.0);
                    (lp * steps + 0.5).floor() / steps
                }
                _ => lp,
            }
        } else {
            lp
        };

        let amp_env = adsr_tick(
            &mut st.amp_stage,
            &mut st.amp_level,
            &mut st.amp_time,
            amp_a,
            amp_d,
            amp_s,
            amp_r,
            dt,
            voice.released,
        );

        let mono = filtered * amp_env * gain * (voice.velocity as f64);
        (mono, mono)
    }
}
