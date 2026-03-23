import re

file_path = "src/main.rs"
with open(file_path, "r") as f:
    code = f.read()

replacement = """\
                        Keycode::T if !input.ctrl() => {
                            state.next_theme();
                        }
                        Keycode::Up if input.shift() => {
                            if let Some(id) = state.selected_track {
                                if let Some(i) = state.project.tracks.iter().position(|t| t.id == id) {
                                    if i > 0 {
                                        state.project.tracks.swap(i, i - 1);
                                        state.dirty = true;
                                    }
                                }
                            }
                        }
                        Keycode::Down if input.shift() => {
                            if let Some(id) = state.selected_track {
                                if let Some(i) = state.project.tracks.iter().position(|t| t.id == id) {
                                    if i + 1 < state.project.tracks.len() {
                                        state.project.tracks.swap(i, i + 1);
                                        state.dirty = true;
                                    }
                                }
                            }
                        }"""

code = code.replace("""\
                        Keycode::T if !input.ctrl() => {
                            state.next_theme();
                        }""", replacement)

with open(file_path, "w") as f:
    f.write(code)
    
print("Reorder tracks patched.")
