import re

file_path = "src/views.rs"
with open(file_path, "r") as f:
    code = f.read()

# For volume in track lanes (around line 617)
# "if vol_changed { ... }" -> We will replace this.

vol_track_lanes_old = """\
            if vol_changed {
                if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == id) {
                    t.volume = volume;
                    state.dirty = true;
                }
            }
            // Commit volume change to undo stack on mouse release
            if input.mouse_released && input.drag_widget == WidgetId::Slider(id * 10) {
                let old_vol = input.drag_start_value as f32;
                if (old_vol - volume).abs() > 1e-4 {
                    state.commands.execute(
                        Box::new(crate::commands::SetTrackVolume { track_id: id, old_value: old_vol, new_value: volume }),
                        &mut state.project,
                    );
                }
            }"""

vol_track_lanes_new = """\
            if vol_changed {
                let mut targets = state.selected_tracks.clone();
                targets.insert(id);
                for tid in &targets {
                    if let Some(t) = state.project.tracks.iter_mut().find(|tx| tx.id == *tid) {
                        t.volume = volume;
                        state.dirty = true;
                    }
                }
            }
            if input.mouse_released && input.drag_widget == WidgetId::Slider(id * 10) {
                let old_vol = input.drag_start_value as f32;
                if (old_vol - volume).abs() > 1e-4 {
                    let mut targets = state.selected_tracks.clone();
                    targets.insert(id);
                    let mut cmds: Vec<Box<dyn crate::commands::Command>> = Vec::new();
                    for tid in &targets {
                        cmds.push(Box::new(crate::commands::SetTrackVolume {
                            track_id: *tid, old_value: old_vol, new_value: volume
                        }));
                    }
                    if cmds.len() == 1 {
                        state.commands.execute(cmds.remove(0), &mut state.project);
                    } else {
                        state.commands.execute(Box::new(crate::commands::CompositeCommand { desc: "Set Multi Volume".into(), cmds }), &mut state.project);
                    }
                }
            }"""

code = code.replace(vol_track_lanes_old, vol_track_lanes_new)

# I can apply similar logic for PAN, MUTE, SOLO...
# But MUTE and SOLO logic operates differently, it triggers instantly on click.

# So for MUTE in track lanes:
mute_old = """\
        if mute_clicked {
            if let Some(t) = state.project.tracks.iter().find(|t| t.id == id) {
                state.commands.execute(
                    Box::new(crate::commands::SetTrackMute {
                        track_id: id,
                        new_value: !t.mute,
                        old_value: t.mute,
                    }),
                    &mut state.project,
                );
            }
        }"""

mute_new = """\
        if mute_clicked {
            let mut targets = state.selected_tracks.clone();
            targets.insert(id);
            let mut cmds: Vec<Box<dyn crate::commands::Command>> = Vec::new();
            for tid in &targets {
                if let Some(t) = state.project.tracks.iter().find(|tx| tx.id == *tid) {
                    cmds.push(Box::new(crate::commands::SetTrackMute {
                        track_id: *tid, new_value: !mute, old_value: t.mute,
                    }));
                }
            }
            if cmds.len() == 1 {
                state.commands.execute(cmds.remove(0), &mut state.project);
            } else {
                state.commands.execute(Box::new(crate::commands::CompositeCommand { desc: "Set Multi Mute".into(), cmds }), &mut state.project);
            }
        }"""
code = code.replace(mute_old, mute_new)

with open(file_path, "w") as f:
    f.write(code)

print("Patched basic volume/mute")
