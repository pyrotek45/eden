// Eden DAW — Named-instance effect chain helpers (realtime engine)
//
// The realtime audio engine stores effect instances alongside their names:
//   `Vec<(String, Box<dyn EffectModule>)>`
// so it can diff the current chain against the project snapshot and reuse
// existing DSP state.  These helpers operate on that representation so the
// sync / smooth / process logic is written once.

use crate::modules::{create_effect, EffectModule};

/// Sync a list of running named effect instances to match a desired slot list.
/// Reuses existing instances by name to preserve their DSP state (avoiding
/// audio glitches), only creating fresh instances for newly-added effects.
///
/// Used for **both** per-track effect chains and the master rack.
pub fn sync_named_effect_chain(
    running: &mut Vec<(String, Box<dyn EffectModule>)>,
    desired: &[(String, Vec<(String, f32)>)],
    sample_rate: u32,
) {
    let changed = desired.len() != running.len()
        || desired
            .iter()
            .zip(running.iter())
            .any(|((want, _), (have, _))| want != have);
    if !changed {
        return;
    }
    let mut new_fx: Vec<(String, Box<dyn EffectModule>)> = Vec::with_capacity(desired.len());
    for (fx_name, _) in desired {
        let idx = running
            .iter()
            .position(|(n, _)| n.as_str() == fx_name.as_str());
        if let Some(i) = idx {
            new_fx.push(running.remove(i));
        } else if let Some(m) = create_effect(fx_name, sample_rate) {
            new_fx.push((fx_name.to_string(), m));
        }
    }
    *running = new_fx;
}

/// Process a named effect chain for one sample (with sidechain support).
///
/// `input` — current (left, right) signal for this track.
/// `effects` — named effect instances: `&mut [(String, Box<dyn EffectModule>)]`.
/// `slot_params` — raw snapshot params per slot: `&[(String, Vec<(String, f32)>)]`.
/// `smoothed` — optional pre-smoothed param cache `&[Vec<(String, f32)>]`.
/// `sidechain_map` — per-slot sidechain source track index.
/// `per_track_sample` — all tracks' current signal (for sidechain lookup).
/// `bpm`, `sample_rate` — context.
///
/// Returns the processed (left, right) pair.
#[allow(clippy::too_many_arguments)]
pub fn process_named_effect_chain(
    mut signal: (f64, f64),
    effects: &mut [(String, Box<dyn EffectModule>)],
    slot_params: &[(String, Vec<(String, f32)>)],
    smoothed: Option<&[Vec<(String, f32)>]>,
    sidechain_map: &[Option<usize>],
    per_track_sample: &[(f64, f64)],
    bpm: f64,
    sample_rate: f64,
) -> (f64, f64) {
    for (fi, (_, fx)) in effects.iter_mut().enumerate() {
        fx.set_bpm(bpm);
        // Resolve sidechain source signal (default: self)
        let sc_ti = sidechain_map.get(fi).copied().flatten();
        let (key_l, key_r) = if let Some(sc_idx) = sc_ti {
            if sc_idx < per_track_sample.len() {
                per_track_sample[sc_idx]
            } else {
                signal
            }
        } else {
            signal
        };
        // Use smoothed params if available, else raw snapshot
        let params: &[(String, f32)] = if let Some(sm) = smoothed {
            if fi < sm.len() && fi < slot_params.len() && sm[fi].len() == slot_params[fi].1.len() {
                &sm[fi]
            } else if fi < slot_params.len() {
                &slot_params[fi].1
            } else {
                continue;
            }
        } else if fi < slot_params.len() {
            &slot_params[fi].1
        } else {
            continue;
        };
        let (ol, or) = fx.process_sidechain(signal.0, signal.1, key_l, key_r, params, sample_rate);
        signal = (ol, or);
    }
    signal
}

/// Process a CStrip2 channel strip for one sample.
///
/// `input` — current (left, right) signal.
/// `cstrip` — CStrip2 effect instance.
/// `raw_params` — raw CStrip2 params from snapshot.
/// `smoothed` — optional pre-smoothed params.
/// `bypass` — whether the CStrip2 is bypassed.
/// `sample_rate` — audio sample rate.
///
/// Returns the processed (left, right) pair.
pub fn process_cstrip(
    input: (f64, f64),
    cstrip: &mut dyn EffectModule,
    raw_params: &[(String, f32)],
    smoothed: Option<&[(String, f32)]>,
    bypass: bool,
    sample_rate: f64,
) -> (f64, f64) {
    if bypass || raw_params.is_empty() {
        return input;
    }
    let params = if let Some(sm) = smoothed {
        if sm.len() == raw_params.len() {
            sm
        } else {
            raw_params
        }
    } else {
        raw_params
    };
    cstrip.process(input.0, input.1, params, sample_rate)
}

/// Process the master rack effect chain for one sample (no sidechain — key = self).
///
/// `mix` — current stereo mix (left, right).
/// `effects` — named master effect instances.
/// `slot_params` — raw snapshot params per master slot.
/// `smoothed` — optional pre-smoothed param cache.
/// `bpm`, `sample_rate` — context.
///
/// Returns the processed (left, right) pair.
pub fn process_named_master_effects(
    mut mix: (f64, f64),
    effects: &mut [(String, Box<dyn EffectModule>)],
    slot_params: &[(String, Vec<(String, f32)>)],
    smoothed: Option<&[Vec<(String, f32)>]>,
    bpm: f64,
    sample_rate: f64,
) -> (f64, f64) {
    for (fi, (_, fx)) in effects.iter_mut().enumerate() {
        fx.set_bpm(bpm);
        let params: &[(String, f32)] = if let Some(sm) = smoothed {
            if fi < sm.len() && fi < slot_params.len() && sm[fi].len() == slot_params[fi].1.len() {
                &sm[fi]
            } else if fi < slot_params.len() {
                &slot_params[fi].1
            } else {
                continue;
            }
        } else if fi < slot_params.len() {
            &slot_params[fi].1
        } else {
            continue;
        };
        let (ml, mr) = fx.process_sidechain(mix.0, mix.1, mix.0, mix.1, params, sample_rate);
        mix = (ml, mr);
    }
    mix
}
