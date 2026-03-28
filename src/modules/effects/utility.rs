// Eden DAW — Utility effect (Gain + Pan + Phase Invert + DC Offset)

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

pub struct FxUtility {
    sm_gain: SmoothedParam,
    sm_pan: SmoothedParam,
    sm_dc: SmoothedParam,
}
impl FxUtility {
    pub fn new() -> Self {
        Self {
            sm_gain: SmoothedParam::new(0.0, 44100.0),
            sm_pan: SmoothedParam::new(0.0, 44100.0),
            sm_dc: SmoothedParam::new(0.0, 44100.0),
        }
    }
}

pub(crate) static UTILITY_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "gain_db",
        name: "Gain dB",
        default: 0.0,
        min: -60.0,
        max: 24.0,
        options: None,
    },
    ParamDesc {
        id: "pan",
        name: "Pan",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "phase",
        name: "Phase",
        default: 0.0,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "dc_offset",
        name: "DC Offset",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
];

impl EffectModule for FxUtility {
    fn name(&self) -> &'static str {
        "Utility"
    }
    fn params(&self) -> &'static [ParamDesc] {
        UTILITY_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], _sr: f64) -> (f64, f64) {
        let db = self.sm_gain.tick(param_val(params, "gain_db", 0.0) as f64);
        let pan = self.sm_pan.tick(param_val(params, "pan", 0.0) as f64);
        let phase_inv = param_val(params, "phase", 0.0);
        let dc = self.sm_dc.tick(param_val(params, "dc_offset", 0.0) as f64);
        let gain = db_to_lin(db);
        let polarity = if phase_inv > 0.5 { -1.0 } else { 1.0 };
        let pan_l = if pan > 0.0 { 1.0 - pan } else { 1.0 };
        let pan_r = if pan < 0.0 { 1.0 + pan } else { 1.0 };
        let out_l = (left + dc) * gain * polarity * pan_l;
        let out_r = (right + dc) * gain * polarity * pan_r;
        (out_l, out_r)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxUtility::new())
    }
}
