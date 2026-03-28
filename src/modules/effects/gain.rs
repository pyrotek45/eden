// Eden DAW — Gain effect

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

pub struct FxGain {
    sm_gain: SmoothedParam,
}
impl FxGain {
    pub fn new() -> Self {
        Self {
            sm_gain: SmoothedParam::new(0.0, 44100.0),
        }
    }
}

pub(crate) static GAIN_PARAMS: &[ParamDesc] = &[ParamDesc {
    id: "gain_db",
    name: "Gain dB",
    default: 0.0,
    min: -60.0,
    max: 24.0,
    options: None,
}];

impl EffectModule for FxGain {
    fn name(&self) -> &'static str {
        "Gain"
    }
    fn params(&self) -> &'static [ParamDesc] {
        GAIN_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], _sr: f64) -> (f64, f64) {
        let db = self.sm_gain.tick(param_val(params, "gain_db", 0.0) as f64);
        let g = db_to_lin(db);
        (left * g, right * g)
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxGain::new())
    }
}
