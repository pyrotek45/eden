import re

file_path = "src/views.rs"
with open(file_path, "r") as f:
    code = f.read()

old_call = """\
            BottomPanelTab::InstrumentRack => {
                draw_instrument_rack(canvas, state, content_y, w, content_h);
            }"""

new_call = """\
            BottomPanelTab::InstrumentRack => {
                draw_instrument_rack(canvas, input, state, content_y, w, content_h);
            }"""

code = code.replace(old_call, new_call)

old_def = """\
fn draw_instrument_rack(
    canvas: &mut Canvas<Window>,
    state: &AppState,
    top: i32, w: i32, h: i32,
) {
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // Placeholder slots
    for i in 0..4 {
        let slot_y = top + 8 + i * 44;
        let slot_h = 40;
        canvas.set_draw_color(Theme::c(state.theme.panel_bg));
        let _ = canvas.fill_rect(Rect::new(8, slot_y, (w - 16) as u32, slot_h as u32));
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_rect(Rect::new(8, slot_y, (w - 16) as u32, slot_h as u32));
        draw_pixel_label(canvas, &state.theme, "— empty slot —", 24, slot_y + 16, 120,
            sdl2::pixels::Color::RGBA(90, 90, 90, 180));
    }
    draw_pixel_label(canvas, &state.theme, "INSTRUMENT RACK", w / 2 - 50, top + h - 14, 120,
        sdl2::pixels::Color::RGBA(70, 70, 80, 200));
}"""

new_def = """\
fn draw_instrument_rack(
    canvas: &mut Canvas<Window>,
    input: &mut crate::widgets::InputState,
    state: &mut AppState,
    top: i32, w: i32, h: i32,
) {
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    let rack_label = if let Some(tid) = state.selected_track {
        format!("INSTRUMENT RACK - TRACK {}", tid)
    } else {
        "INSTRUMENT RACK - NO TRACK SELECTED".into()
    };
    draw_pixel_label(canvas, &state.theme, &rack_label, 8, top + 6, 200, Theme::c(state.theme.text_secondary));

    // Draw the Sampler UI
    let rx = 10;
    let ry = top + 24;
    let rw = 300;
    let rh = h - 34;

    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(rx, ry, rw as u32, rh as u32));
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_rect(Rect::new(rx, ry, rw as u32, rh as u32));
    let _ = canvas.draw_rect(Rect::new(rx+1, ry+1, (rw-2) as u32, (rh-2) as u32));

    draw_pixel_label(canvas, &state.theme, "SIMPLE SAMPLER", rx + 10, ry + 10, 120, Theme::c(state.theme.text_primary));

    // Dropping a sample from browser
    let in_sampler = input.mouse_in_rect(rx, ry, rw, rh);
    let is_drop = in_sampler && !input.mouse_down && input.drag_ghost_text.is_some();
    if is_drop {
        if let Some(ghost) = input.drag_ghost_text.take() {
            // Note: simple sampler data wouldn't actually play unless sent to audio engine
            // But visually acknowledge:
            println!("Dropped sample into Sampler: {}", ghost);
        }
    }
    
    // Show a waveform placeholder or standard parameters
    draw_pixel_label(canvas, &state.theme, "Drag sample here", rx + 10, ry + 40, 120, sdl2::pixels::Color::RGBA(120, 120, 130, 200));

    let mut attack = 0.01;
    let mut release = 0.5;
    crate::widgets::knob(canvas, input, &state.theme, &crate::widgets::KnobParams {
        id: crate::widgets::WidgetId::Knob(4001),
        x: rx + 50, y: ry + rh - 40,
        radius: 14, min: 0.0, max: 2.0, sensitivity: 0.01,
        label: Some("ATTACK".into()), bipolar: false, default_value: Some(0.01),
    }, &mut attack);
    
    crate::widgets::knob(canvas, input, &state.theme, &crate::widgets::KnobParams {
        id: crate::widgets::WidgetId::Knob(4002),
        x: rx + 110, y: ry + rh - 40,
        radius: 14, min: 0.0, max: 5.0, sensitivity: 0.02,
        label: Some("REL".into()), bipolar: false, default_value: Some(0.5),
    }, &mut release);
}"""

code = code.replace(old_def, new_def)

with open(file_path, "w") as f:
    f.write(code)

print("Instrument rack patched")
