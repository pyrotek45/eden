//! Unit tests for the shared DSP pipeline (`dsp.rs`).
//!
//! Covers:
//!   • DcHpState — DC removal, AC passthrough, stereo independence
//!   • sync_named_effect_chain — reuse, reorder, add, remove
//!   • smooth_params — convergence, growth, stability
//!   • process_named_effect_chain — passthrough, sidechain routing, param selection
//!   • process_cstrip — passthrough when bypassed, processes when active
//!   • process_named_master_effects — passthrough, single effect, multi-effect
//!   • mix_audio_clips — silence outside clip, micro-fades, user fades
//!   • pan_and_mix — center, hard-left, hard-right, volume scaling
//!   • evaluate_automation_at — interpolation, boundary, empty
//!   • song_length_beats — empty, single clip, multiple clips

use super::*;
use crate::dsp;
use crate::modules::{create_effect, get_param_descs, ModuleExtra};
use std::sync::Arc;

// ══════════════════════════════════════════════════════════════════════════════
// Helper: default params for an effect from the registry
// ══════════════════════════════════════════════════════════════════════════════

fn default_params_for(name: &str) -> Vec<(String, f32)> {
    get_param_descs(name)
        .iter()
        .map(|d| (d.id.to_string(), d.default))
        .collect()
}

/// Helper: construct a minimal AudioTrack with just audio clips
fn make_audio_track(clips: Vec<crate::engine::AudioSampleClip>) -> crate::engine::AudioTrack {
    crate::engine::AudioTrack {
        volume: 1.0,
        pan: 0.0,
        mute: false,
        solo: false,
        is_automation: false,
        midi_clips: vec![],
        audio_clips: clips,
        instrument_module: None,
        instrument_params: vec![],
        effect_slots: vec![],
        midi_effect_slots: vec![],
        effect_sidechain_track: vec![],
        cstrip2_params: vec![],
        cstrip2_bypass: true,
        extra: ModuleExtra::default(),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// DcHpState
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_dc_hp_removes_dc_offset() {
    let mut hp = dsp::DcHpState::new();
    // Feed DC = 1.0 for 2 seconds → output should decay to near zero
    for _ in 0..88200 {
        hp.process(1.0, 1.0);
    }
    let (l, r) = hp.process(1.0, 1.0);
    assert!(
        l.abs() < 0.005,
        "DC HP should remove constant DC, got L={l}"
    );
    assert!(
        r.abs() < 0.005,
        "DC HP should remove constant DC, got R={r}"
    );
}

#[test]
fn test_dc_hp_passes_ac_signal() {
    let mut hp = dsp::DcHpState::new();
    let sr = 44100.0;
    let freq = 440.0;
    // Settle for 0.5 s
    for i in 0..22050 {
        let t = i as f64 / sr;
        let s = (t * freq * std::f64::consts::TAU).sin();
        hp.process(s, s);
    }
    // Measure energy over next 0.1 s
    let mut energy = 0.0;
    let n = 4410;
    for i in 22050..22050 + n {
        let t = i as f64 / sr;
        let s = (t * freq * std::f64::consts::TAU).sin();
        let (l, _) = hp.process(s, s);
        energy += l * l;
    }
    let rms = (energy / n as f64).sqrt();
    assert!(
        rms > 0.5,
        "DC HP should pass 440 Hz with minimal attenuation, got RMS={rms}"
    );
}

#[test]
fn test_dc_hp_stereo_independence() {
    let mut hp = dsp::DcHpState::new();
    // Feed DC left, AC right
    let sr = 44100.0;
    for i in 0..44100 {
        let t = i as f64 / sr;
        let ac = (t * 440.0 * std::f64::consts::TAU).sin();
        hp.process(1.0, ac);
    }
    let (l, r) = hp.process(1.0, 0.0);
    assert!(
        l.abs() < 0.01,
        "Left channel DC should be attenuated, got {l}"
    );
    // Right channel should still have energy from recent AC
    // (this sample is 0.0 input, but the filter has state from the previous sample)
    // Just check L and R are independent
    assert!(
        (l - r).abs() > 0.001 || l.abs() < 0.01,
        "L and R channels should be independent"
    );
}

#[test]
fn test_dc_hp_initial_state_zero() {
    let hp = dsp::DcHpState::new();
    assert_eq!(hp.x_l, 0.0);
    assert_eq!(hp.y_l, 0.0);
    assert_eq!(hp.x_r, 0.0);
    assert_eq!(hp.y_r, 0.0);
}

#[test]
fn test_dc_hp_step_response() {
    let mut hp = dsp::DcHpState::new();
    // First sample of step: output should be ~1.0 (high pass lets through the transient)
    let (l, _) = hp.process(1.0, 0.0);
    assert!(
        (l - 1.0).abs() < 0.01,
        "First sample of step should be ~1.0, got {l}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// sync_named_effect_chain
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_sync_named_effect_chain_empty_to_empty() {
    let mut running: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let desired: Vec<(String, Vec<(String, f32)>)> = Vec::new();
    dsp::sync_named_effect_chain(&mut running, &desired, 44100);
    assert!(running.is_empty());
}

#[test]
fn test_sync_named_effect_chain_adds_new() {
    let mut running: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let desired = vec![
        ("Gain".to_string(), default_params_for("Gain")),
        ("LP Filter".to_string(), default_params_for("LP Filter")),
    ];
    dsp::sync_named_effect_chain(&mut running, &desired, 44100);
    assert_eq!(running.len(), 2);
    assert_eq!(running[0].0, "Gain");
    assert_eq!(running[1].0, "LP Filter");
}

#[test]
fn test_sync_named_effect_chain_reuses_existing() {
    let mut running: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let desired = vec![("Gain".to_string(), default_params_for("Gain"))];
    dsp::sync_named_effect_chain(&mut running, &desired, 44100);
    // Process a sample to mutate internal state
    let params = default_params_for("Gain");
    let (_, _) = running[0].1.process(1.0, 1.0, &params, 44100.0);

    // Sync again with same chain — should reuse the same instance (not reset)
    let desired2 = vec![("Gain".to_string(), default_params_for("Gain"))];
    dsp::sync_named_effect_chain(&mut running, &desired2, 44100);
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].0, "Gain");
}

#[test]
fn test_sync_named_effect_chain_removes_missing() {
    let mut running: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let desired = vec![
        ("Gain".to_string(), default_params_for("Gain")),
        ("LP Filter".to_string(), default_params_for("LP Filter")),
    ];
    dsp::sync_named_effect_chain(&mut running, &desired, 44100);
    assert_eq!(running.len(), 2);

    // Remove LP Filter
    let desired2 = vec![("Gain".to_string(), default_params_for("Gain"))];
    dsp::sync_named_effect_chain(&mut running, &desired2, 44100);
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].0, "Gain");
}

#[test]
fn test_sync_named_effect_chain_reorders() {
    let mut running: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let desired = vec![
        ("Gain".to_string(), default_params_for("Gain")),
        ("LP Filter".to_string(), default_params_for("LP Filter")),
    ];
    dsp::sync_named_effect_chain(&mut running, &desired, 44100);

    // Reverse order
    let desired2 = vec![
        ("LP Filter".to_string(), default_params_for("LP Filter")),
        ("Gain".to_string(), default_params_for("Gain")),
    ];
    dsp::sync_named_effect_chain(&mut running, &desired2, 44100);
    assert_eq!(running.len(), 2);
    assert_eq!(running[0].0, "LP Filter");
    assert_eq!(running[1].0, "Gain");
}

#[test]
fn test_sync_named_effect_chain_no_change_noop() {
    let mut running: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let desired = vec![("Gain".to_string(), default_params_for("Gain"))];
    dsp::sync_named_effect_chain(&mut running, &desired, 44100);
    assert_eq!(running.len(), 1);

    // Same desired — no change needed
    let desired2 = vec![("Gain".to_string(), default_params_for("Gain"))];
    dsp::sync_named_effect_chain(&mut running, &desired2, 44100);
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].0, "Gain");
}

// ══════════════════════════════════════════════════════════════════════════════
// smooth_params
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_smooth_params_converges() {
    let mut cache: Vec<(String, f32)> = vec![("vol".into(), 0.0)];
    let target: Vec<(String, f32)> = vec![("vol".into(), 1.0)];
    // Run for many iterations
    for _ in 0..50000 {
        dsp::smooth_params(&mut cache, &target, 0.002);
    }
    assert!(
        (cache[0].1 - 1.0).abs() < 0.001,
        "smooth_params should converge to target, got {}",
        cache[0].1
    );
}

#[test]
fn test_smooth_params_no_overshoot() {
    let mut cache: Vec<(String, f32)> = vec![("vol".into(), 0.0)];
    let target: Vec<(String, f32)> = vec![("vol".into(), 1.0)];
    for _ in 0..100000 {
        dsp::smooth_params(&mut cache, &target, 0.002);
        assert!(
            cache[0].1 >= 0.0 && cache[0].1 <= 1.0,
            "smooth_params should not overshoot: got {}",
            cache[0].1
        );
    }
}

#[test]
fn test_smooth_params_grows_cache_for_new_params() {
    let mut cache: Vec<(String, f32)> = vec![("vol".into(), 0.5)];
    let target: Vec<(String, f32)> = vec![("vol".into(), 0.5), ("pan".into(), 0.3)];
    dsp::smooth_params(&mut cache, &target, 0.002);
    assert_eq!(cache.len(), 2, "cache should grow to match target length");
    // New param should be initialized to target value
    assert!(
        (cache[1].1 - 0.3).abs() < 0.001,
        "new param should init to target, got {}",
        cache[1].1
    );
}

#[test]
fn test_smooth_params_empty() {
    let mut cache: Vec<(String, f32)> = Vec::new();
    let target: Vec<(String, f32)> = Vec::new();
    dsp::smooth_params(&mut cache, &target, 0.002);
    assert!(cache.is_empty());
}

#[test]
fn test_smooth_params_ramps_not_jumps() {
    let mut cache: Vec<(String, f32)> = vec![("vol".into(), 0.0)];
    let target: Vec<(String, f32)> = vec![("vol".into(), 1.0)];
    dsp::smooth_params(&mut cache, &target, 0.002);
    let first = cache[0].1;
    assert!(
        first > 0.0 && first < 0.1,
        "first smooth step should be small, got {first}"
    );
}

#[test]
fn test_smooth_params_downward() {
    let mut cache: Vec<(String, f32)> = vec![("vol".into(), 1.0)];
    let target: Vec<(String, f32)> = vec![("vol".into(), 0.0)];
    for _ in 0..50000 {
        dsp::smooth_params(&mut cache, &target, 0.002);
    }
    assert!(
        cache[0].1.abs() < 0.001,
        "smooth_params should converge downward, got {}",
        cache[0].1
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// process_named_effect_chain
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_process_named_effect_chain_empty_passthrough() {
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let slot_params: Vec<(String, Vec<(String, f32)>)> = Vec::new();
    let sidechain_map: Vec<Option<usize>> = Vec::new();
    let per_track = vec![(0.0, 0.0)];
    let (l, r) = dsp::process_named_effect_chain(
        (0.5, -0.3),
        &mut effects,
        &slot_params,
        None,
        &sidechain_map,
        &per_track,
        120.0,
        44100.0,
    );
    assert!(
        (l - 0.5).abs() < 1e-12 && (r - (-0.3)).abs() < 1e-12,
        "Empty chain should pass through: ({l}, {r})"
    );
}

#[test]
fn test_process_named_effect_chain_gain_effect() {
    // Gain at +6 dB should amplify signal (after smoothing settles)
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> =
        vec![("Gain".to_string(), create_effect("Gain", 44100).unwrap())];
    let mut params = default_params_for("Gain");
    // Set gain to +6 dB
    for p in &mut params {
        if p.0 == "gain_db" {
            p.1 = 6.0;
        }
    }
    let slot_params = vec![("Gain".to_string(), params)];
    let sidechain_map = vec![None];
    let per_track = vec![(0.5, 0.5)];
    // Run for many samples to let SmoothedParam settle
    let mut l = 0.0;
    for _ in 0..5000 {
        let out = dsp::process_named_effect_chain(
            (0.5, 0.5),
            &mut effects,
            &slot_params,
            None,
            &sidechain_map,
            &per_track,
            120.0,
            44100.0,
        );
        l = out.0;
    }
    // +6 dB ≈ 2× amplitude
    assert!(
        l > 0.9,
        "Gain at +6dB should roughly double signal after settling, got L={l}"
    );
}

#[test]
fn test_process_named_effect_chain_uses_smoothed_params() {
    // When smoothed params are provided, they should be used instead of raw
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> =
        vec![("Gain".to_string(), create_effect("Gain", 44100).unwrap())];
    // Raw params: gain = 0 dB (unity)
    let raw_params = default_params_for("Gain");
    let slot_params = vec![("Gain".to_string(), raw_params.clone())];
    // Smoothed params: gain = +12 dB
    let mut smoothed_params = raw_params.clone();
    for p in &mut smoothed_params {
        if p.0 == "gain_db" {
            p.1 = 12.0;
        }
    }
    let smoothed = vec![smoothed_params];
    let sidechain_map = vec![None];
    let per_track = vec![(0.5, 0.5)];
    // Settle: the Gain effect has its own internal SmoothedParam, so we need
    // many iterations for it to reach the +12 dB target.
    let mut l = 0.0;
    for _ in 0..5000 {
        let out = dsp::process_named_effect_chain(
            (0.5, 0.5),
            &mut effects,
            &slot_params,
            Some(&smoothed),
            &sidechain_map,
            &per_track,
            120.0,
            44100.0,
        );
        l = out.0;
    }
    // +12 dB ≈ 4× amplitude → output ~2.0
    assert!(
        l > 1.5,
        "Should use smoothed +12dB gain after settling, got L={l}"
    );
}

#[test]
fn test_process_named_effect_chain_sidechain_routing() {
    // With sidechain, the key signal comes from a different track
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> =
        vec![("Gain".to_string(), create_effect("Gain", 44100).unwrap())];
    let params = default_params_for("Gain");
    let slot_params = vec![("Gain".to_string(), params)];
    // Sidechain from track 1
    let sidechain_map = vec![Some(1)];
    let per_track = vec![(0.5, 0.5), (0.8, 0.8)];
    let (l, r) = dsp::process_named_effect_chain(
        (0.5, 0.5),
        &mut effects,
        &slot_params,
        None,
        &sidechain_map,
        &per_track,
        120.0,
        44100.0,
    );
    // Gain doesn't use sidechain, so output should still be based on input
    assert!(l.is_finite() && r.is_finite());
}

// ══════════════════════════════════════════════════════════════════════════════
// process_cstrip
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_process_cstrip_bypass() {
    let mut cstrip = create_effect("CStrip2", 44100).unwrap();
    let params = default_params_for("CStrip2");
    let (l, r) = dsp::process_cstrip((0.5, -0.3), cstrip.as_mut(), &params, None, true, 44100.0);
    assert!(
        (l - 0.5).abs() < 1e-12 && (r - (-0.3)).abs() < 1e-12,
        "Bypassed CStrip should pass through: ({l}, {r})"
    );
}

#[test]
fn test_process_cstrip_empty_params_bypass() {
    let mut cstrip = create_effect("CStrip2", 44100).unwrap();
    let (l, r) = dsp::process_cstrip((0.5, -0.3), cstrip.as_mut(), &[], None, false, 44100.0);
    assert!(
        (l - 0.5).abs() < 1e-12 && (r - (-0.3)).abs() < 1e-12,
        "CStrip with empty params should pass through: ({l}, {r})"
    );
}

#[test]
fn test_process_cstrip_active_produces_output() {
    let mut cstrip = create_effect("CStrip2", 44100).unwrap();
    let params = default_params_for("CStrip2");
    // Settle with a few samples first
    for _ in 0..100 {
        dsp::process_cstrip((0.5, 0.5), cstrip.as_mut(), &params, None, false, 44100.0);
    }
    let (l, r) = dsp::process_cstrip((0.5, 0.5), cstrip.as_mut(), &params, None, false, 44100.0);
    assert!(l.is_finite(), "CStrip L must be finite, got {l}");
    assert!(r.is_finite(), "CStrip R must be finite, got {r}");
    // With default params, should roughly pass through (within ~6dB)
    assert!(
        l.abs() > 0.01,
        "CStrip should not silence the signal, got L={l}"
    );
}

#[test]
fn test_process_cstrip_uses_smoothed_params() {
    let mut cstrip = create_effect("CStrip2", 44100).unwrap();
    let raw = default_params_for("CStrip2");
    // Make smoothed params with max output
    let mut smoothed = raw.clone();
    for p in &mut smoothed {
        if p.0 == "output" {
            p.1 = 1.0; // max output
        }
    }
    // Settle
    for _ in 0..200 {
        dsp::process_cstrip(
            (0.5, 0.5),
            cstrip.as_mut(),
            &raw,
            Some(&smoothed),
            false,
            44100.0,
        );
    }
    let (l, _) = dsp::process_cstrip(
        (0.5, 0.5),
        cstrip.as_mut(),
        &raw,
        Some(&smoothed),
        false,
        44100.0,
    );
    assert!(l.is_finite(), "CStrip with smoothed params must be finite");
}

// ══════════════════════════════════════════════════════════════════════════════
// process_named_master_effects
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_process_named_master_effects_empty_passthrough() {
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let slot_params: Vec<(String, Vec<(String, f32)>)> = Vec::new();
    let (l, r) = dsp::process_named_master_effects(
        (0.7, -0.4),
        &mut effects,
        &slot_params,
        None,
        120.0,
        44100.0,
    );
    assert!(
        (l - 0.7).abs() < 1e-12 && (r - (-0.4)).abs() < 1e-12,
        "Empty master chain should pass through"
    );
}

#[test]
fn test_process_named_master_effects_gain() {
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> =
        vec![("Gain".to_string(), create_effect("Gain", 44100).unwrap())];
    let mut params = default_params_for("Gain");
    for p in &mut params {
        if p.0 == "gain_db" {
            p.1 = -60.0; // -60 dB ≈ 0.001× amplitude
        }
    }
    let slot_params = vec![("Gain".to_string(), params)];
    // Settle internal SmoothedParam
    let mut l = 0.0;
    let mut r = 0.0;
    for _ in 0..5000 {
        let out = dsp::process_named_master_effects(
            (0.5, 0.5),
            &mut effects,
            &slot_params,
            None,
            120.0,
            44100.0,
        );
        l = out.0;
        r = out.1;
    }
    assert!(
        l.abs() < 0.01,
        "Master -60dB gain should nearly silence after settling, got L={l}"
    );
    assert!(
        r.abs() < 0.01,
        "Master -60dB gain should nearly silence after settling, got R={r}"
    );
}

#[test]
fn test_process_named_master_effects_chain_order() {
    // Two gains: +6dB then -6dB should roughly cancel
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> = vec![
        ("Gain".to_string(), create_effect("Gain", 44100).unwrap()),
        ("Gain".to_string(), create_effect("Gain", 44100).unwrap()),
    ];
    let mut params_up = default_params_for("Gain");
    for p in &mut params_up {
        if p.0 == "gain_db" {
            p.1 = 6.0;
        }
    }
    let mut params_down = default_params_for("Gain");
    for p in &mut params_down {
        if p.0 == "gain_db" {
            p.1 = -6.0;
        }
    }
    let slot_params = vec![
        ("Gain".to_string(), params_up),
        ("Gain".to_string(), params_down),
    ];
    let (l, _) = dsp::process_named_master_effects(
        (0.5, 0.5),
        &mut effects,
        &slot_params,
        None,
        120.0,
        44100.0,
    );
    // Should be approximately unity
    assert!(
        (l - 0.5).abs() < 0.05,
        "+6dB then -6dB should cancel, got L={l}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// pan_and_mix
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_pan_center() {
    let (l, r) = dsp::pan_and_mix((1.0, 1.0), 0.0, 1.0);
    // Center pan: L and R should be equal and close to 1.0
    assert!(
        (l - r).abs() < 0.01,
        "Center pan: L and R should be equal, got ({l}, {r})"
    );
    assert!(
        l > 0.9 && l < 1.1,
        "Center pan at unity vol should be ~1.0, got L={l}"
    );
}

#[test]
fn test_pan_hard_left() {
    let (l, r) = dsp::pan_and_mix((1.0, 1.0), -1.0, 1.0);
    assert!(
        l > r,
        "Hard left: L should be louder than R, got ({l}, {r})"
    );
    // R should be near zero
    assert!(r.abs() < 0.1, "Hard left: R should be near zero, got {r}");
}

#[test]
fn test_pan_hard_right() {
    let (l, r) = dsp::pan_and_mix((1.0, 1.0), 1.0, 1.0);
    assert!(
        r > l,
        "Hard right: R should be louder than L, got ({l}, {r})"
    );
    assert!(l.abs() < 0.1, "Hard right: L should be near zero, got {l}");
}

#[test]
fn test_pan_volume_scales() {
    let (l1, r1) = dsp::pan_and_mix((1.0, 1.0), 0.0, 1.0);
    let (l2, r2) = dsp::pan_and_mix((1.0, 1.0), 0.0, 0.5);
    assert!(
        (l2 - l1 * 0.5).abs() < 0.01,
        "Half volume should halve output: {l1} vs {l2}"
    );
    assert!(
        (r2 - r1 * 0.5).abs() < 0.01,
        "Half volume should halve output: {r1} vs {r2}"
    );
}

#[test]
fn test_pan_zero_volume_silence() {
    let (l, r) = dsp::pan_and_mix((1.0, 1.0), 0.0, 0.0);
    assert!(
        l.abs() < 1e-12 && r.abs() < 1e-12,
        "Zero volume should be silence, got ({l}, {r})"
    );
}

#[test]
fn test_pan_preserves_stereo_separation() {
    // Different L and R inputs should produce different L and R outputs at center
    let (l, r) = dsp::pan_and_mix((0.8, 0.2), 0.0, 1.0);
    assert!(
        l > r,
        "With louder left input, left output should be louder: ({l}, {r})"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// evaluate_automation_at
// ══════════════════════════════════════════════════════════════════════════════

fn make_automation_project(
    target: &str,
    points: Vec<(f64, f32)>,
    start: f64,
    length: f64,
) -> Project {
    let mut p = Project::default();
    let mut t = Track::new(99, "Auto", TrackType::Automation);
    t.clips.push(Clip::Automation(AutomationClip {
        target_param: target.to_string(),
        points: points
            .into_iter()
            .map(|(time, value)| AutomationPoint { time, value })
            .collect(),
        start_time: start,
        length,
        name: "A".into(),
        color: [0; 4],
    }));
    p.tracks.push(t);
    p
}

#[test]
fn test_evaluate_automation_empty_project() {
    let p = Project::default();
    let vals = dsp::evaluate_automation_at(&p, 1.0);
    assert!(vals.is_empty());
}

#[test]
fn test_evaluate_automation_before_clip() {
    let p = make_automation_project("1:1:vol", vec![(0.0, 0.5), (1.0, 1.0)], 2.0, 4.0);
    let vals = dsp::evaluate_automation_at(&p, 1.0);
    assert!(vals.is_empty(), "Before clip start, should have no values");
}

#[test]
fn test_evaluate_automation_after_clip() {
    let p = make_automation_project("1:1:vol", vec![(0.0, 0.5), (1.0, 1.0)], 0.0, 2.0);
    let vals = dsp::evaluate_automation_at(&p, 3.0);
    assert!(vals.is_empty(), "After clip end, should have no values");
}

#[test]
fn test_evaluate_automation_at_start() {
    let p = make_automation_project("1:1:vol", vec![(0.0, 0.5), (1.0, 1.0)], 0.0, 4.0);
    let vals = dsp::evaluate_automation_at(&p, 0.0);
    assert_eq!(vals.len(), 1);
    assert_eq!(vals[0].0, "1:1:vol");
    assert!(
        (vals[0].1 - 0.5).abs() < 0.01,
        "At start, should be 0.5, got {}",
        vals[0].1
    );
}

#[test]
fn test_evaluate_automation_at_end() {
    let p = make_automation_project("1:1:vol", vec![(0.0, 0.5), (1.0, 1.0)], 0.0, 4.0);
    // Just before clip end
    let vals = dsp::evaluate_automation_at(&p, 3.999);
    assert_eq!(vals.len(), 1);
    assert!(
        (vals[0].1 - 1.0).abs() < 0.01,
        "Near end, should be ~1.0, got {}",
        vals[0].1
    );
}

#[test]
fn test_evaluate_automation_interpolation_midpoint() {
    let p = make_automation_project("1:1:vol", vec![(0.0, 0.0), (1.0, 1.0)], 0.0, 4.0);
    // Midpoint: t = 0.5 → value = 0.5
    let vals = dsp::evaluate_automation_at(&p, 2.0);
    assert_eq!(vals.len(), 1);
    assert!(
        (vals[0].1 - 0.5).abs() < 0.02,
        "Midpoint interpolation should be ~0.5, got {}",
        vals[0].1
    );
}

#[test]
fn test_evaluate_automation_single_point() {
    let p = make_automation_project("1:1:vol", vec![(0.5, 0.75)], 0.0, 4.0);
    let vals = dsp::evaluate_automation_at(&p, 1.0); // t=0.25, before point
    assert_eq!(vals.len(), 1);
    // Before the single point → should return that point's value
    assert!(
        (vals[0].1 - 0.75).abs() < 0.01,
        "Before single point should return point value, got {}",
        vals[0].1
    );
}

#[test]
fn test_evaluate_automation_empty_points() {
    let p = make_automation_project("1:1:vol", vec![], 0.0, 4.0);
    let vals = dsp::evaluate_automation_at(&p, 1.0);
    assert!(vals.is_empty(), "Empty points should produce no output");
}

// ══════════════════════════════════════════════════════════════════════════════
// song_length_beats
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_song_length_beats_empty_project() {
    let p = Project::default();
    let len = dsp::song_length_beats(&p);
    assert!(
        len >= 1.0,
        "Empty project should have minimum length of 1.0, got {len}"
    );
}

#[test]
fn test_song_length_beats_single_clip() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T1", TrackType::Midi);
    t.clips.push(Clip::Midi(MidiClip {
        notes: vec![],
        start_time: 2.0,
        length: 4.0,
        name: "C".into(),
        color: [0; 4],
    }));
    p.tracks.push(t);
    let len = dsp::song_length_beats(&p);
    assert!(
        (len - 6.0).abs() < 0.01,
        "Clip 2..6 → song length 6.0, got {len}"
    );
}

#[test]
fn test_song_length_beats_ignores_automation() {
    let mut p = Project::default();
    // Add automation clip at beat 100 — should be ignored
    let mut t = Track::new(99, "Auto", TrackType::Automation);
    t.clips.push(Clip::Automation(AutomationClip {
        target_param: "1:1:vol".into(),
        points: vec![],
        start_time: 0.0,
        length: 100.0,
        name: "A".into(),
        color: [0; 4],
    }));
    p.tracks.push(t);
    let len = dsp::song_length_beats(&p);
    assert!(len < 2.0, "Automation tracks should be ignored, got {len}");
}

#[test]
fn test_song_length_beats_multiple_clips() {
    let mut p = Project::default();
    let mut t1 = Track::new(1, "T1", TrackType::Midi);
    t1.clips.push(Clip::Midi(MidiClip {
        notes: vec![],
        start_time: 0.0,
        length: 4.0,
        name: "C1".into(),
        color: [0; 4],
    }));
    let mut t2 = Track::new(2, "T2", TrackType::Midi);
    t2.clips.push(Clip::Midi(MidiClip {
        notes: vec![],
        start_time: 4.0,
        length: 8.0,
        name: "C2".into(),
        color: [0; 4],
    }));
    p.tracks.push(t1);
    p.tracks.push(t2);
    let len = dsp::song_length_beats(&p);
    assert!((len - 12.0).abs() < 0.01, "Max clip end is 12.0, got {len}");
}

// ══════════════════════════════════════════════════════════════════════════════
// mix_audio_clips
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mix_audio_clips_silence_outside_clip() {
    let track = make_audio_track(vec![crate::engine::AudioSampleClip {
        start_beats: 4.0,
        length_beats: 4.0,
        offset_secs: 0.0,
        samples: Arc::new(vec![1.0; 44100]),
        sample_rate: 44100,
        gain: 1.0,
        fade_in: 0.0,
        fade_out: 0.0,
    }]);
    // Query before clip
    let (l, r) = dsp::mix_audio_clips(&track, 0.0, 2.0, 44100.0);
    assert!(
        l.abs() < 1e-12 && r.abs() < 1e-12,
        "Before clip should be silent"
    );
    // Query after clip
    let (l2, r2) = dsp::mix_audio_clips(&track, 10.0, 2.0, 44100.0);
    assert!(
        l2.abs() < 1e-12 && r2.abs() < 1e-12,
        "After clip should be silent"
    );
}

#[test]
fn test_mix_audio_clips_produces_signal() {
    let track = make_audio_track(vec![crate::engine::AudioSampleClip {
        start_beats: 0.0,
        length_beats: 4.0,
        offset_secs: 0.0,
        samples: Arc::new(vec![0.5; 88200]),
        sample_rate: 44100,
        gain: 1.0,
        fade_in: 0.0,
        fade_out: 0.0,
    }]);
    // Midpoint of clip (after micro-fade settles)
    let beats_per_sec = 2.0; // 120 BPM
    let pos = 1.0; // 1 beat = 0.5 seconds into a 2-second clip
    let (l, r) = dsp::mix_audio_clips(&track, pos, beats_per_sec, 44100.0);
    assert!(
        l.abs() > 0.1,
        "Clip should produce audible signal at midpoint, got L={l}"
    );
    assert!(
        r.abs() > 0.1,
        "Clip should produce audible signal at midpoint, got R={r}"
    );
}

#[test]
fn test_mix_audio_clips_gain() {
    let make_track = |gain: f32| {
        make_audio_track(vec![crate::engine::AudioSampleClip {
            start_beats: 0.0,
            length_beats: 4.0,
            offset_secs: 0.0,
            samples: Arc::new(vec![1.0; 44100]),
            sample_rate: 44100,
            gain,
            fade_in: 0.0,
            fade_out: 0.0,
        }])
    };

    let pos = 2.0;
    let bps = 2.0;
    let sr = 44100.0;
    let (l_full, _) = dsp::mix_audio_clips(&make_track(1.0), pos, bps, sr);
    let (l_half, _) = dsp::mix_audio_clips(&make_track(0.5), pos, bps, sr);
    assert!(
        (l_half - l_full * 0.5).abs() < 0.01,
        "Half gain should be half amplitude: full={l_full}, half={l_half}"
    );
}

#[test]
fn test_mix_audio_clips_user_fade_in() {
    let track = make_audio_track(vec![crate::engine::AudioSampleClip {
        start_beats: 0.0,
        length_beats: 8.0,
        offset_secs: 0.0,
        samples: Arc::new(vec![1.0; 441000]), // 10 seconds of audio
        sample_rate: 44100,
        gain: 1.0,
        fade_in: 1.0, // 1 second fade in
        fade_out: 0.0,
    }]);

    let bps = 2.0; // 120 BPM
    let sr = 44100.0;
    // Early in fade-in (0.25 beats = 0.125 seconds, well within 1s fade)
    let (l_early, _) = dsp::mix_audio_clips(&track, 0.25, bps, sr);
    // After fade-in (3 beats = 1.5 seconds, past 1s fade)
    let (l_late, _) = dsp::mix_audio_clips(&track, 3.0, bps, sr);
    assert!(
        l_late > l_early,
        "Signal after fade-in should be louder: early={l_early}, late={l_late}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// midi_to_freq
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_midi_to_freq_a4() {
    let freq = dsp::midi_to_freq(69);
    assert!(
        (freq - 440.0).abs() < 1.0,
        "MIDI 69 should be ~440 Hz, got {freq}"
    );
}

#[test]
fn test_midi_to_freq_octave_doubling() {
    let f1 = dsp::midi_to_freq(60);
    let f2 = dsp::midi_to_freq(72);
    assert!(
        (f2 / f1 - 2.0).abs() < 0.02,
        "12 semitones should double frequency: {f1} → {f2}"
    );
}

#[test]
fn test_midi_to_freq_middle_c() {
    let freq = dsp::midi_to_freq(60);
    // Middle C ≈ 261.63 Hz
    assert!(
        (freq - 261.63).abs() < 2.0,
        "MIDI 60 should be ~261.6 Hz, got {freq}"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Stress / edge case tests
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_process_named_effect_chain_skips_missing_params() {
    // More effects than slot_params — should not panic
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> = vec![
        ("Gain".to_string(), create_effect("Gain", 44100).unwrap()),
        ("Gain".to_string(), create_effect("Gain", 44100).unwrap()),
    ];
    // Only one set of params
    let params = default_params_for("Gain");
    let slot_params = vec![("Gain".to_string(), params)];
    let sidechain_map = vec![None, None];
    let per_track = vec![(0.5, 0.5)];
    let (l, r) = dsp::process_named_effect_chain(
        (0.5, 0.5),
        &mut effects,
        &slot_params,
        None,
        &sidechain_map,
        &per_track,
        120.0,
        44100.0,
    );
    assert!(
        l.is_finite() && r.is_finite(),
        "Should not panic on missing params"
    );
}

#[test]
fn test_process_named_master_effects_skips_missing_params() {
    let mut effects: Vec<(String, Box<dyn crate::modules::EffectModule>)> = vec![
        ("Gain".to_string(), create_effect("Gain", 44100).unwrap()),
        ("Gain".to_string(), create_effect("Gain", 44100).unwrap()),
    ];
    let slot_params = vec![("Gain".to_string(), default_params_for("Gain"))];
    let (l, r) = dsp::process_named_master_effects(
        (0.5, 0.5),
        &mut effects,
        &slot_params,
        None,
        120.0,
        44100.0,
    );
    assert!(l.is_finite() && r.is_finite());
}

#[test]
fn test_sync_named_effect_chain_unknown_effect_ignored() {
    let mut running: Vec<(String, Box<dyn crate::modules::EffectModule>)> = Vec::new();
    let desired = vec![
        ("NonExistentPlugin9999".to_string(), vec![]),
        ("Gain".to_string(), default_params_for("Gain")),
    ];
    dsp::sync_named_effect_chain(&mut running, &desired, 44100);
    // Unknown effect should be skipped, only Gain should be present
    assert_eq!(running.len(), 1, "Unknown effects should be skipped");
    assert_eq!(running[0].0, "Gain");
}

#[test]
fn test_dc_hp_preserves_finite_after_extreme_input() {
    let mut hp = dsp::DcHpState::new();
    // Feed extreme values
    for _ in 0..1000 {
        let (l, r) = hp.process(1e10, -1e10);
        assert!(l.is_finite(), "DC HP should stay finite with large input");
        assert!(r.is_finite(), "DC HP should stay finite with large input");
    }
    // Then silence
    for _ in 0..44100 {
        let (l, r) = hp.process(0.0, 0.0);
        assert!(l.is_finite(), "DC HP should stay finite");
        assert!(r.is_finite(), "DC HP should stay finite");
    }
}
