import re

file_path = "src/views.rs"
with open(file_path, "r") as f:
    code = f.read()

code = code.replace("let add_btn_y = y2 + 10;", """\
    let mut add_btn_y = top;
    let track_count = state.project.tracks.len() as i32;
    add_btn_y = top - scroll_y + track_count * 50;""")
code = code.replace("        let hw = state.arrangement.track_header_width;", "        let hw = state.arrangement.track_header_width;")

# Fix crate::widgets::InputState -> crate::input::InputState
code = code.replace("crate::widgets::InputState", "crate::input::InputState")

# Fix crate::widgets::WidgetId -> crate::input::WidgetId
code = code.replace("crate::widgets::WidgetId::Knob(4001)", "crate::input::WidgetId::Knob(4001)")
code = code.replace("crate::widgets::WidgetId::Knob(4002)", "crate::input::WidgetId::Knob(4002)")

# Fix drag_ghost_text
old_drop = """\
    let is_drop = in_sampler && !input.mouse_down && input.drag_ghost_text.is_some();
    if is_drop {
        if let Some(ghost) = input.drag_ghost_text.take() {
            // Note: simple sampler data wouldn't actually play unless sent to audio engine
            // But visually acknowledge:
            println!("Dropped sample into Sampler: {}", ghost);
        }
    }"""
new_drop = """\
    let is_drop = in_sampler && !input.mouse_down && state.sample_drag_idx.is_some();
    if is_drop {
        if let Some(drag_idx) = state.sample_drag_idx.take() {
            println!("Dropped sample into Sampler: index {}", drag_idx);
        }
    }"""
code = code.replace(old_drop, new_drop)

with open(file_path, "w") as f:
    f.write(code)

print("Fixed views")
