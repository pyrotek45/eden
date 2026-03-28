// Eden DAW — SuperSawSynth (HyperSaw) instrument
//
// JP-8000-style dual 7-oscillator detuned saw with stereo width.

use crate::modules::dsp_primitives::*;
use crate::modules::{InstrumentModule, ModuleExtra, ModuleVoice, ParamDesc};

pub struct SuperSawSynth;

const JP8000_DETUNE_COEFS: [f64; 7] = [0.0, 128.0, -128.0, 408.0, -412.0, 704.0, -720.0];

const SUPERSAW_PAN_L: [f64; 7] = [
    std::f64::consts::FRAC_1_SQRT_2,
    1.0,
    0.0,
    0.891006524188368,
    0.45399049973954675,
    0.7933533402912352,
    0.6087614290087207,
];
const SUPERSAW_PAN_R: [f64; 7] = [
    std::f64::consts::FRAC_1_SQRT_2,
    0.0,
    1.0,
    0.45399049973954675,
    0.891006524188368,
    0.6087614290087207,
    0.7933533402912352,
];
const PAN_CENTER: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Process one JP-8000-style supersaw bank (7 oscillators) with stereo width.
#[inline(always)]
pub fn supersaw_bank(
    phases: &mut [f64],
    base_freq: f64,
    detune_amt: f64,
    mix: f64,
    width: f64,
    sample_rate: f64,
) -> (f64, f64) {
    let base_inc = base_freq / sample_rate;
    let detune_base = base_inc * detune_amt;

    let center_atten = 25.0 / 128.0;
    let side_atten = mix;

    let mut sum_l = 0.0_f64;
    let mut sum_r = 0.0_f64;

    for i in 0..7 {
        let voice_detune = JP8000_DETUNE_COEFS[i] * detune_base * (1.0 / 128.0);
        let inc = base_inc + voice_detune;
        let phase = phases[i];

        let naive = 2.0 * phase - 1.0;
        let saw = naive - polyblep(phase, inc.abs().max(1e-12));

        let weight = if i == 0 { center_atten } else { side_atten };
        let s = saw * weight;

        let pan_l = PAN_CENTER + (SUPERSAW_PAN_L[i] - PAN_CENTER) * width;
        let pan_r = PAN_CENTER + (SUPERSAW_PAN_R[i] - PAN_CENTER) * width;
        sum_l += s * pan_l;
        sum_r += s * pan_r;

        let next = phase + inc;
        phases[i] = next - next.floor();
    }
    let total_weight = center_atten + 6.0 * side_atten;
    let norm = if total_weight > 1e-9 {
        1.0 / total_weight
    } else {
        1.0
    };
    (sum_l * norm, sum_r * norm)
}

pub(crate) static SUPERSAW_PARAMS: &[ParamDesc] = &[
    // ── Oscillator 1 ──
    ParamDesc {
        id: "osc1_detune",
        name: "O1 Detune",
        default: 0.01,
        min: 0.0,
        max: 0.04,
        options: None,
    },
    ParamDesc {
        id: "osc1_mix",
        name: "O1 Mix",
        default: 0.75,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "osc1_width",
        name: "O1 Width",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Oscillator 2 ──
    ParamDesc {
        id: "osc2_detune",
        name: "O2 Detune",
        default: 0.01,
        min: 0.0,
        max: 0.04,
        options: None,
    },
    ParamDesc {
        id: "osc2_mix",
        name: "O2 Mix",
        default: 0.75,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_width",
        name: "O2 Width",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Oscillator blend + tuning ──
    ParamDesc {
        id: "osc_blend",
        name: "Osc Blend",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_semi",
        name: "O2 Semi",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "osc2_fine",
        name: "O2 Fine",
        default: 0.0,
        min: -100.0,
        max: 100.0,
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
        id: "noise_gain",
        name: "Noise",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "noise_hp",
        name: "Noise HP",
        default: 0.15,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    // ── Filter ──
    ParamDesc {
        id: "filter_cutoff",
        name: "Cutoff",
        default: 0.9,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_reso",
        name: "Reso",
        default: 0.1,
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
        default: 0.3,
        min: 0.001,
        max: 8.0,
        options: None,
    },
    // ── Filter env cont + Amp ADSR ──
    ParamDesc {
        id: "filter_s",
        name: "F.Sus",
        default: 0.3,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "filter_r",
        name: "F.Rel",
        default: 0.4,
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

impl InstrumentModule for SuperSawSynth {
    fn name(&self) -> &'static str {
        "HyperSaw"
    }
    fn params(&self) -> &'static [ParamDesc] {
        SUPERSAW_PARAMS
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

        let osc1_detune = param_val(params, "osc1_detune", 0.1) as f64;
        let osc1_mix = param_val(params, "osc1_mix", 0.75) as f64;
        let osc1_width = param_val(params, "osc1_width", 0.5) as f64;
        let osc2_detune = param_val(params, "osc2_detune", 0.1) as f64;
        let osc2_mix = param_val(params, "osc2_mix", 0.75) as f64;
        let osc2_width = param_val(params, "osc2_width", 0.5) as f64;
        let osc_blend = param_val(params, "osc_blend", 0.0) as f64;
        let osc2_semi = param_val(params, "osc2_semi", 0.0) as f64;
        let osc2_fine = param_val(params, "osc2_fine", 0.0) as f64;
        let gain = db_to_lin(param_val(params, "gain", 0.0) as f64);
        let noise_gain = param_val(params, "noise_gain", 0.0) as f64;
        let noise_hp_norm = param_val(params, "noise_hp", 0.15) as f64;
        let filter_cutoff_norm = param_val(params, "filter_cutoff", 0.9) as f64;
        let filter_reso = param_val(params, "filter_reso", 0.1) as f64;
        let filter_env_amt = param_val(params, "filter_env", 0.0) as f64;
        let filter_a = param_val(params, "filter_a", 0.01) as f64;
        let filter_d = param_val(params, "filter_d", 0.3) as f64;
        let filter_s = param_val(params, "filter_s", 0.3) as f64;
        let filter_r = param_val(params, "filter_r", 0.4) as f64;
        let amp_a = param_val(params, "amp_a", 0.01) as f64;
        let amp_d = param_val(params, "amp_d", 0.1) as f64;
        let amp_s = param_val(params, "amp_s", 0.8) as f64;
        let amp_r = param_val(params, "amp_r", 0.3) as f64;

        // ── Dual SuperSaw oscillators (stereo) ──
        let freq1 = voice.freq;
        let (osc1_l, osc1_r) = supersaw_bank(
            &mut st.extra_phases[0..7],
            freq1,
            osc1_detune,
            osc1_mix,
            osc1_width,
            sample_rate,
        );

        let detune_ratio = fast_pow2((osc2_semi + osc2_fine / 100.0) / 12.0);
        let freq2 = voice.freq * detune_ratio;
        let (osc2_l, osc2_r) = supersaw_bank(
            &mut st.extra_phases[7..14],
            freq2,
            osc2_detune,
            osc2_mix,
            osc2_width,
            sample_rate,
        );

        let osc_l = osc1_l * (1.0 - osc_blend) + osc2_l * osc_blend;
        let osc_r = osc1_r * (1.0 - osc_blend) + osc2_r * osc_blend;

        // ── White noise with highpass filter ──
        let noise = if noise_gain > 0.001 {
            let mut s = st.noise_seed;
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            st.noise_seed = s;
            let raw_noise =
                (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
            let noise_hp_hz = 20.0 * fast_pow2(noise_hp_norm * 8.64);
            let (_, _, hp_noise) = svf_tick(
                raw_noise,
                noise_hp_hz,
                0.0,
                sample_rate,
                &mut st.noise_hp_ic1,
                &mut st.noise_hp_ic2,
            );
            hp_noise
        } else {
            0.0
        };
        let osc_l = osc_l + noise * noise_gain;
        let osc_r = osc_r + noise * noise_gain;

        // ── Highpass on detuned saws (JP-8000 characteristic) ──
        let (_, _, hp_l) = svf_tick(
            osc_l,
            20.0,
            0.0,
            sample_rate,
            &mut st.hp_ic1,
            &mut st.hp_ic2,
        );
        let (_, _, hp_r) = svf_tick(
            osc_r,
            20.0,
            0.0,
            sample_rate,
            &mut st.hp_ic1_r,
            &mut st.hp_ic2_r,
        );

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

        let (filt_l, _, _) = svf_tick(
            hp_l,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1,
            &mut st.filt_ic2,
        );
        let (filt_r, _, _) = svf_tick(
            hp_r,
            cutoff_hz,
            filter_reso,
            sample_rate,
            &mut st.filt_ic1_r,
            &mut st.filt_ic2_r,
        );

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

        let g = amp_env * gain * (voice.velocity as f64);
        (filt_l * g, filt_r * g)
    }
}
