// Eden DAW — Views: edit

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::audio_editor::draw_audio_editor;
use super::automation_editor::draw_automation_editor;
use super::clip_manager::draw_clip_manager;
use super::piano_roll::draw_piano_roll_at;
use super::transport::{draw_mode_tabs, draw_transport};
use crate::app::input::InputState;
use crate::app::state::*;
use crate::theme::Theme;

pub fn draw_edit(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    draw_transport(canvas, input, state);
    draw_mode_tabs(canvas, input, state);

    let top = state.transport_bar_height() + state.mode_tab_height();
    let w = state.window_width as i32;
    let h = state.window_height as i32 - top;

    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // ── Clip manager sidebar (left 200px) ──
    let sidebar_w = 200i32;
    draw_clip_manager(canvas, input, state, top, sidebar_w, h);

    // Vertical separator
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(sidebar_w, top),
        sdl2::rect::Point::new(sidebar_w, top + h),
    );

    // ── Main editor area (right of sidebar) ──
    let editor_x = sidebar_w + 1;
    let editor_w = w - editor_x;

    let clip_type = state.selected_clip.and_then(|(tid, ci)| {
        state
            .project
            .tracks
            .iter()
            .find(|t| t.id == tid)
            .and_then(|t| t.clips.get(ci))
            .map(|c| match c {
                crate::app::models::Clip::Midi(_) => 0,
                crate::app::models::Clip::Audio(_) => 1,
                crate::app::models::Clip::Automation(_) => 2,
            })
    });

    // Clip the canvas to editor area (approximate using offsets passed to drawing functions)
    match clip_type {
        Some(1) => draw_audio_editor(canvas, input, state, top, w, h),
        Some(2) => draw_automation_editor(canvas, input, state, top, w, h),
        _ => {
            // For piano roll (or no selection): fill editor area, then draw piano roll
            if clip_type.is_some() {
                // Offset mouse coords so piano roll starts at editor_x
                draw_piano_roll_at(canvas, input, state, editor_x, top, editor_w, h);
            } else {
                // No clip selected placeholder in editor area
                canvas.set_draw_color(Theme::c(state.theme.text_dim));
                let cx = editor_x + editor_w / 2;
                let _ = canvas.fill_rect(Rect::new(cx - 80, top + h / 2 - 2, 160, 4));
            }
        }
    }
    let _ = editor_x;
}
