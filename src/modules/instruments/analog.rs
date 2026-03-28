// Eden DAW — SubtractiveSynth (Analog) instrument

use crate::modules::dsp_primitives::*;
use crate::modules::{InstrumentModule, ModuleExtra, ModuleVoice, ParamDesc};

pub struct SubtractiveSynth;

pub(crate) static SUBTRACTIVE_PARAMS: &[ParamDesc] = &[
    // ── Oscillators ──
    ParamDesc {
        id: "osc1_wave",
        name: "Osc1 Shape",
        default: 1.0,
        min: 0.0,
        max: 4.0,
        options: Some(&["Sine", "Saw", "Square", "Triangle", "Noise"]),
    },
    ParamDesc {
        id: "osc2_wave",
        name: "Osc2 Shape",
        default: 1.0,
        min: 0.0,
        max: 4.0,
        options: Some(&["Sine", "Saw", "Square", "Triangle", "Noise"]),
    },
    ParamDesc {
        id: "osc_mix",
        name: "Osc Mix",
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
    // ── Oscillator tuning ──
    ParamDesc {
        id: "osc2_semi",
        name: "Semi",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_fine",
        name: "Fine",
        default: 0.0,
        min: -100.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "filter_type",
        name: "Filt Type",
        default: 0.0,
        min: 0.0,
        max: 2.0,
        options: Some(&["LowPass", "HighPass", "BandPass"]),
    },
    ParamDesc {
        id: "filter_cutoff",
        name: "Cutoff",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Filter ──
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
    // ── Filter env cont + Amp ADSR ──
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
    // ── Amp ADSR cont ──
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
];

impl InstrumentModule for SubtractiveSynth {
    fn name(&self) -> &'static str {
        "Analog"
    }
    fn params(&self) -> &'static [ParamDesc] {
        SUBTRACTIVE_PARAMS
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

        let osc1_shape = param_val(params, "osc1_wave", 1.0) as f64;
        let osc2_shape = param_val(params, "osc2_wave", 1.0) as f64;
        let osc2_semi = param_val(params, "osc2_semi", 0.0) as f64;
        let osc2_fine = param_val(params, "osc2_fine", 0.0) as f64;
        let osc_mix = param_val(params, "osc_mix", 0.0) as f64;
        let filter_cutoff_norm = param_val(params, "filter_cutoff", 0.8) as f64;
        let filter_reso = param_val(params, "filter_reso", 0.0) as f64;
        let filter_env_amt = param_val(params, "filter_env", 0.0) as f64;
        let filter_type = param_val(params, "filter_type", 0.0) as f64;
        let filter_a = param_val(params, "filter_a", 0.01) as f64;
        let filter_d = param_val(params, "filter_d", 0.2) as f64;
        let filter_s = param_val(params, "filter_s", 0.4) as f64;
        let filter_r = param_val(params, "filter_r", 0.3) as f64;
        let amp_a = param_val(params, "amp_a", 0.01) as f64;
        let amp_d = param_val(params, "amp_d", 0.1) as f64;
        let amp_s = param_val(params, "amp_s", 0.8) as f64;
        let amp_r = param_val(params, "amp_r", 0.3) as f64;
        let gain = db_to_lin(param_val(params, "gain", 0.0) as f64);

        // ── Oscillators with morphing ──
        let osc1_inc = voice.freq / sample_rate;
        let mut osc1 = osc_morph(osc1_shape, st.phase0, osc1_inc, &mut st.noise_seed);
        if osc1_shape >= 3.0 {
            let noise_frac = (osc1_shape - 3.0).min(1.0);
            let (_lp, _bp, hp) = svf_tick(
                osc1,
                80.0,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1,
                &mut st.noise_hp_ic2,
            );
            osc1 = osc1 * (1.0 - noise_frac) + hp * noise_frac;
        }

        let detune = fast_pow2((osc2_semi + osc2_fine / 100.0) / 12.0);
        let osc2_freq = voice.freq * detune;
        let osc2_inc = osc2_freq / sample_rate;
        let mut osc2 = osc_morph(osc2_shape, st.phase1, osc2_inc, &mut st.noise_seed);
        if osc2_shape >= 3.0 {
            let noise_frac = (osc2_shape - 3.0).min(1.0);
            let (_lp, _bp, hp) = svf_tick(
                osc2,
                80.0,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1b,
                &mut st.noise_hp_ic2b,
            );
            osc2 = osc2 * (1.0 - noise_frac) + hp * noise_frac;
        }

        st.phase0 += osc1_inc;
        if st.phase0 >= 1.0 {
            st.phase0 -= 1.0;
        }
        st.phase1 += osc2_inc;
        if st.phase1 >= 1.0 {
            st.phase1 -= 1.0;
        }

        let osc_out = osc1 * (1.0 - osc_mix) + osc2 * osc_mix;

        // ── Filter envelope ──
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

        let (lp, bp, hp) = svf_tick(
            osc_out,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1,
            &mut st.filt_ic2,
        );
        let filtered = if filter_type <= 1.0 {
            let t = filter_type;
            lp * (1.0 - t) + hp * t
        } else {
            let t = filter_type - 1.0;
            hp * (1.0 - t) + bp * t
        };

        // ── Amp envelope ──
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
