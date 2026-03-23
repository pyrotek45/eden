import re

file_path = "src/views.rs"
with open(file_path, "r") as f:
    code = f.read()

# Fix the integer truncation truncation...
code = code.replace("input.drag_start_y = orig_start as i32; // store original start using Y just to save memory", "state.drag_original_positions.insert((track_id, clip_idx as usize), orig_start);")
code = code.replace("let orig_start = input.drag_start_y as f64;", "let orig_start = state.drag_original_positions.get(&(track_id, clip_idx)).copied().unwrap_or(0.0);")

with open(file_path, "w") as f:
    f.write(code)

print("Fixed floating point bug")
