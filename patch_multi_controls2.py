import re
file_path = "src/views.rs"
with open(file_path, "r") as f:
    code = f.read()

pan_track_lanes_old = """\
            if pan_changed {
                if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == id) {
                    t.pan = pan;
                    state.dirty = true;
                }
            }
            // Commit pan change to undo stack on mouse release
            if input.mouse_released && input.drag_widget == WidgetId::Knob(id * 10 + 1) {
                let old_pan = input.drag_start_value as f32;
                if (old_pan - pan).abs() > 1e-4 {
                    state.commands.execute(
                        Box::new(crate::commands::SetTrackPan { track_id: id, old_value: old_pan, new_value: pan }),
                        &mut state.project,
                    );
                }
            }"""

pan_track_lanes_new = """\
            if pan_changed {
                let mut targets = state.selected_tracks.clone();
                targets.insert(id);
                for tid in &targets {
                    if let Some(t) = state.project.tracks.iter_mut().find(|tx| tx.id == *tid) {
                        t.pan = pan;
                        state.dirty = true;
                    }
                }
            }
            if input.mouse_released && input.drag_widget == WidgetId::Knob(id * 10 + 1) {
                let old_pan = input.drag_start_value as f32;
                if (old_pan - pan).abs() > 1e-4 {
                    let mut targets = state.selected_tracks.clone();
                    targets.insert(id);
                    let mut cmds: Vec<Box<dyn crate::commands::Command>> = Vec::new();
                    for tid in &targets {
                        cmds.push(Box::new(crate::commands::SetTrackPan {
                            track_id: *tid, old_value: old_pan, new_value: pan
                        }));
                    }
                    if cmds.len() == 1 {
                        state.commands.execute(cmds.remove(0), &mut state.project);
                    } else {
                        state.commands.execute(Box::new(crate::commands::CompositeCommand { desc: "Set Multi Pan".into(), cmds }), &mut state.project);
                    }
                }
            }"""

code = code.replace(pan_track_lanes_old, pan_track_lanes_new)

solo_old = """\
        if solo_clicked {
            if let Some(t) = state.project.tracks.iter().find(|t| t.id == id) {
                state.commands.execute(
                    Box::new(crate::commands::SetTrackSolo {
                        track_id: id,
                        new_value: !t.solo,
                        old_value: t.solo,
                    }),
                    &mut state.project,
                );
            }
        }"""

solo_new = """\
        if solo_clicked {
            let mut targets = state.selected_tracks.clone();
            targets.insert(id);
            let mut cmds: Vec<Box<dyn crate::commands::Command>> = Vec::new();
            for tid in &targets {
                if let Some(t) = state.project.tracks.iter().find(|tx| tx.id == *tid) {
                    cmds.push(Box::new(crate::commands::SetTrackSolo {
                        track_id: *tid, new_value: !solo, old_value: t.solo,
                    }));
                }
            }
            if cmds.len() == 1 {
                state.commands.execute(cmds.remove(0), &mut state.project);
            } else {
                state.commands.execute(Box::new(crate::commands::CompositeCommand { desc: "Set Multi Solo".into(), cmds }), &mut state.project);
            }
        }"""
code = code.replace(solo_old, solo_new)

with open(file_path, "w") as f:
    f.write(code)

print("Patched basic pan/solo")
