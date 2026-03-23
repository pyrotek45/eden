import re

file_path = "src/views.rs"
with open(file_path, "r") as f:
    code = f.read()

old_code = """\
        }
    }

    // Handle clip drag (move / resize)"""

new_code = """\
        }
    }

    let add_btn_y = y2 + 10;
    if add_btn_y < top + state.track_area_height() && add_btn_y + 30 > top {
        let hw = state.arrangement.track_header_width;
        let clicked = button(canvas, input, &state.theme, &ButtonParams {
            id: WidgetId::Button(3000),
            x: 10, y: add_btn_y, width: hw - 20, height: 24,
            label: "ADD TRACK".into(), toggled: false, icon: ButtonIcon::None,
        });
        if clicked {
            let id = state.project.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
            let new_track = crate::models::Track::new(id, "New Track".into(), crate::models::TrackType::Midi);
            state.project.tracks.push(new_track);
            state.dirty = true;
        }
    }

    // Handle clip drag (move / resize)"""

code = code.replace(old_code, new_code)

with open(file_path, "w") as f:
    f.write(code)

print("Add Track button added")
