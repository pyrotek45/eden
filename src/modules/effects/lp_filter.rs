// Eden DAW — LP Filter effect

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

pub struct FxLpFilter {
    ic1_l: f64,
    ic2_l: f64,
    ic1_r: f64,
    ic2_r: f64,
    sm_cutoff: SmoothedParam,
    sm_reso: SmoothedParam,
    sm_output: SmoothedParam,
}
impl FxLpFilter {
    pub fn new() -> Self {
        Self {
            ic1_l: 0.0,
            ic2_l: 0.0,
            ic1_r: 0.0,
            ic2_r: 0.0,
            sm_cutoff: SmoothedParam::new(0.8, 44100.0),
            sm_reso: SmoothedParam::new(0.0, 44100.0),
            sm_output: SmoothedParam::new(0.0, 44100.0),
        }
    }
}

pub(crate) static LP_FILTER_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "cutoff",
        name: "Cutoff",
        default: 0.8,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    ParamDesc {
        id: "reso",
        name: "Resonance",
        default: 0.0,
        min: 0.0,
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

impl EffectModule for FxLpFilter {
    fn name(&self) -> &'static str {
        "LP Filter"
    }
    fn params(&self) -> &'static [ParamDesc] {
        LP_FILTER_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let cutoff_norm = self.sm_cutoff.tick(param_val(params, "cutoff", 0.8) as f64);
        let reso = self.sm_reso.tick(param_val(params, "reso", 0.0) as f64);
        let cutoff_hz = (20.0 * fast_pow2(cutoff_norm * 9.965784284662087)).clamp(20.0, sr * 0.49);
        let (lp_l, _, _) = svf_tick(left, cutoff_hz, reso, sr, &mut self.ic1_l, &mut self.ic2_l);
        let (lp_r, _, _) = svf_tick(right, cutoff_hz, reso, sr, &mut self.ic1_r, &mut self.ic2_r);
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);
        if out_db.abs() < 0.001 {
            (lp_l, lp_r)
        } else {
            let g = db_to_linear(out_db);
            (lp_l * g, lp_r * g)
        }
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxLpFilter::new())
    }
}
