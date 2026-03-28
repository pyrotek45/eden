// Eden DAW — Autoduck effect (tempo-synced volume ducking)

use crate::modules::dsp_primitives::*;
use crate::modules::{EffectModule, ParamDesc};

pub struct FxAutoduck {
    phase: f64,
    sm_duck: SmoothedParam,
    sm_attack: SmoothedParam,
    sm_hold: SmoothedParam,
    sm_release: SmoothedParam,
    sm_period: SmoothedParam,
    sm_shift: SmoothedParam,
    sm_curve: SmoothedParam,
    sm_output: SmoothedParam,
}

impl FxAutoduck {
    pub fn new() -> Self {
        let sr = 44100.0;
        Self {
            phase: 0.0,
            sm_duck: SmoothedParam::new(-12.0, sr),
            sm_attack: SmoothedParam::new(5.0, sr),
            sm_hold: SmoothedParam::new(50.0, sr),
            sm_release: SmoothedParam::new(100.0, sr),
            sm_period: SmoothedParam::new(500.0, sr),
            sm_shift: SmoothedParam::new(0.0, sr),
            sm_curve: SmoothedParam::new(50.0, sr),
            sm_output: SmoothedParam::new(0.0, sr),
        }
    }
}

pub(crate) static AUTODUCK_PARAMS: &[ParamDesc] = &[
    ParamDesc {
        id: "duck_db",
        name: "Duck",
        default: -12.0,
        min: -60.0,
        max: 0.0,
        options: None,
    },
    ParamDesc {
        id: "attack",
        name: "Attack",
        default: 5.0,
        min: 0.1,
        max: 200.0,
        options: None,
    },
    ParamDesc {
        id: "hold",
        name: "Hold",
        default: 50.0,
        min: 0.0,
        max: 500.0,
        options: None,
    },
    ParamDesc {
        id: "release",
        name: "Release",
        default: 100.0,
        min: 1.0,
        max: 1000.0,
        options: None,
    },
    ParamDesc {
        id: "period",
        name: "Period",
        default: 500.0,
        min: 50.0,
        max: 4000.0,
        options: None,
    },
    ParamDesc {
        id: "shift",
        name: "Shift",
        default: 0.0,
        min: 0.0,
        max: 100.0,
        options: None,
    },
    ParamDesc {
        id: "curve",
        name: "Curve",
        default: 50.0,
        min: 0.0,
        max: 100.0,
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

impl EffectModule for FxAutoduck {
    fn name(&self) -> &'static str {
        "Autoduck"
    }
    fn params(&self) -> &'static [ParamDesc] {
        AUTODUCK_PARAMS
    }
    fn process(&mut self, left: f64, right: f64, params: &[(String, f32)], sr: f64) -> (f64, f64) {
        let duck_db = self
            .sm_duck
            .tick(param_val(params, "duck_db", -12.0) as f64);
        let attack_ms = self
            .sm_attack
            .tick(param_val(params, "attack", 5.0) as f64)
            .max(0.1);
        let hold_ms = self
            .sm_hold
            .tick(param_val(params, "hold", 50.0) as f64)
            .max(0.0);
        let release_ms = self
            .sm_release
            .tick(param_val(params, "release", 100.0) as f64)
            .max(1.0);
        let period_ms = self
            .sm_period
            .tick(param_val(params, "period", 500.0) as f64)
            .max(1.0);
        let shift_pct = self.sm_shift.tick(param_val(params, "shift", 0.0) as f64);
        let curve_pct = self.sm_curve.tick(param_val(params, "curve", 50.0) as f64);
        let out_db = self
            .sm_output
            .tick(param_val(params, "output_db", 0.0) as f64);

        let phase_inc = 1000.0 / (period_ms * sr);
        self.phase += phase_inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let shifted = (self.phase + shift_pct / 100.0) % 1.0;
        let total_env_ms = attack_ms + hold_ms + release_ms;
        let env_fraction = (total_env_ms / period_ms).min(1.0);
        let attack_end = (attack_ms / period_ms).min(1.0);
        let hold_end = ((attack_ms + hold_ms) / period_ms).min(1.0);
        let release_end = env_fraction;

        let raw_env = if shifted < attack_end {
            if attack_end > 1e-9 {
                shifted / attack_end
            } else {
                1.0
            }
        } else if shifted < hold_end {
            1.0
        } else if shifted < release_end {
            let rel_phase = (shifted - hold_end) / (release_end - hold_end).max(1e-9);
            1.0 - rel_phase
        } else {
            0.0
        };

        let curve_norm = curve_pct / 100.0;
        let shaped = if (curve_norm - 0.5).abs() < 0.01 {
            raw_env
        } else if curve_norm < 0.5 {
            let exp = 1.0 + (0.5 - curve_norm) * 6.0;
            raw_env.powf(exp)
        } else {
            let exp = 1.0 / (1.0 + (curve_norm - 0.5) * 6.0);
            raw_env.powf(exp)
        };

        let duck_gain = db_to_lin(duck_db * shaped);
        let dl = left * duck_gain;
        let dr = right * duck_gain;
        if out_db.abs() < 0.001 {
            (dl, dr)
        } else {
            let g = db_to_lin(out_db);
            (dl * g, dr * g)
        }
    }
    fn reset(&mut self) {
        self.phase = 0.0;
    }
    fn fresh(&self) -> Box<dyn EffectModule> {
        Box::new(FxAutoduck::new())
    }
}
