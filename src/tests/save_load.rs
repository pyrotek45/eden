//! Save/load (serde JSON) round-trip tests.

use super::*;

#[test]
fn test_save_load_maxed_project_roundtrip() {
    let p = make_maxed_project();
    let json = serde_json::to_string_pretty(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_project_eq(&p, &p2);
}

#[test]
fn test_save_load_project_name() {
    let mut p = Project::default();
    p.name = "My Cool Project 🎵".into();
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.name, "My Cool Project 🎵");
}

#[test]
fn test_save_load_sample_rate() {
    let mut p = Project::default();
    p.sample_rate = 96000;
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.sample_rate, 96000);
}

#[test]
fn test_save_load_time_signature() {
    let mut p = Project::default();
    p.time_signature = (7, 8);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.time_signature, (7, 8));
}

#[test]
fn test_save_load_tempo_map() {
    let mut p = Project::default();
    p.tempo_map.changes = vec![
        TempoChange {
            beat: 0.0,
            bpm: 80.0,
        },
        TempoChange {
            beat: 32.0,
            bpm: 200.0,
        },
    ];
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.tempo_map.changes.len(), 2);
    assert!((p2.tempo_map.changes[1].bpm - 200.0).abs() < 0.001);
}

#[test]
fn test_save_load_transport_all_fields() {
    let mut p = Project::default();
    p.transport.playing = true;
    p.transport.recording = true;
    p.transport.position = 123.456;
    p.transport.loop_enabled = true;
    p.transport.loop_region.start = 4.0;
    p.transport.loop_region.end = 32.0;
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert!(p2.transport.playing);
    assert!(p2.transport.recording);
    assert!((p2.transport.position - 123.456).abs() < 0.001);
    assert!(p2.transport.loop_enabled);
    assert!((p2.transport.loop_region.start - 4.0).abs() < 0.001);
    assert!((p2.transport.loop_region.end - 32.0).abs() < 0.001);
}

#[test]
fn test_save_load_master_rack() {
    let mut p = Project::default();
    p.master_rack = vec![
        RackSlot::eq(1),
        RackSlot::compressor(2),
        RackSlot::limiter(3),
    ];
    // Tweak a param
    p.master_rack[0].params[0].value = -6.0;
    p.master_rack[1].enabled = false;
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.master_rack.len(), 3);
    assert_eq!(p2.master_rack[0].plugin_name, "EQ");
    assert!((p2.master_rack[0].params[0].value - (-6.0)).abs() < 0.001);
    assert!(!p2.master_rack[1].enabled);
    assert_eq!(p2.master_rack[2].plugin_name, "Limiter");
}

#[test]
fn test_save_load_track_all_fields() {
    let mut p = Project::default();
    let mut t = Track::new(42, "TestTrack", TrackType::Midi);
    t.volume = 0.37;
    t.pan = -0.8;
    t.mute = true;
    t.solo = true;
    t.color = [1, 2, 3, 4];
    t.height = 200;
    t.instrument_idx = 5;
    t.sampler_file = Some("my_sample.wav".into());
    t.automation_enabled = false;
    t.cstrip2_params = vec![("treble".into(), 0.9)];
    t.cstrip2_bypass = true;
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    let t2 = &p2.tracks[0];
    assert_eq!(t2.id, 42);
    assert_eq!(t2.name, "TestTrack");
    assert!((t2.volume - 0.37).abs() < 0.001);
    assert!((t2.pan - (-0.8)).abs() < 0.001);
    assert!(t2.mute);
    assert!(t2.solo);
    assert_eq!(t2.color, [1, 2, 3, 4]);
    assert_eq!(t2.height, 200);
    assert_eq!(t2.instrument_idx, 5);
    assert_eq!(t2.sampler_file.as_deref(), Some("my_sample.wav"));
    assert!(!t2.automation_enabled);
    assert_eq!(t2.cstrip2_params.len(), 1);
    assert!(t2.cstrip2_bypass);
}

#[test]
fn test_save_load_rack_slot_sidechain() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    let mut comp = RackSlot::compressor(10);
    comp.sidechain_track_id = Some(99);
    t.rack.push(comp);
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(
        p2.tracks[0].rack.last().unwrap().sidechain_track_id,
        Some(99)
    );
}

#[test]
fn test_save_load_rack_param_ranges() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    let mut slot = RackSlot::limiter(1);
    // Set each param to its min and max
    slot.params[0].value = slot.params[0].max; // gain_db = 24
    slot.params[1].value = slot.params[1].min; // ceiling = -12
    t.rack = vec![slot];
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    let pp = &p2.tracks[0].rack[0].params;
    assert!((pp[0].value - 24.0).abs() < 0.001);
    assert!((pp[1].value - (-12.0)).abs() < 0.001);
}

#[test]
fn test_save_load_midi_clip_all_fields() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    t.clips.push(Clip::Midi(MidiClip {
        notes: vec![
            MidiNote {
                pitch: 0,
                velocity: 0,
                start: 0.0,
                length: 0.001,
            },
            MidiNote {
                pitch: 127,
                velocity: 127,
                start: 99.0,
                length: 100.0,
            },
        ],
        start_time: 5.5,
        length: 200.0,
        name: "Extreme Clip".into(),
        color: [0, 0, 0, 0],
    }));
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    if let Clip::Midi(mc) = &p2.tracks[0].clips[0] {
        assert_eq!(mc.notes.len(), 2);
        assert_eq!(mc.notes[0].pitch, 0);
        assert_eq!(mc.notes[1].velocity, 127);
        assert!((mc.start_time - 5.5).abs() < 0.001);
        assert!((mc.length - 200.0).abs() < 0.001);
        assert_eq!(mc.name, "Extreme Clip");
    } else {
        panic!("Expected Midi clip");
    }
}

#[test]
fn test_save_load_audio_clip_all_fields() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Audio);
    t.clips.push(Clip::Audio(AudioClip {
        source_file: "/long/path/to/file.wav".into(),
        start_time: 10.0,
        offset: 2.5,
        length: 32.0,
        gain: 0.1,
        name: "My Audio".into(),
        color: [128, 128, 128, 255],
        fade_in: 0.05,
        fade_out: 1.0,
    }));
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    if let Clip::Audio(ac) = &p2.tracks[0].clips[0] {
        assert_eq!(ac.source_file, "/long/path/to/file.wav");
        assert!((ac.offset - 2.5).abs() < 0.001);
        assert!((ac.gain - 0.1).abs() < 0.001);
        assert!((ac.fade_in - 0.05).abs() < 0.001);
        assert!((ac.fade_out - 1.0).abs() < 0.001);
    } else {
        panic!("Expected Audio clip");
    }
}

#[test]
fn test_save_load_automation_clip_all_fields() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Automation);
    t.clips.push(Clip::Automation(AutomationClip {
        points: vec![
            AutomationPoint {
                time: 0.0,
                value: -1.0,
            },
            AutomationPoint {
                time: 100.0,
                value: 1.0,
            },
        ],
        start_time: 0.0,
        length: 100.0,
        target_param: "1:1:cutoff".into(),
        name: "Sweep".into(),
        color: [255, 255, 255, 255],
    }));
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    if let Clip::Automation(ac) = &p2.tracks[0].clips[0] {
        assert_eq!(ac.points.len(), 2);
        assert!((ac.points[1].value - 1.0).abs() < 0.001);
        assert_eq!(ac.target_param, "1:1:cutoff");
    } else {
        panic!("Expected Automation clip");
    }
}

#[test]
fn test_save_load_empty_project() {
    let p = Project::default();
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.tracks.len(), 0);
    assert_eq!(p2.name, "Untitled");
}

#[test]
fn test_save_load_many_tracks() {
    let mut p = Project::default();
    for i in 0..100 {
        p.tracks
            .push(Track::new(i, &format!("Track{}", i), TrackType::Midi));
    }
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.tracks.len(), 100);
    assert_eq!(p2.tracks[99].name, "Track99");
}

#[test]
fn test_save_load_missing_optional_fields_use_defaults() {
    // Simulate loading a JSON that's missing serde(default) fields
    let json = r#"{
        "name": "Old",
        "sample_rate": 44100,
        "tracks": [{
            "id": 1,
            "name": "T",
            "track_type": "Midi",
            "volume": 0.5,
            "pan": 0.0,
            "mute": false,
            "solo": false,
            "clips": [],
            "color": [100, 160, 255, 200],
            "height": 80
        }],
        "tempo_map": {"changes": [{"beat": 0.0, "bpm": 120.0}]},
        "transport": {"playing": false, "recording": false, "position": 0.0, "loop_enabled": false, "loop_region": {"start": 0.0, "end": 8.0}}
    }"#;
    let p: Project = serde_json::from_str(json).unwrap();
    // serde(default) fields should be their defaults
    assert_eq!(p.time_signature, (4, 4));
    assert!(p.master_rack.is_empty()); // serde(default) = empty vec
    assert_eq!(p.tracks[0].rack.len(), 0); // serde(default)
    assert!(p.tracks[0].automation_enabled); // default_true
    assert!(p.tracks[0].sampler_file.is_none());
    assert!(!p.tracks[0].cstrip2_bypass);
}

#[test]
fn test_save_load_corrupt_json_error() {
    let result: Result<Project, _> = serde_json::from_str("NOT JSON");
    assert!(result.is_err());
}

#[test]
fn test_save_load_wrong_types_error() {
    let result: Result<Project, _> = serde_json::from_str(r#"{"name": 42}"#);
    assert!(result.is_err());
}

#[test]
fn test_save_load_extreme_float_values() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    t.volume = f32::MIN_POSITIVE;
    t.pan = -1.0;
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert!(p2.tracks[0].volume > 0.0);
    assert!((p2.tracks[0].pan - (-1.0)).abs() < 0.001);
}

#[test]
fn test_save_load_every_rack_module_type() {
    let modules = [
        "Analog",
        "HyperSaw",
        "Sampler",
        "Monolith",
        "Sine Osc",
        "Square Osc",
        "Saw Osc",
        "Triangle Osc",
        "LP Filter",
        "HP Filter",
        "Delay",
        "Reverb",
        "Chorus",
        "Distortion",
        "Compressor",
        "EQ",
        "Gain",
        "Utility",
        "Limiter",
        "Autoduck",
        "Arpeggiator",
        "Chord",
        "Transpose",
        "Velocity",
    ];
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    t.rack.clear();
    for (i, name) in modules.iter().enumerate() {
        t.rack.push(create_rack_slot_for_module(name, i as u32 + 1));
    }
    p.tracks.push(t.clone());
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.tracks[0].rack.len(), modules.len());
    for (i, name) in modules.iter().enumerate() {
        assert_eq!(
            p2.tracks[0].rack[i].plugin_name, *name,
            "Module {} name mismatch after round-trip",
            name,
        );
        // Verify all params survived
        assert_eq!(
            p2.tracks[0].rack[i].params.len(),
            t.rack[i].params.len(),
            "Module {} param count mismatch",
            name,
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// SAVE/LOAD COMPLETENESS — EVERY KNOB & SETTING
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_save_load_rack_slot_enabled_flag() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    let mut slot = RackSlot::delay(10);
    slot.enabled = false;
    t.rack.push(slot);
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert!(
        !p2.tracks[0].rack.last().unwrap().enabled,
        "Disabled RackSlot should survive round-trip"
    );
}

#[test]
fn test_save_load_rack_param_name_and_default() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    let mut slot = RackSlot::compressor(10);
    // Mutate a param value away from its default
    slot.params[0].value = slot.params[0].min;
    t.rack.push(slot.clone());
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    let pp = &p2.tracks[0].rack.last().unwrap().params[0];
    assert_eq!(
        pp.name, slot.params[0].name,
        "Param display name should survive round-trip"
    );
    assert!(
        (pp.default - slot.params[0].default).abs() < 1e-6,
        "Param default should survive round-trip"
    );
    assert!(
        (pp.value - slot.params[0].min).abs() < 1e-6,
        "Param value at min should survive round-trip"
    );
}

#[test]
fn test_save_load_render_deterministic() {
    // Render a project, save/load it, render again — output must be identical
    let mut p = make_test_project();
    p.tracks[0].volume = 0.73;
    p.tracks[0].pan = -0.4;
    p.tracks[0].cstrip2_params = vec![("treble".into(), 0.7_f32)];
    p.tracks[0].cstrip2_bypass = false;
    let buf1 = render_to_buffer(&p, 44100, 1.0);

    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    let buf2 = render_to_buffer(&p2, 44100, 1.0);

    assert_eq!(
        buf1.len(),
        buf2.len(),
        "Render length mismatch after save/load"
    );
    for i in 0..buf1.len() {
        assert!(
            (buf1[i].0 - buf2[i].0).abs() < 1e-10 && (buf1[i].1 - buf2[i].1).abs() < 1e-10,
            "Render differs at sample {} after save/load: ({:.8},{:.8}) vs ({:.8},{:.8})",
            i,
            buf1[i].0,
            buf1[i].1,
            buf2[i].0,
            buf2[i].1
        );
    }
}

#[test]
fn test_save_load_loop_region() {
    let mut p = Project::default();
    p.transport.loop_enabled = true;
    p.transport.loop_region.start = 4.0;
    p.transport.loop_region.end = 16.0;
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert!(p2.transport.loop_enabled);
    assert!((p2.transport.loop_region.start - 4.0).abs() < 0.001);
    assert!((p2.transport.loop_region.end - 16.0).abs() < 0.001);
}

#[test]
fn test_save_load_transport_position() {
    let mut p = Project::default();
    p.transport.playing = true;
    p.transport.recording = true;
    p.transport.position = 42.5;
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert!(p2.transport.playing);
    assert!(p2.transport.recording);
    assert!((p2.transport.position - 42.5).abs() < 0.001);
}

#[test]
fn test_save_load_cstrip2_multi_params() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    t.cstrip2_params = vec![
        ("treble".into(), 0.8),
        ("bass".into(), 0.3),
        ("output_db".into(), -6.0),
    ];
    t.cstrip2_bypass = false;
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    let cs = &p2.tracks[0].cstrip2_params;
    assert_eq!(cs.len(), 3);
    assert_eq!(cs[0].0, "treble");
    assert!((cs[0].1 - 0.8).abs() < 1e-6);
    assert_eq!(cs[1].0, "bass");
    assert!((cs[1].1 - 0.3).abs() < 1e-6);
    assert_eq!(cs[2].0, "output_db");
    assert!((cs[2].1 - (-6.0)).abs() < 1e-6);
    assert!(!p2.tracks[0].cstrip2_bypass);
}

#[test]
fn test_save_load_master_rack_multi_effects() {
    let mut p = Project::default();
    p.master_rack = vec![
        RackSlot::compressor(1),
        RackSlot::eq(2),
        RackSlot::limiter(3),
    ];
    // Modify a param in each
    p.master_rack[0].params[0].value = -30.0; // threshold
    p.master_rack[1].params[0].value = 6.0; // lo_gain
    p.master_rack[2].params[0].value = 12.0; // gain_db
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.master_rack.len(), 3);
    assert_eq!(p2.master_rack[0].plugin_name, "Compressor");
    assert!((p2.master_rack[0].params[0].value - (-30.0)).abs() < 1e-6);
    assert_eq!(p2.master_rack[1].plugin_name, "EQ");
    assert!((p2.master_rack[1].params[0].value - 6.0).abs() < 1e-6);
    assert_eq!(p2.master_rack[2].plugin_name, "Limiter");
    assert!((p2.master_rack[2].params[0].value - 12.0).abs() < 1e-6);
}

#[test]
fn test_save_load_multiple_clips_per_track() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    for i in 0..5 {
        t.clips.push(Clip::Midi(MidiClip {
            notes: vec![MidiNote {
                pitch: 60 + i,
                velocity: 100,
                start: 0.0,
                length: 1.0,
            }],
            start_time: i as f64 * 2.0,
            length: 2.0,
            name: format!("Clip{}", i),
            color: [i as u8 * 50, 0, 0, 255],
        }));
    }
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.tracks[0].clips.len(), 5);
    for i in 0..5 {
        if let Clip::Midi(mc) = &p2.tracks[0].clips[i] {
            assert_eq!(mc.name, format!("Clip{}", i));
            assert!((mc.start_time - i as f64 * 2.0).abs() < 0.001);
            assert_eq!(mc.notes[0].pitch, 60 + i as u8);
        } else {
            panic!("Expected Midi clip at index {}", i);
        }
    }
}

#[test]
fn test_save_load_mixed_clip_types_on_one_track() {
    // A MIDI track with midi + automation clips (unusual but valid)
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    t.clips.push(Clip::Midi(MidiClip {
        notes: vec![MidiNote {
            pitch: 60,
            velocity: 100,
            start: 0.0,
            length: 1.0,
        }],
        start_time: 0.0,
        length: 2.0,
        name: "M".into(),
        color: [0; 4],
    }));
    t.clips.push(Clip::Automation(AutomationClip {
        points: vec![
            AutomationPoint {
                time: 0.0,
                value: 0.0,
            },
            AutomationPoint {
                time: 4.0,
                value: 1.0,
            },
        ],
        start_time: 0.0,
        length: 4.0,
        target_param: "1:1:cutoff".into(),
        name: "A".into(),
        color: [255, 0, 0, 255],
    }));
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.tracks[0].clips.len(), 2);
    assert!(matches!(p2.tracks[0].clips[0], Clip::Midi(_)));
    assert!(matches!(p2.tracks[0].clips[1], Clip::Automation(_)));
}

#[test]
fn test_save_load_all_track_types_in_project() {
    let mut p = Project::default();
    p.tracks.push(Track::new(1, "Midi", TrackType::Midi));
    p.tracks.push(Track::new(2, "Audio", TrackType::Audio));
    p.tracks.push(Track::new(3, "Auto", TrackType::Automation));
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(p2.tracks.len(), 3);
    assert_eq!(p2.tracks[0].track_type, TrackType::Midi);
    assert_eq!(p2.tracks[1].track_type, TrackType::Audio);
    assert_eq!(p2.tracks[2].track_type, TrackType::Automation);
}

#[test]
fn test_save_load_midi_effect_chain() {
    let mut p = Project::default();
    let mut t = Track::new(1, "T", TrackType::Midi);
    t.rack.push(RackSlot::arpeggiator(10));
    t.rack.push(RackSlot::chord(11));
    t.rack.push(RackSlot::transpose(12));
    t.rack.push(RackSlot::velocity(13));
    p.tracks.push(t);
    let json = serde_json::to_string(&p).unwrap();
    let p2: Project = serde_json::from_str(&json).unwrap();
    let rack = &p2.tracks[0].rack;
    assert_eq!(rack.len(), 5); // synth + 4 midi effects
    assert_eq!(rack[1].plugin_name, "Arpeggiator");
    assert_eq!(rack[2].plugin_name, "Chord");
    assert_eq!(rack[3].plugin_name, "Transpose");
    assert_eq!(rack[4].plugin_name, "Velocity");
}
