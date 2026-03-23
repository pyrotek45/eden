// Eden DAW — Unit Tests
//
// Covers:
//   • Undo/Redo: every Command variant → apply then undo restores original state
//   • CommandManager: stack behaviour, redo clearing, max history
//   • DSP primitives: polyblep, adsr_tick, svf_tick, osc_morph
//   • Model constructors: Track, Clip, RackSlot, Project
//   • Module registry: create_instrument, create_effect, is_instrument, get_param_descs
//   • SuperSaw bank: JP-8000 detune table produces detuned frequencies

#[cfg(test)]
mod tests {
    use crate::commands::*;
    use crate::models::*;
    use crate::modules::*;

    // ─── Helper: create a minimal project with one MIDI track + clip ───

    fn make_test_project() -> Project {
        let mut p = Project::default();
        p.name = "Test".into();
        let mut t = Track::new(1, "Track1", TrackType::Midi);
        t.volume = 0.8;
        t.pan = 0.0;
        t.mute = false;
        t.solo = false;
        t.rack.push(RackSlot::subtractive_synth(100));
        t.clips.push(Clip::Midi(MidiClip {
            notes: vec![
                MidiNote {
                    pitch: 60,
                    velocity: 100,
                    start: 0.0,
                    length: 1.0,
                },
                MidiNote {
                    pitch: 64,
                    velocity: 90,
                    start: 1.0,
                    length: 1.0,
                },
                MidiNote {
                    pitch: 67,
                    velocity: 95,
                    start: 2.0,
                    length: 1.0,
                },
            ],
            start_time: 0.0,
            length: 4.0,
            name: "Clip1".into(),
            color: [100, 160, 255, 200],
        }));
        p.tracks.push(t);

        // Add audio track
        let mut t2 = Track::new(2, "Audio1", TrackType::Audio);
        t2.clips.push(Clip::Audio(AudioClip {
            source_file: "test.wav".into(),
            start_time: 0.0,
            offset: 0.0,
            length: 8.0,
            gain: 1.0,
            name: "Audio1".into(),
            color: [220, 140, 60, 200],
        }));
        p.tracks.push(t2);

        // Add automation track
        let mut t3 = Track::new(3, "Auto1", TrackType::Automation);
        t3.clips.push(Clip::Automation(AutomationClip {
            points: vec![
                AutomationPoint {
                    time: 0.0,
                    value: 0.2,
                },
                AutomationPoint {
                    time: 4.0,
                    value: 0.8,
                },
            ],
            start_time: 0.0,
            length: 4.0,
            target_param: "filter_cutoff".into(),
            name: "Auto1".into(),
            color: [220, 180, 80, 200],
        }));
        p.tracks.push(t3);

        p
    }

    /// Assert two projects are identical (using serde serialization for deep comparison).
    fn assert_project_eq(a: &Project, b: &Project) {
        let ja = serde_json::to_string(a).unwrap();
        let jb = serde_json::to_string(b).unwrap();
        assert_eq!(ja, jb, "Projects differ after undo");
    }

    // ═══════════════════════════════════════════════════════════════
    // COMMAND UNDO/REDO TESTS
    // For every command: snapshot → apply → undo → assert snapshot == current
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_set_track_volume_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetTrackVolume {
            track_id: 1,
            new_value: 0.5,
            old_value: 0.0,
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].volume - 0.5).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_track_pan_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetTrackPan {
            track_id: 1,
            new_value: -0.5,
            old_value: 0.0,
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].pan - (-0.5)).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_track_mute_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetTrackMute {
            track_id: 1,
            new_value: true,
            old_value: false,
        };
        cmd.apply(&mut project);
        assert!(project.tracks[0].mute);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_track_solo_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetTrackSolo {
            track_id: 1,
            new_value: true,
            old_value: false,
        };
        cmd.apply(&mut project);
        assert!(project.tracks[0].solo);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_add_track_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let new_track = Track::new(99, "NewTrack", TrackType::Midi);
        let mut cmd = AddTrack { track: new_track };
        cmd.apply(&mut project);
        assert_eq!(project.tracks.len(), 4);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_remove_track_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = RemoveTrack {
            track_id: 1,
            removed_track: None,
            index: 0,
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks.len(), 2);
        assert!(project.tracks.iter().all(|t| t.id != 1));
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_tempo_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetTempo {
            new_bpm: 140.0,
            old_bpm: 0.0,
        };
        cmd.apply(&mut project);
        assert!((project.tempo_map.changes[0].bpm - 140.0).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_move_clip_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = MoveClip {
            track_id: 1,
            clip_index: 0,
            new_start: 4.0,
            old_start: 0.0,
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].clips[0].start_time() - 4.0).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_loop_region_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetLoopRegion {
            new_start: 2.0,
            new_end: 6.0,
            old_start: 0.0,
            old_end: 0.0,
        };
        cmd.apply(&mut project);
        assert!((project.transport.loop_region.start - 2.0).abs() < 1e-6);
        assert!((project.transport.loop_region.end - 6.0).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_toggle_transport_playing_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = ToggleTransportPlaying { old_value: false };
        cmd.apply(&mut project);
        assert!(project.transport.playing);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_toggle_loop_enabled_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = ToggleLoopEnabled { old_value: false };
        cmd.apply(&mut project);
        assert!(project.transport.loop_enabled);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_reset_transport_position_undo() {
        let mut project = make_test_project();
        project.transport.position = 4.0;
        let snapshot = project.clone();
        let mut cmd = ResetTransportPosition { old_value: 0.0 };
        cmd.apply(&mut project);
        assert!((project.transport.position).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_add_clips_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let new_clip = Clip::Midi(MidiClip {
            notes: vec![],
            start_time: 8.0,
            length: 4.0,
            name: "New".into(),
            color: [100, 100, 100, 200],
        });
        let mut cmd = AddClips {
            clips: vec![(1, new_clip)],
            added_indices: vec![],
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks[0].clips.len(), 2);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_move_clips_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = MoveClips {
            moves: vec![((1, 0), 0.0, 8.0)],
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].clips[0].start_time() - 8.0).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_resize_clip_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = ResizeClip {
            track_id: 1,
            clip_idx: 0,
            old_start: 0.0,
            old_len: 4.0,
            new_start: 0.0,
            new_len: 8.0,
            old_audio_offset: None,
            new_audio_offset: None,
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].clips[0].length() - 8.0).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_resize_track_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = ResizeTrack {
            track_id: 1,
            old_height: 80,
            new_height: 120,
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks[0].height, 120);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_add_midi_note_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = AddMidiNote {
            track_id: 1,
            clip_idx: 0,
            note: MidiNote {
                pitch: 72,
                velocity: 100,
                start: 3.0,
                length: 1.0,
            },
        };
        cmd.apply(&mut project);
        if let Clip::Midi(m) = &project.tracks[0].clips[0] {
            assert_eq!(m.notes.len(), 4);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_duplicate_notes_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let new_notes = vec![
            MidiNote {
                pitch: 60,
                velocity: 100,
                start: 4.0,
                length: 1.0,
            },
            MidiNote {
                pitch: 64,
                velocity: 90,
                start: 5.0,
                length: 1.0,
            },
        ];
        let mut cmd = DuplicateNotes {
            track_id: 1,
            clip_idx: 0,
            new_notes,
            count: 0,
        };
        cmd.apply(&mut project);
        if let Clip::Midi(m) = &project.tracks[0].clips[0] {
            assert_eq!(m.notes.len(), 5);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_delete_midi_notes_undo() {
        let mut project = make_test_project();
        let notes_backup: Vec<MidiNote> = if let Clip::Midi(m) = &project.tracks[0].clips[0] {
            m.notes.clone()
        } else {
            vec![]
        };
        let snapshot = project.clone();
        let mut cmd = DeleteMidiNotes {
            track_id: 1,
            clip_idx: 0,
            notes: vec![(0, notes_backup[0].clone()), (2, notes_backup[2].clone())],
        };
        cmd.apply(&mut project);
        if let Clip::Midi(m) = &project.tracks[0].clips[0] {
            assert_eq!(m.notes.len(), 1);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_move_midi_notes_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = MoveMidiNotes {
            track_id: 1,
            clip_idx: 0,
            moves: vec![(0, 0.0, 60, 2.0, 62)],
        };
        cmd.apply(&mut project);
        if let Clip::Midi(m) = &project.tracks[0].clips[0] {
            assert_eq!(m.notes[0].pitch, 62);
            assert!((m.notes[0].start - 2.0).abs() < 1e-6);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_delete_clips_undo() {
        let mut project = make_test_project();
        let clip_data = project.tracks[0].clips[0].clone();
        let snapshot = project.clone();
        let mut cmd = DeleteClips {
            clips: vec![(1, 0, clip_data)],
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks[0].clips.len(), 0);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_create_clip_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let new_clip = Clip::Midi(MidiClip {
            notes: vec![],
            start_time: 4.0,
            length: 4.0,
            name: "New".into(),
            color: [0, 0, 0, 200],
        });
        let mut cmd = CreateClip {
            track_id: 1,
            clip: new_clip,
            added_idx: 0,
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks[0].clips.len(), 2);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_resize_clips_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = ResizeClips {
            clips: vec![(1, 0, 0.0, 4.0, 0.0, 8.0)],
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].clips[0].length() - 8.0).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_resize_midi_note_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = ResizeMidiNote {
            track_id: 1,
            clip_idx: 0,
            note_idx: 0,
            old_len: 1.0,
            new_len: 2.0,
        };
        cmd.apply(&mut project);
        if let Clip::Midi(m) = &project.tracks[0].clips[0] {
            assert!((m.notes[0].length - 2.0).abs() < 1e-6);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_reorder_track_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = ReorderTrack {
            track_id: 1,
            old_index: 0,
            new_index: 2,
        };
        cmd.apply(&mut project);
        // Track 1 should now be at index 2
        assert_eq!(project.tracks[2].id, 1);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_track_name_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetTrackName {
            track_id: 1,
            old_name: String::new(),
            new_name: "Renamed".into(),
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks[0].name, "Renamed");
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_rack_slot_toggle_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = RackSlotToggle {
            track_id: 1,
            slot_idx: 0,
            old_enabled: true,
        };
        cmd.apply(&mut project);
        assert!(!project.tracks[0].rack[0].enabled);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_rack_slot_add_undo() {
        let mut project = make_test_project();
        let initial_rack_len = project.tracks[0].rack.len();
        let snapshot = project.clone();
        let mut cmd = RackSlotAdd {
            track_id: 1,
            slot: RackSlot::delay(200),
            insert_at: None,
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks[0].rack.len(), initial_rack_len + 1);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_rack_slot_remove_undo() {
        let mut project = make_test_project();
        let initial_rack_len = project.tracks[0].rack.len();
        let snapshot = project.clone();
        let mut cmd = RackSlotRemove {
            track_id: 1,
            slot_idx: 0,
            removed_slot: None,
        };
        cmd.apply(&mut project);
        assert_eq!(project.tracks[0].rack.len(), initial_rack_len - 1);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_rack_param_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetRackParam {
            track_id: 1,
            slot_idx: 0,
            param_idx: 0, // osc1_wave
            old_value: 0.0,
            new_value: 2.0,
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].rack[0].params[0].value - 2.0).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_set_note_velocity_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = SetNoteVelocity {
            track_id: 1,
            clip_idx: 0,
            note_idx: 0,
            old_velocity: 0,
            new_velocity: 50,
        };
        cmd.apply(&mut project);
        if let Clip::Midi(m) = &project.tracks[0].clips[0] {
            assert_eq!(m.notes[0].velocity, 50);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_add_automation_point_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = AddAutomationPoint {
            track_id: 3,
            clip_idx: 0,
            point: AutomationPoint {
                time: 2.0,
                value: 0.5,
            },
            inserted_idx: 0,
        };
        cmd.apply(&mut project);
        if let Clip::Automation(a) = &project.tracks[2].clips[0] {
            assert_eq!(a.points.len(), 3);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_delete_automation_point_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = DeleteAutomationPoint {
            track_id: 3,
            clip_idx: 0,
            point_idx: 0,
            removed_point: None,
        };
        cmd.apply(&mut project);
        if let Clip::Automation(a) = &project.tracks[2].clips[0] {
            assert_eq!(a.points.len(), 1);
        }
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_move_automation_point_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let mut cmd = MoveAutomationPoint {
            track_id: 3,
            clip_idx: 0,
            point_idx: 0,
            old_time: 0.0,
            old_value: 0.2,
            new_time: 1.0,
            new_value: 0.6,
        };
        cmd.apply(&mut project);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    #[test]
    fn test_composite_command_undo() {
        let mut project = make_test_project();
        let snapshot = project.clone();
        let cmds: Vec<Box<dyn Command>> = vec![
            Box::new(SetTrackVolume {
                track_id: 1,
                new_value: 0.5,
                old_value: 0.0,
            }),
            Box::new(SetTrackPan {
                track_id: 1,
                new_value: 0.3,
                old_value: 0.0,
            }),
        ];
        let mut cmd = CompositeCommand {
            desc: "Composite".into(),
            cmds,
        };
        cmd.apply(&mut project);
        assert!((project.tracks[0].volume - 0.5).abs() < 1e-6);
        assert!((project.tracks[0].pan - 0.3).abs() < 1e-6);
        cmd.undo(&mut project);
        assert_project_eq(&snapshot, &project);
    }

    // ═══════════════════════════════════════════════════════════════
    // COMMAND MANAGER TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_command_manager_undo_redo() {
        let mut project = make_test_project();
        let mut mgr = CommandManager::new(100);

        let original_vol = project.tracks[0].volume;

        mgr.execute(
            Box::new(SetTrackVolume {
                track_id: 1,
                new_value: 0.5,
                old_value: 0.0,
            }),
            &mut project,
        );
        assert!((project.tracks[0].volume - 0.5).abs() < 1e-6);
        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());

        mgr.undo(&mut project);
        assert!((project.tracks[0].volume - original_vol).abs() < 1e-6);
        assert!(!mgr.can_undo());
        assert!(mgr.can_redo());

        mgr.redo(&mut project);
        assert!((project.tracks[0].volume - 0.5).abs() < 1e-6);
        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());
    }

    #[test]
    fn test_command_manager_redo_cleared_on_new_command() {
        let mut project = make_test_project();
        let mut mgr = CommandManager::new(100);

        mgr.execute(
            Box::new(SetTrackVolume {
                track_id: 1,
                new_value: 0.5,
                old_value: 0.0,
            }),
            &mut project,
        );
        mgr.undo(&mut project);
        assert!(mgr.can_redo());

        // New command should clear redo stack
        mgr.execute(
            Box::new(SetTrackPan {
                track_id: 1,
                new_value: 0.3,
                old_value: 0.0,
            }),
            &mut project,
        );
        assert!(!mgr.can_redo());
    }

    #[test]
    fn test_command_manager_max_history() {
        let mut project = make_test_project();
        let mut mgr = CommandManager::new(3);

        for i in 0..5 {
            mgr.execute(
                Box::new(SetTrackVolume {
                    track_id: 1,
                    new_value: i as f32 * 0.1,
                    old_value: 0.0,
                }),
                &mut project,
            );
        }

        // Should only have 3 undos (max_history = 3)
        let mut undo_count = 0;
        while mgr.can_undo() {
            mgr.undo(&mut project);
            undo_count += 1;
        }
        assert_eq!(undo_count, 3);
    }

    #[test]
    fn test_command_manager_descriptions() {
        let mut project = make_test_project();
        let mut mgr = CommandManager::new(100);

        assert_eq!(mgr.undo_description(), None);
        mgr.execute(
            Box::new(SetTrackVolume {
                track_id: 1,
                new_value: 0.5,
                old_value: 0.0,
            }),
            &mut project,
        );
        assert_eq!(mgr.undo_description(), Some("Set Track Volume"));
    }

    #[test]
    fn test_command_manager_push_undo() {
        let mut project = make_test_project();
        let mut mgr = CommandManager::new(100);

        // Snapshot the project before mutation
        let snapshot = project.clone();

        // Manually apply a change (simulating live drag)
        project.tracks[0].volume = 0.5;

        // push_undo_snapshot: record the pre-mutation state
        mgr.push_undo_snapshot(snapshot, "Set Track Volume");
        assert!(mgr.can_undo());
        // Volume should be changed (we applied it manually)
        assert!((project.tracks[0].volume - 0.5).abs() < 1e-6);

        // Undo should restore the snapshot (original volume)
        mgr.undo(&mut project);
        assert!((project.tracks[0].volume - 0.8).abs() < 1e-6);
    }

    // ═══════════════════════════════════════════════════════════════
    // DSP TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_polyblep_at_zero() {
        // At phase 0, polyblep should correct the discontinuity
        let dt = 0.01;
        let val = polyblep(0.0, dt);
        assert!((val - (-1.0)).abs() < 1e-6, "polyblep(0, dt) should be -1");
    }

    #[test]
    fn test_polyblep_at_midpoint() {
        // In the middle of the phase, polyblep should be 0
        let dt = 0.01;
        let val = polyblep(0.5, dt);
        assert!(val.abs() < 1e-6, "polyblep(0.5, dt) should be 0");
    }

    #[test]
    fn test_polyblep_near_one() {
        // Near phase = 1.0
        let dt = 0.01;
        let val = polyblep(0.999, dt);
        // Should be non-zero near the discontinuity
        assert!(val.abs() > 0.0);
    }

    #[test]
    fn test_adsr_attack_phase() {
        let mut stage = EnvStage::Attack;
        let mut level = 0.0;
        let mut time = 0.0;
        let dt = 1.0 / 44100.0;

        // Run for 100 samples in attack mode
        for _ in 0..100 {
            adsr_tick(
                &mut stage, &mut level, &mut time, 0.01, 0.1, 0.5, 0.1, dt, false,
            );
        }
        // Level should have risen from 0
        assert!(level > 0.0, "Level should increase during attack");
        assert!(level <= 1.0, "Level should not exceed 1.0");
    }

    #[test]
    fn test_adsr_full_cycle() {
        let mut stage = EnvStage::Attack;
        let mut level = 0.0;
        let mut time = 0.0;
        let dt = 1.0 / 44100.0;
        let attack = 0.001;
        let decay = 0.001;
        let sustain = 0.5;
        let release = 0.001;

        // Run through attack + decay to sustain
        for _ in 0..1000 {
            adsr_tick(
                &mut stage, &mut level, &mut time, attack, decay, sustain, release, dt, false,
            );
        }
        // Should be near sustain level
        assert!(
            (level - sustain).abs() < 0.1,
            "Level should be near sustain: got {}",
            level
        );

        // Trigger release
        for _ in 0..5000 {
            adsr_tick(
                &mut stage, &mut level, &mut time, attack, decay, sustain, release, dt, true,
            );
        }
        // Should be near zero / off
        assert!(
            level < 0.01,
            "Level should be near 0 after release: got {}",
            level
        );
        assert_eq!(stage, EnvStage::Off);
    }

    #[test]
    fn test_svf_tick_lowpass() {
        let mut ic1 = 0.0;
        let mut ic2 = 0.0;
        let sr = 44100.0;

        // Feed a DC signal through a lowpass — should pass through
        let mut last_lp = 0.0;
        for _ in 0..1000 {
            let (lp, _, _) = svf_tick(1.0, 1000.0, 0.0, sr, &mut ic1, &mut ic2);
            last_lp = lp;
        }
        // LP should converge near 1.0 for DC input
        assert!(
            (last_lp - 1.0).abs() < 0.1,
            "LP should pass DC: got {}",
            last_lp
        );
    }

    #[test]
    fn test_svf_tick_highpass() {
        let mut ic1 = 0.0;
        let mut ic2 = 0.0;
        let sr = 44100.0;

        // Feed DC through highpass — should block it
        let mut last_hp = 0.0;
        for _ in 0..1000 {
            let (_, _, hp) = svf_tick(1.0, 1000.0, 0.0, sr, &mut ic1, &mut ic2);
            last_hp = hp;
        }
        assert!(last_hp.abs() < 0.1, "HP should block DC: got {}", last_hp);
    }

    #[test]
    fn test_osc_morph_sine() {
        let mut noise = 12345u64;
        let val = osc_morph(0.0, 0.25, 0.01, &mut noise);
        // Phase 0.25 of a sine = sin(0.5π) = 1.0
        assert!(
            (val - 1.0).abs() < 0.01,
            "Sine at phase 0.25 should be ~1.0, got {}",
            val
        );
    }

    #[test]
    fn test_osc_morph_crossfade() {
        let mut noise = 12345u64;
        // Shape 0.5 = halfway between sine and saw
        let val = osc_morph(0.5, 0.5, 0.01, &mut noise);
        // Sine at 0.5 = 0.0, Saw at 0.5 = 0.0 → crossfade = 0.0
        assert!(val.abs() < 0.1, "Crossfade at phase 0.5 should be near 0");
    }

    #[test]
    fn test_param_val_found() {
        let params = vec![("gain".to_string(), 0.5f32), ("cutoff".to_string(), 0.8f32)];
        assert!((param_val(&params, "gain", 1.0) - 0.5).abs() < 1e-6);
        assert!((param_val(&params, "cutoff", 0.0) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_param_val_not_found_returns_default() {
        let params: Vec<(String, f32)> = vec![];
        assert!((param_val(&params, "gain", 0.7) - 0.7).abs() < 1e-6);
    }

    // ═══════════════════════════════════════════════════════════════
    // MODEL TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_track_new() {
        let t = Track::new(1, "Test", TrackType::Midi);
        assert_eq!(t.id, 1);
        assert_eq!(t.name, "Test");
        assert_eq!(t.track_type, TrackType::Midi);
        assert!((t.volume - 0.8).abs() < 1e-6);
        assert!((t.pan).abs() < 1e-6);
        assert!(!t.mute);
        assert!(!t.solo);
        assert!(t.clips.is_empty());
    }

    #[test]
    fn test_clip_accessors() {
        let clip = Clip::Midi(MidiClip {
            notes: vec![],
            start_time: 2.0,
            length: 4.0,
            name: "Clip".into(),
            color: [100, 100, 100, 200],
        });
        assert!((clip.start_time() - 2.0).abs() < 1e-6);
        assert!((clip.length() - 4.0).abs() < 1e-6);
        assert_eq!(clip.name(), "Clip");
    }

    #[test]
    fn test_clip_set_start_time() {
        let mut clip = Clip::Midi(MidiClip {
            notes: vec![],
            start_time: 0.0,
            length: 4.0,
            name: "Clip".into(),
            color: [0; 4],
        });
        clip.set_start_time(3.0);
        assert!((clip.start_time() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_clip_set_length() {
        let mut clip = Clip::Audio(AudioClip {
            source_file: "test.wav".into(),
            start_time: 0.0,
            offset: 0.0,
            length: 4.0,
            gain: 1.0,
            name: "Audio".into(),
            color: [0; 4],
        });
        clip.set_length(8.0);
        assert!((clip.length() - 8.0).abs() < 1e-6);
    }

    #[test]
    fn test_rack_param_new() {
        let p = RackParam::new("cutoff", "Cutoff", 0.5, 0.0, 1.0);
        assert_eq!(p.id, "cutoff");
        assert_eq!(p.name, "Cutoff");
        assert!((p.value - 0.5).abs() < 1e-6);
        assert!((p.min).abs() < 1e-6);
        assert!((p.max - 1.0).abs() < 1e-6);
        assert!((p.default - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_rack_slot_constructors() {
        // Test all rack slot constructors produce valid slots
        let constructors: Vec<(&str, fn(u32) -> RackSlot)> = vec![
            ("Sine Osc", RackSlot::sine_osc),
            ("Square Osc", RackSlot::square_osc),
            ("Saw Osc", RackSlot::saw_osc),
            ("Triangle Osc", RackSlot::triangle_osc),
            ("Analog", RackSlot::subtractive_synth),
            ("HyperSaw", RackSlot::supersaw),
            ("Sampler", RackSlot::sampler),
            ("Monolith", RackSlot::heavy_synth),
            ("LP Filter", RackSlot::lpfilter),
            ("HP Filter", RackSlot::hpfilter),
            ("Delay", RackSlot::delay),
            ("Reverb", RackSlot::reverb),
            ("Chorus", RackSlot::chorus),
            ("Distortion", RackSlot::distortion),
            ("Compressor", RackSlot::compressor),
            ("EQ", RackSlot::eq),
            ("Gain", RackSlot::gain),
            ("Utility", RackSlot::utility),
        ];
        for (name, ctor) in constructors {
            let slot = ctor(1);
            assert_eq!(slot.plugin_name, name);
            assert!(slot.enabled);
            assert!(!slot.params.is_empty(), "{} should have params", name);
        }
    }

    #[test]
    fn test_create_rack_slot_for_module() {
        let modules = vec![
            "Analog",
            "HyperSaw",
            "Sampler",
            "Monolith",
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
        ];
        for name in modules {
            let slot = create_rack_slot_for_module(name, 1);
            assert_eq!(slot.plugin_name, name);
        }
    }

    #[test]
    fn test_project_default() {
        let p = Project::default();
        assert_eq!(p.name, "Untitled");
        assert_eq!(p.sample_rate, 44100);
        assert!(p.tracks.is_empty());
        assert!(!p.transport.playing);
    }

    #[test]
    fn test_project_demo() {
        let p = Project::demo();
        assert_eq!(p.name, "Demo Project");
        assert!(!p.tracks.is_empty());
    }

    #[test]
    fn test_project_next_track_id() {
        let mut p = Project::default();
        assert_eq!(p.next_track_id(), 1);
        p.tracks.push(Track::new(5, "T", TrackType::Midi));
        assert_eq!(p.next_track_id(), 6);
    }

    #[test]
    fn test_tempo_map_bpm_at() {
        let tm = TempoMap::default();
        assert!((tm.bpm_at(0.0) - 128.0).abs() < 1e-6);
    }

    #[test]
    fn test_tempo_map_beats_seconds_conversion() {
        let tm = TempoMap::default();
        let bpm = tm.bpm_at(0.0);
        let secs = tm.beats_to_seconds(1.0);
        assert!((secs - 60.0 / bpm).abs() < 1e-6);
        let beats_back = tm.seconds_to_beats(secs);
        assert!((beats_back - 1.0).abs() < 1e-6);
    }

    // ═══════════════════════════════════════════════════════════════
    // MODULE REGISTRY TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_is_instrument() {
        assert!(is_instrument("Analog"));
        assert!(is_instrument("HyperSaw"));
        assert!(is_instrument("Sampler"));
        assert!(is_instrument("Monolith"));
        assert!(!is_instrument("LP Filter"));
        assert!(!is_instrument("Unknown"));
    }

    #[test]
    fn test_is_effect() {
        assert!(is_effect("LP Filter"));
        assert!(is_effect("HP Filter"));
        assert!(is_effect("Delay"));
        assert!(is_effect("Reverb"));
        assert!(is_effect("Chorus"));
        assert!(is_effect("Distortion"));
        assert!(is_effect("Compressor"));
        assert!(is_effect("EQ"));
        assert!(is_effect("Gain"));
        assert!(is_effect("Utility"));
        assert!(!is_effect("Analog"));
    }

    #[test]
    fn test_create_instrument() {
        assert!(create_instrument("Analog").is_some());
        assert!(create_instrument("HyperSaw").is_some());
        assert!(create_instrument("Sampler").is_some());
        assert!(create_instrument("Monolith").is_some());
        assert!(create_instrument("Unknown").is_none());
    }

    #[test]
    fn test_create_effect() {
        let effects = vec![
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
        ];
        for name in effects {
            assert!(
                create_effect(name, 44100).is_some(),
                "create_effect({}) should work",
                name
            );
        }
        assert!(create_effect("Unknown", 44100).is_none());
    }

    #[test]
    fn test_get_param_descs() {
        let modules = vec![
            "Analog",
            "HyperSaw",
            "Sampler",
            "Monolith",
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
        ];
        for name in modules {
            let descs = get_param_descs(name);
            assert!(!descs.is_empty(), "{} should have param descs", name);
        }
        assert!(get_param_descs("Unknown").is_empty());
    }

    #[test]
    fn test_instrument_params_match_rack_slot() {
        // Verify instrument param descs match the rack slot params
        let instruments = vec![
            ("Analog", RackSlot::subtractive_synth(1)),
            ("HyperSaw", RackSlot::supersaw(1)),
            ("Sampler", RackSlot::sampler(1)),
            ("Monolith", RackSlot::heavy_synth(1)),
        ];
        for (name, slot) in instruments {
            let descs = get_param_descs(name);
            assert_eq!(
                descs.len(),
                slot.params.len(),
                "{}: param desc count ({}) != rack slot param count ({})",
                name,
                descs.len(),
                slot.params.len()
            );
            for (desc, param) in descs.iter().zip(slot.params.iter()) {
                assert_eq!(
                    desc.id, param.id,
                    "{}: param id mismatch: desc='{}' vs slot='{}'",
                    name, desc.id, param.id
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // VOICE STATE TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_voice_state_default_phases_different() {
        let v1 = VoiceState::default();
        // All extra phases should be between 0 and 1
        for p in &v1.extra_phases {
            assert!(*p >= 0.0 && *p <= 1.0, "Phase out of range: {}", p);
        }
    }

    #[test]
    fn test_module_voice_new() {
        let v = ModuleVoice::new(440.0, 0.8, 0, 69);
        assert!((v.freq - 440.0).abs() < 1e-6);
        assert!((v.velocity - 0.8).abs() < 1e-6);
        assert_eq!(v.track_idx, 0);
        assert_eq!(v.pitch, 69);
        assert!(!v.released);
        assert!(v.preview_samples_remaining.is_none());
    }

    #[test]
    fn test_voice_is_done() {
        let mut v = ModuleVoice::new(440.0, 0.8, 0, 69);
        assert!(!voice_is_done(&v));
        v.state.amp_stage = EnvStage::Off;
        assert!(voice_is_done(&v));
    }

    // ═══════════════════════════════════════════════════════════════
    // SUPERSAW BANK TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_supersaw_bank_produces_output() {
        let mut phases = [0.0f64; 7];
        let sr = 44100.0;
        let mut sum = 0.0;
        for _ in 0..1000 {
            let (l, r) = supersaw_bank(&mut phases, 440.0, 0.3, 0.75, sr, 0.5);
            sum += l.abs() + r.abs();
        }
        assert!(sum > 0.0, "SuperSaw bank should produce non-zero output");
    }

    #[test]
    fn test_supersaw_bank_no_detune() {
        // With detune=0, all 7 saws should be at the same frequency
        let mut phases = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
        let sr = 44100.0;
        let mut sum = 0.0_f64;
        for _ in 0..1000 {
            let (l, r) = supersaw_bank(&mut phases, 440.0, 0.0, 0.5, sr, 0.5);
            sum += l.abs() + r.abs();
        }
        assert!(sum > 0.01, "Should have non-zero output sum: got {}", sum);
    }

    #[test]
    fn test_supersaw_bank_zero_mix() {
        // With mix=0, detuned oscillators contribute nothing → only center saw
        let mut phases = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
        let sr = 44100.0;
        let mut sum = 0.0_f64;
        for _ in 0..1000 {
            let (l, r) = supersaw_bank(&mut phases, 440.0, 0.5, 0.0, sr, 0.5);
            sum += l.abs() + r.abs();
        }
        assert!(sum > 0.01, "Center saw should produce output: got {}", sum);
    }

    // ═══════════════════════════════════════════════════════════════
    // EFFECT MODULE TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_effect_fresh_resets_state() {
        let effects: Vec<Box<dyn EffectModule>> = vec![
            Box::new(FxLpFilter::new()),
            Box::new(FxHpFilter::new()),
            Box::new(FxDelay::new(44100)),
            Box::new(FxReverb::new(44100)),
            Box::new(FxChorus::new(44100)),
            Box::new(FxDistortion),
            Box::new(FxCompressor::new()),
            Box::new(FxEq::new()),
            Box::new(FxGain),
            Box::new(FxUtility),
        ];
        for eff in &effects {
            let fresh = eff.fresh();
            assert_eq!(fresh.name(), eff.name());
            assert_eq!(fresh.params().len(), eff.params().len());
        }
    }

    #[test]
    fn test_gain_effect_unity() {
        let mut fx = FxGain;
        let params = vec![("gain_db".to_string(), 0.0f32)];
        let (l, _r) = fx.process(1.0, 1.0, &params, 44100.0);
        assert!(
            (l - 1.0).abs() < 1e-6,
            "0dB gain should pass through unchanged"
        );
    }

    #[test]
    fn test_distortion_bypass_at_zero_drive() {
        let mut fx = FxDistortion;
        let params = vec![
            ("drive".to_string(), 0.0f32),
            ("type".to_string(), 0.0f32),
            ("mix".to_string(), 1.0f32),
        ];
        let (l, _r) = fx.process(0.5, 0.5, &params, 44100.0);
        assert!(
            (l - 0.5).abs() < 0.01,
            "Zero drive should pass through: got {}",
            l
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // INSTRUMENT PROCESSING TESTS
    // ═══════════════════════════════════════════════════════════════

    fn descs_to_params(descs: &[ParamDesc]) -> Vec<(String, f32)> {
        descs
            .iter()
            .map(|p| (p.id.to_string(), p.default))
            .collect()
    }

    #[test]
    fn test_subtractive_synth_produces_output() {
        let synth = SubtractiveSynth;
        let mut voice = ModuleVoice::new(440.0, 0.8, 0, 69);
        let params = descs_to_params(get_param_descs("Analog"));
        let extra = ModuleExtra::default();

        let mut sum = 0.0;
        for _ in 0..1000 {
            let (l, r) = synth.process_voice(&mut voice, &params, 44100.0, &extra);
            sum += l.abs() + r.abs();
        }
        assert!(sum > 0.0, "SubtractiveSynth should produce output");
    }

    #[test]
    fn test_supersaw_synth_produces_output() {
        let synth = SuperSawSynth;
        let mut voice = ModuleVoice::new(440.0, 0.8, 0, 69);
        let params = descs_to_params(get_param_descs("HyperSaw"));
        let extra = ModuleExtra::default();

        let mut sum = 0.0;
        for _ in 0..1000 {
            let (l, r) = synth.process_voice(&mut voice, &params, 44100.0, &extra);
            sum += l.abs() + r.abs();
        }
        assert!(sum > 0.0, "SuperSawSynth should produce output");
    }

    #[test]
    fn test_heavy_synth_produces_output() {
        let synth = HeavySynth;
        let mut voice = ModuleVoice::new(440.0, 0.8, 0, 69);
        let params = descs_to_params(get_param_descs("Monolith"));
        let extra = ModuleExtra::default();

        let mut sum = 0.0;
        for _ in 0..1000 {
            let (l, r) = synth.process_voice(&mut voice, &params, 44100.0, &extra);
            sum += l.abs() + r.abs();
        }
        assert!(sum > 0.0, "HeavySynth should produce output");
    }

    #[test]
    fn test_heavy_synth_all_shapes() {
        // Verify all 8 oscillator shapes produce output
        for shape in 0..8 {
            let synth = HeavySynth;
            let mut voice = ModuleVoice::new(440.0, 0.8, 0, 69);
            let mut params = descs_to_params(get_param_descs("Monolith"));
            for p in params.iter_mut() {
                if p.0 == "osc_shape" {
                    p.1 = shape as f32;
                }
            }
            let extra = ModuleExtra::default();
            let mut sum = 0.0;
            for _ in 0..1000 {
                let (l, r) = synth.process_voice(&mut voice, &params, 44100.0, &extra);
                sum += l.abs() + r.abs();
            }
            assert!(
                sum > 0.0,
                "HeavySynth shape {} should produce output",
                shape
            );
        }
    }

    #[test]
    fn test_supersaw_two_oscillators() {
        let synth = SuperSawSynth;
        let mut voice = ModuleVoice::new(440.0, 0.8, 0, 69);
        let mut params = descs_to_params(get_param_descs("HyperSaw"));

        // Set blend to 0.5 (mix both oscs) and detune osc2 by +7 semitones
        for p in params.iter_mut() {
            if p.0 == "osc_blend" {
                p.1 = 0.5;
            }
            if p.0 == "osc2_semi" {
                p.1 = 7.0;
            }
        }
        let extra = ModuleExtra::default();

        let mut sum = 0.0;
        for _ in 0..2000 {
            let (l, r) = synth.process_voice(&mut voice, &params, 44100.0, &extra);
            sum += l.abs() + r.abs();
        }
        assert!(sum > 0.0, "Dual SuperSaw should produce output");
    }

    #[test]
    fn test_instrument_voice_release() {
        let synth = SubtractiveSynth;
        let mut voice = ModuleVoice::new(440.0, 0.8, 0, 69);
        let mut params = descs_to_params(get_param_descs("Analog"));
        // Very short envelope
        for p in params.iter_mut() {
            match p.0.as_str() {
                "amp_a" => p.1 = 0.001,
                "amp_d" => p.1 = 0.001,
                "amp_r" => p.1 = 0.001,
                "amp_s" => p.1 = 0.5,
                _ => {}
            }
        }
        let extra = ModuleExtra::default();

        // Play for a bit
        for _ in 0..500 {
            synth.process_voice(&mut voice, &params, 44100.0, &extra);
        }
        assert!(!voice_is_done(&voice));

        // Release
        voice.released = true;
        for _ in 0..5000 {
            synth.process_voice(&mut voice, &params, 44100.0, &extra);
        }
        assert!(voice_is_done(&voice), "Voice should be done after release");
    }

    // ═══════════════════════════════════════════════════════════════
    // EFFECT MODULE TESTS
    // ═══════════════════════════════════════════════════════════════

    fn descs_to_fx_params(descs: &[ParamDesc]) -> Vec<(String, f32)> {
        descs
            .iter()
            .map(|d| (d.id.to_string(), d.default))
            .collect()
    }

    #[test]
    fn test_lp_filter_attenuates_highs() {
        let mut fx = FxLpFilter::new();
        let params = descs_to_fx_params(get_param_descs("LP Filter"));
        // Set cutoff very low
        let low_params: Vec<(String, f32)> = params
            .iter()
            .map(|(n, v)| {
                if n == "cutoff" {
                    (n.clone(), 0.0)
                } else {
                    (n.clone(), *v)
                }
            })
            .collect();

        // Feed in a high freq signal (~10kHz)
        let sr = 44100.0;
        let mut sum_open = 0.0_f64;
        let mut sum_closed = 0.0_f64;
        let mut fx_open = FxLpFilter::new();
        for i in 0..1000 {
            let sig = (i as f64 * 10000.0 * std::f64::consts::TAU / sr).sin();
            let (l, _) = fx_open.process(sig, sig, &params, sr);
            sum_open += l.abs();
            let (l2, _) = fx.process(sig, sig, &low_params, sr);
            sum_closed += l2.abs();
        }
        assert!(
            sum_closed < sum_open * 0.5,
            "LP filter should attenuate highs"
        );
    }

    #[test]
    fn test_hp_filter_attenuates_lows() {
        let mut fx = FxHpFilter::new();
        let params = descs_to_fx_params(get_param_descs("HP Filter"));
        // Set cutoff very high
        let high_params: Vec<(String, f32)> = params
            .iter()
            .map(|(n, v)| {
                if n == "cutoff" {
                    (n.clone(), 1.0)
                } else {
                    (n.clone(), *v)
                }
            })
            .collect();

        let sr = 44100.0;
        let mut sum_open = 0.0_f64;
        let mut sum_closed = 0.0_f64;
        let mut fx_open = FxHpFilter::new();
        for i in 0..1000 {
            let sig = (i as f64 * 100.0 * std::f64::consts::TAU / sr).sin(); // 100Hz
            let (l, _) = fx_open.process(sig, sig, &params, sr);
            sum_open += l.abs();
            let (l2, _) = fx.process(sig, sig, &high_params, sr);
            sum_closed += l2.abs();
        }
        assert!(
            sum_closed < sum_open * 0.5,
            "HP filter should attenuate lows"
        );
    }

    #[test]
    fn test_delay_produces_echo() {
        let mut fx = FxDelay::new(44100);
        let params = descs_to_fx_params(get_param_descs("Delay"));

        // Feed an impulse, then check for echo after delay time
        let sr = 44100.0;
        let (l, _) = fx.process(1.0, 1.0, &params, sr);
        let _ = l;
        // Process silence for delay time worth of samples
        let delay_samples = (0.25 * sr) as usize;
        let mut found_echo = false;
        for _ in 0..delay_samples + 10 {
            let (l, _) = fx.process(0.0, 0.0, &params, sr);
            if l.abs() > 0.01 {
                found_echo = true;
            }
        }
        assert!(found_echo, "Delay should produce an echo");
    }

    #[test]
    fn test_distortion_increases_harmonics() {
        let mut fx_clean = FxDistortion;
        let mut fx_dirty = FxDistortion;
        let clean_params = descs_to_fx_params(get_param_descs("Distortion"));
        let dirty_params: Vec<(String, f32)> = clean_params
            .iter()
            .map(|(n, v)| {
                if n == "drive" {
                    (n.clone(), 0.9)
                } else {
                    (n.clone(), *v)
                }
            })
            .collect();
        let no_drive: Vec<(String, f32)> = clean_params
            .iter()
            .map(|(n, _)| {
                if n == "drive" {
                    (n.clone(), 0.0)
                } else {
                    (n.clone(), 0.0)
                }
            })
            .collect();

        let sr = 44100.0;
        let mut _sum_clean = 0.0_f64;
        let mut sum_dirty = 0.0_f64;
        for i in 0..1000 {
            let sig = (i as f64 * 440.0 * std::f64::consts::TAU / sr).sin() * 0.5;
            let (l, _) = fx_clean.process(sig, sig, &no_drive, sr);
            _sum_clean += l.abs();
            let (l2, _) = fx_dirty.process(sig, sig, &dirty_params, sr);
            sum_dirty += l2.abs();
        }
        assert!(sum_dirty > 0.0, "Distortion should produce output");
    }

    #[test]
    fn test_gain_boosts_signal() {
        let mut fx = FxGain;
        let sr = 44100.0;
        let boost_params = vec![("gain_db".to_string(), 12.0_f32)];
        let (l, r) = fx.process(0.5, 0.5, &boost_params, sr);
        assert!(l > 0.5, "Gain +12dB should boost signal");
        assert!(r > 0.5, "Gain +12dB should boost signal (R)");
    }

    #[test]
    fn test_utility_pan_left() {
        let mut fx = FxUtility;
        let sr = 44100.0;
        let params = vec![
            ("gain_db".to_string(), 0.0_f32),
            ("pan".to_string(), -1.0),
            ("phase".to_string(), 0.0),
            ("dc_offset".to_string(), 0.0),
        ];
        let (l, r) = fx.process(1.0, 1.0, &params, sr);
        assert!(l > r, "Pan full left: L ({}) should be > R ({})", l, r);
        assert!(r < 0.01, "Pan full left: R should be near zero");
    }

    #[test]
    fn test_utility_pan_right() {
        let mut fx = FxUtility;
        let sr = 44100.0;
        let params = vec![
            ("gain_db".to_string(), 0.0_f32),
            ("pan".to_string(), 1.0),
            ("phase".to_string(), 0.0),
            ("dc_offset".to_string(), 0.0),
        ];
        let (l, r) = fx.process(1.0, 1.0, &params, sr);
        assert!(r > l, "Pan full right: R ({}) should be > L ({})", r, l);
        assert!(l < 0.01, "Pan full right: L should be near zero");
    }

    #[test]
    fn test_compressor_reduces_loud_signal() {
        let mut fx = FxCompressor::new();
        let sr = 44100.0;
        let params: Vec<(String, f32)> = vec![
            ("threshold".to_string(), 0.3),
            ("ratio".to_string(), 0.8),
            ("attack".to_string(), 0.001),
            ("release".to_string(), 0.1),
            ("makeup".to_string(), 0.0),
        ];
        // Feed loud signal until envelope converges
        let mut last_l = 0.0;
        for _ in 0..2000 {
            let (l, _) = fx.process(0.9, 0.9, &params, sr);
            last_l = l;
        }
        assert!(
            last_l.abs() < 0.9,
            "Compressor should reduce loud signal: got {}",
            last_l
        );
    }

    #[test]
    fn test_reverb_adds_tail() {
        let mut fx = FxReverb::new(44100);
        let sr = 44100.0;
        let params = descs_to_fx_params(get_param_descs("Reverb"));
        // Feed impulse
        fx.process(1.0, 1.0, &params, sr);
        // Check for reverb tail in silence
        let mut tail_energy = 0.0_f64;
        for _ in 0..4000 {
            let (l, r) = fx.process(0.0, 0.0, &params, sr);
            tail_energy += l.abs() + r.abs();
        }
        assert!(tail_energy > 0.1, "Reverb should have a tail after impulse");
    }

    #[test]
    fn test_eq_boosts_low() {
        let mut fx = FxEq::new();
        let sr = 44100.0;
        let mut params = descs_to_fx_params(get_param_descs("EQ"));
        for p in params.iter_mut() {
            if p.0 == "lo_gain" {
                p.1 = 12.0;
            }
        }
        // Feed a low freq signal
        let mut sum = 0.0_f64;
        for i in 0..2000 {
            let sig = (i as f64 * 100.0 * std::f64::consts::TAU / sr).sin() * 0.5;
            let (l, _) = fx.process(sig, sig, &params, sr);
            sum += l.abs();
        }
        // Compare with flat EQ
        let mut fx2 = FxEq::new();
        let flat_params = descs_to_fx_params(get_param_descs("EQ"));
        let mut sum_flat = 0.0_f64;
        for i in 0..2000 {
            let sig = (i as f64 * 100.0 * std::f64::consts::TAU / sr).sin() * 0.5;
            let (l, _) = fx2.process(sig, sig, &flat_params, sr);
            sum_flat += l.abs();
        }
        assert!(sum > sum_flat, "EQ +12dB low should boost low frequencies");
    }

    #[test]
    fn test_chorus_modulates_signal() {
        let mut fx = FxChorus::new(44100);
        let sr = 44100.0;
        let params = descs_to_fx_params(get_param_descs("Chorus"));
        let mut outputs = Vec::new();
        for i in 0..2000 {
            let sig = (i as f64 * 440.0 * std::f64::consts::TAU / sr).sin() * 0.5;
            let (l, _) = fx.process(sig, sig, &params, sr);
            outputs.push(l);
        }
        // Chorus should produce non-zero output
        let sum: f64 = outputs.iter().map(|x| x.abs()).sum();
        assert!(sum > 0.0, "Chorus should produce output");
    }

    #[test]
    fn test_supersaw_stereo_width() {
        let synth = SuperSawSynth;
        let mut voice = ModuleVoice::new(440.0, 0.8, 0, 69);
        let mut params = descs_to_params(get_param_descs("HyperSaw"));
        // Set width to 1.0 for maximum stereo spread
        for p in params.iter_mut() {
            if p.0 == "osc1_width" {
                p.1 = 1.0;
            }
        }
        let extra = ModuleExtra::default();
        let mut diff_sum = 0.0_f64;
        for _ in 0..2000 {
            let (l, r) = synth.process_voice(&mut voice, &params, 44100.0, &extra);
            diff_sum += (l - r).abs();
        }
        assert!(
            diff_sum > 0.0,
            "SuperSaw with width=1.0 should have stereo difference"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // SERIALIZATION ROUND-TRIP TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_project_serialize_deserialize() {
        let p = make_test_project();
        let json = serde_json::to_string(&p).unwrap();
        let p2: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(p.name, p2.name);
        assert_eq!(p.tracks.len(), p2.tracks.len());
        assert_eq!(p.tracks[0].name, p2.tracks[0].name);
    }

    #[test]
    fn test_demo_project_serialize_roundtrip() {
        let p = Project::demo();
        let json = serde_json::to_string(&p).unwrap();
        let p2: Project = serde_json::from_str(&json).unwrap();
        assert_project_eq(&p, &p2);
    }

    // ═══════════════════════════════════════════════════════════════
    // UNDO/REDO ROUND-TRIP STRESS TEST
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_multiple_undo_redo_roundtrips() {
        let mut project = make_test_project();
        let mut mgr = CommandManager::new(100);
        let original = project.clone();

        // Apply several commands
        mgr.execute(
            Box::new(SetTrackVolume {
                track_id: 1,
                new_value: 0.5,
                old_value: 0.0,
            }),
            &mut project,
        );
        mgr.execute(
            Box::new(SetTrackPan {
                track_id: 1,
                new_value: 0.3,
                old_value: 0.0,
            }),
            &mut project,
        );
        mgr.execute(
            Box::new(SetTrackMute {
                track_id: 1,
                new_value: true,
                old_value: false,
            }),
            &mut project,
        );
        mgr.execute(
            Box::new(AddMidiNote {
                track_id: 1,
                clip_idx: 0,
                note: MidiNote {
                    pitch: 72,
                    velocity: 100,
                    start: 3.0,
                    length: 1.0,
                },
            }),
            &mut project,
        );

        // Undo all
        while mgr.can_undo() {
            mgr.undo(&mut project);
        }
        assert_project_eq(&original, &project);

        // Redo all
        while mgr.can_redo() {
            mgr.redo(&mut project);
        }

        // Undo all again
        while mgr.can_undo() {
            mgr.undo(&mut project);
        }
        assert_project_eq(&original, &project);
    }
}
