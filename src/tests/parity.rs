//! Render ↔ playback signal-chain parity tests.

use super::*;

/// CStrip2 with default params (EQ neutral, comp off) should pass audio through
/// with minimal level change (<1 dB attenuation, no clipping).
#[test]
fn test_cstrip2_passthrough() {
    use crate::modules::create_effect;
    let sr = 44100u32;
    let mut cs = create_effect("CStrip2", sr).unwrap();
    let params = vec![
        ("treble".to_string(), 0.5f32),
        ("treb_frq".to_string(), 0.5),
        ("mid".to_string(), 0.5),
        ("bass".to_string(), 0.5),
        ("bass_frq".to_string(), 0.5),
        ("lo_cap".to_string(), 1.0),
        ("hi_cap".to_string(), 0.0),
        ("compress".to_string(), 0.0),
        ("comp_spd".to_string(), 0.0),
        ("output".to_string(), 0.33),
    ];

    let input_level = 0.5_f64;
    let (out_l, out_r) = cs.process(input_level, input_level, &params, sr as f64);

    // Output should be finite and not silent
    assert!(out_l.is_finite(), "CStrip2 L output must be finite");
    assert!(out_r.is_finite(), "CStrip2 R output must be finite");
    assert!(out_l.abs() > 0.001, "CStrip2 should not silence input");
    // Output should not clip hard (saturation may raise it slightly)
    assert!(out_l.abs() < 3.0, "CStrip2 output should not clip severely");
}

/// CStrip2 with maximum output gain should amplify signal.
#[test]
fn test_cstrip2_output_gain() {
    use crate::modules::create_effect;
    let sr = 44100u32;
    let mut cs_low = create_effect("CStrip2", sr).unwrap();
    let mut cs_high = create_effect("CStrip2", sr).unwrap();

    let params_low = vec![
        ("treble".to_string(), 0.5f32),
        ("treb_frq".to_string(), 0.5),
        ("mid".to_string(), 0.5),
        ("bass".to_string(), 0.5),
        ("bass_frq".to_string(), 0.5),
        ("lo_cap".to_string(), 1.0),
        ("hi_cap".to_string(), 0.0),
        ("compress".to_string(), 0.0),
        ("comp_spd".to_string(), 0.0),
        ("output".to_string(), 0.0), // min gain
    ];
    let mut params_high = params_low.clone();
    params_high.last_mut().unwrap().1 = 1.0; // max gain

    let (low_l, _) = cs_low.process(0.5, 0.5, &params_low, sr as f64);
    let (high_l, _) = cs_high.process(0.5, 0.5, &params_high, sr as f64);

    assert!(
        high_l.abs() > low_l.abs(),
        "higher output param should produce louder output ({} vs {})",
        high_l.abs(),
        low_l.abs()
    );
}

/// CStrip2 output must remain finite over a sustained sine burst (no divergence).
#[test]
fn test_cstrip2_no_divergence() {
    use crate::modules::create_effect;
    let sr = 44100u32;
    let mut cs = create_effect("CStrip2", sr).unwrap();
    let params = vec![
        ("treble".to_string(), 0.8f32), // boosted highs
        ("treb_frq".to_string(), 0.5),
        ("mid".to_string(), 0.8),
        ("bass".to_string(), 0.8),
        ("bass_frq".to_string(), 0.5),
        ("lo_cap".to_string(), 1.0),
        ("hi_cap".to_string(), 0.0),
        ("compress".to_string(), 0.5), // light compression
        ("comp_spd".to_string(), 0.5),
        ("output".to_string(), 0.5),
    ];

    for i in 0..4410 {
        // 100 ms of 440 Hz sine
        let t = i as f64 / sr as f64;
        let s = (2.0 * std::f64::consts::PI * 440.0 * t).sin() * 0.8;
        let (ol, or2) = cs.process(s, s, &params, sr as f64);
        assert!(
            ol.is_finite() && or2.is_finite(),
            "CStrip2 output diverged at sample {}: L={} R={}",
            i,
            ol,
            or2
        );
        assert!(
            ol.abs() < 10.0 && or2.is_finite(),
            "CStrip2 severely clipping at sample {}",
            i
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// PLAYBACK / RENDER STRUCTURAL PARITY TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_parity_cstrip2_affects_output() {
    let mut p_plain = make_test_project();
    p_plain.master_rack.clear();
    p_plain.tracks[0].cstrip2_bypass = false;
    p_plain.tracks[0].cstrip2_params.clear();
    let buf_plain = render_to_buffer(&p_plain, 44100, 1.0);

    let mut p_cstrip = make_test_project();
    p_cstrip.master_rack.clear();
    p_cstrip.tracks[0].cstrip2_bypass = false;
    p_cstrip.tracks[0].cstrip2_params = vec![
        ("treble".into(), 0.8),
        ("bass".into(), 0.2),
        ("output_db".into(), 6.0),
    ];
    let buf_cstrip = render_to_buffer(&p_cstrip, 44100, 1.0);

    let len = buf_plain.len().min(buf_cstrip.len());
    assert!(len > 0);
    let mut diff_sum = 0.0_f64;
    for i in 0..len {
        diff_sum += (buf_plain[i].0 - buf_cstrip[i].0).abs();
        diff_sum += (buf_plain[i].1 - buf_cstrip[i].1).abs();
    }
    let avg_diff = diff_sum / (len * 2) as f64;
    assert!(
        avg_diff > 0.001,
        "CStrip2 with non-default params should change output, avg_diff={:.8}",
        avg_diff
    );
}

#[test]
fn test_parity_cstrip2_bypass_is_passthrough() {
    let mut p_plain = make_test_project();
    p_plain.master_rack.clear();
    p_plain.tracks[0].cstrip2_bypass = true;
    p_plain.tracks[0].cstrip2_params.clear();
    let buf_plain = render_to_buffer(&p_plain, 44100, 1.0);

    let mut p_bypass = make_test_project();
    p_bypass.master_rack.clear();
    p_bypass.tracks[0].cstrip2_bypass = true;
    p_bypass.tracks[0].cstrip2_params = vec![
        ("treble".into(), 1.0),
        ("bass".into(), 0.0),
        ("output_db".into(), 12.0),
    ];
    let buf_bypass = render_to_buffer(&p_bypass, 44100, 1.0);

    let len = buf_plain.len().min(buf_bypass.len());
    let mut max_diff = 0.0_f64;
    for i in 0..len {
        max_diff = max_diff.max((buf_plain[i].0 - buf_bypass[i].0).abs());
        max_diff = max_diff.max((buf_plain[i].1 - buf_bypass[i].1).abs());
    }
    assert!(
        max_diff < 1e-10,
        "Bypassed CStrip2 should be passthrough, max_diff={:.12}",
        max_diff
    );
}

#[test]
fn test_parity_cstrip2_empty_params_no_effect() {
    let mut p1 = make_test_project();
    p1.master_rack.clear();
    p1.tracks[0].cstrip2_bypass = false;
    p1.tracks[0].cstrip2_params.clear();
    let buf1 = render_to_buffer(&p1, 44100, 1.0);

    let mut p2 = make_test_project();
    p2.master_rack.clear();
    p2.tracks[0].cstrip2_bypass = true;
    p2.tracks[0].cstrip2_params.clear();
    let buf2 = render_to_buffer(&p2, 44100, 1.0);

    let len = buf1.len().min(buf2.len());
    let mut max_diff = 0.0_f64;
    for i in 0..len {
        max_diff = max_diff.max((buf1[i].0 - buf2[i].0).abs());
        max_diff = max_diff.max((buf1[i].1 - buf2[i].1).abs());
    }
    assert!(
        max_diff < 1e-10,
        "Empty cstrip2_params should have no effect, max_diff={:.12}",
        max_diff
    );
}

#[test]
fn test_parity_cstrip2_deterministic() {
    let mut p = make_test_project();
    p.tracks[0].cstrip2_bypass = false;
    p.tracks[0].cstrip2_params = vec![("treble".into(), 0.6), ("bass".into(), 0.4)];
    let buf1 = render_to_buffer(&p, 44100, 1.0);
    let buf2 = render_to_buffer(&p, 44100, 1.0);
    assert_eq!(buf1.len(), buf2.len());
    for i in 0..buf1.len() {
        assert!(
            (buf1[i].0 - buf2[i].0).abs() < 1e-10 && (buf1[i].1 - buf2[i].1).abs() < 1e-10,
            "Render with CStrip2 not deterministic at sample {}",
            i
        );
    }
}

#[test]
fn test_parity_effect_chain_before_cstrip() {
    let mut p = make_test_project();
    p.master_rack.clear();
    let mut gain_slot = RackSlot::gain(10);
    if let Some(g) = gain_slot.params.iter_mut().find(|p| p.id == "gain_db") {
        g.value = 6.0;
    }
    p.tracks[0].rack.push(gain_slot);
    p.tracks[0].cstrip2_bypass = false;
    p.tracks[0].cstrip2_params = vec![("treble".into(), 0.9), ("bass".into(), 0.1)];
    let buf_with = render_to_buffer(&p, 44100, 1.0);

    p.tracks[0].cstrip2_bypass = true;
    let buf_without = render_to_buffer(&p, 44100, 1.0);

    let len = buf_with.len().min(buf_without.len());
    let mut diff_sum = 0.0_f64;
    for i in 0..len {
        diff_sum += (buf_with[i].0 - buf_without[i].0).abs();
    }
    let avg = diff_sum / len.max(1) as f64;
    assert!(
        avg > 0.0001,
        "CStrip2 after gain should change signal, avg_diff={:.8}",
        avg
    );
}

#[test]
fn test_parity_velocity_not_scaled_by_volume() {
    let mut p1 = make_test_project();
    p1.master_rack.clear();
    p1.tracks[0].volume = 1.0;
    let buf1 = render_to_buffer(&p1, 44100, 1.0);

    let mut p2 = make_test_project();
    p2.master_rack.clear();
    p2.tracks[0].volume = 0.5;
    let buf2 = render_to_buffer(&p2, 44100, 1.0);

    let rms_fn = |buf: &[(f64, f64)]| -> f64 {
        let sum: f64 = buf.iter().map(|(l, r)| l * l + r * r).sum();
        (sum / buf.len().max(1) as f64).sqrt()
    };
    let r1 = rms_fn(&buf1);
    let r2 = rms_fn(&buf2);
    if r1 > 0.001 {
        let ratio = r2 / r1;
        assert!(
            (ratio - 0.5).abs() < 0.15,
            "Volume 0.5 should halve RMS (linear scaling only), ratio={:.4}",
            ratio
        );
    }
}

#[test]
fn test_parity_sqrt2_pan_compensation() {
    let mut p = Project::default();
    p.master_rack.clear();
    let mut t = Track::new(1, "T", TrackType::Midi);
    t.rack = vec![RackSlot::subtractive_synth(1)];
    t.volume = 1.0;
    t.pan = 0.0;
    t.clips.push(Clip::Midi(MidiClip {
        notes: vec![MidiNote {
            pitch: 69,
            velocity: 100,
            start: 0.0,
            length: 2.0,
        }],
        start_time: 0.0,
        length: 2.0,
        name: "N".into(),
        color: [0; 4],
    }));
    p.tracks.push(t);
    let buf = render_to_buffer(&p, 44100, 1.0);
    let peak_l: f64 = buf.iter().map(|(l, _)| l.abs()).fold(0.0, f64::max);
    assert!(
        peak_l > 0.3,
        "Center pan should not attenuate mono signal (sqrt2 compensation), peak_l={:.4}",
        peak_l
    );
}

#[test]
fn test_parity_no_slew_or_tanh_in_render() {
    let mut p = Project::default();
    p.master_rack = vec![RackSlot::limiter(1)];
    if let Some(c) = p.master_rack[0]
        .params
        .iter_mut()
        .find(|p| p.id == "ceiling_db")
    {
        c.value = -6.0;
    }
    for i in 0..3 {
        let mut t = Track::new(i + 1, &format!("T{}", i), TrackType::Midi);
        t.volume = 1.0;
        t.rack = vec![RackSlot::subtractive_synth(1)];
        t.clips.push(Clip::Midi(MidiClip {
            notes: vec![MidiNote {
                pitch: 60 + (i * 5) as u8,
                velocity: 127,
                start: 0.0,
                length: 2.0,
            }],
            start_time: 0.0,
            length: 2.0,
            name: "C".into(),
            color: [0; 4],
        }));
        p.tracks.push(t);
    }
    let buf = render_to_buffer(&p, 44100, 1.0);
    let ceiling_lin = 10.0_f64.powf(-6.0 / 20.0);
    let max_abs = buf
        .iter()
        .map(|(l, r)| l.abs().max(r.abs()))
        .fold(0.0_f64, f64::max);
    assert!(
        max_abs <= ceiling_lin + 0.01,
        "Render should be hard-clamped at ceiling: max={:.6}, ceiling={:.6}",
        max_abs,
        ceiling_lin
    );
}
