// Eden DAW — Views: bottom_panel

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::audio_editor::draw_audio_editor;
use super::automation_editor::draw_automation_editor;
use super::mixer::{draw_bottom_mixer, draw_instrument_rack, draw_master_rack};
use super::piano_roll::draw_piano_roll_at;
use crate::input::InputState;
use crate::state::*;
use crate::theme::Theme;
use crate::widgets::*;

pub fn draw_bottom_panel(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    let w = state.window_width as i32;
    let total_h = state.window_height as i32;
    let panel_h = state.bottom_panel_effective_h();
    let panel_y = total_h - panel_h;
    let handle_h = state.bottom_panel_handle_h();

    // ── Handle (drag zone + tab bar) ──
    let handle_hover = input.mouse_in_rect(0, panel_y, w, handle_h + 4);
    let handle_bg = if handle_hover || state.bottom_panel_dragging {
        // Slightly lighter when hovered to indicate it's draggable
        let bg = state.theme.panel_bg;
        sdl2::pixels::Color::RGBA(
            (bg[0] as i32 + 15).min(255) as u8,
            (bg[1] as i32 + 15).min(255) as u8,
            (bg[2] as i32 + 15).min(255) as u8,
            bg[3],
        )
    } else {
        Theme::c(state.theme.panel_bg)
    };
    canvas.set_draw_color(handle_bg);
    let _ = canvas.fill_rect(Rect::new(0, panel_y, w as u32, handle_h as u32));

    // Grip dots in center of handle — highlight zone when hovered for double-click
    {
        let gx = w / 2 - 20;
        let dots_hover = input.mouse_in_rect(w / 2 - 30, panel_y, 60, handle_h);
        let dot_color = if dots_hover {
            Theme::c(state.theme.accent)
        } else {
            Theme::c(state.theme.text_dim)
        };
        canvas.set_draw_color(dot_color);
        for i in 0..5 {
            let _ = canvas.fill_rect(Rect::new(gx + i * 10, panel_y + handle_h / 2 - 1, 3, 3));
        }
    }

    // Top border line of handle
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, panel_y),
        sdl2::rect::Point::new(w, panel_y),
    );

    // Close button FIRST (before drag logic, so click isn't swallowed by drag)
    let close_clicked = if state.bottom_panel_open {
        let close_x = w - 28;
        let close_y = panel_y + 3;
        let __auto_id_25 = input.next_id();
        button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_25,
                x: close_x,
                y: close_y,
                width: 22,
                height: 16,
                label: "X".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Close panel".into()),
                ..Default::default()
            },
        )
    } else {
        false
    };
    if close_clicked {
        state.bottom_panel_open = false;
        state.focused_panel = crate::state::FocusedPanel::Arrangement;
    }

    // Drag to resize (only when close wasn't clicked AND mouse is not over a tab button)
    let close_area = input.mouse_in_rect(w - 30, panel_y, 30, handle_h + 4);
    let tab_w = 56i32;
    let tab_h = handle_h - 4;
    let tab_start_x = 6i32;
    let num_tabs = 4i32;
    let tabs_total_w = num_tabs * (tab_w + 3);
    let in_tab_area = input.mouse_in_rect(tab_start_x, panel_y, tabs_total_w, handle_h);

    // Double-click on handle: maximize / restore / close / open
    // Single-click on dots zone (when closed): open halfway
    // Both actions only fire if the mouse didn't travel far (i.e., it wasn't a drag).
    let dots_zone = input.mouse_in_rect(w / 2 - 30, panel_y, 60, handle_h);
    let handle_click_zone = handle_hover && !close_area && !in_tab_area;
    // On press: remember click type (click_type is cleared by begin_frame before release fires)
    if handle_click_zone && input.mouse_pressed && !input.consumed {
        state.bottom_panel_click_type = input.click_type;
    }
    // On release: act if it wasn't a drag (single) or regardless of drift (double)
    if handle_click_zone && input.mouse_released && !input.consumed {
        if state.bottom_panel_click_type == Some(crate::input::ClickType::Double) {
            input.consumed = true;
            if state.bottom_panel_open {
                // Open (any height) → maximize to top (X button handles closing)
                state.bottom_panel_height = state.bottom_panel_max_h();
            } else {
                // Closed + double-click → open fully (maximized)
                state.bottom_panel_height = state.bottom_panel_max_h();
                state.bottom_panel_open = true;
            }
        } else if state.bottom_panel_click_type == Some(crate::input::ClickType::Single)
            && !input.dragging
            && dots_zone
            && !state.bottom_panel_open
        {
            // Closed + single-click on dots zone (no drag) → open halfway
            input.consumed = true;
            state.bottom_panel_height = 600;
            state.bottom_panel_open = true;
        }
    }

    // Drag to resize: start drag anywhere on the handle (including dots zone)
    if handle_hover && !close_area && !in_tab_area && input.mouse_pressed {
        state.bottom_panel_dragging = true;
    }
    if state.bottom_panel_dragging && input.mouse_down {
        let new_h = total_h - input.mouse_y;
        let min_h = handle_h + 60;
        if new_h > handle_h + 20 {
            state.bottom_panel_open = true;
            state.bottom_panel_height = new_h.clamp(min_h, state.bottom_panel_max_h());
        } else {
            state.bottom_panel_open = false;
            state.focused_panel = crate::state::FocusedPanel::Arrangement;
        }
    }
    if !input.mouse_down {
        state.bottom_panel_dragging = false;
    }

    // Tab buttons (in the handle) — drawn AFTER drag logic so they draw on top
    let tabs = [
        (BottomPanelTab::Mixer, "MIXER"),
        (BottomPanelTab::PianoRoll, "EDIT"),
        (BottomPanelTab::InstrumentRack, "RACK"),
        (BottomPanelTab::MasterRack, "MASTER"),
    ];
    for (i, (tab, label)) in tabs.iter().enumerate() {
        let tx = tab_start_x + i as i32 * (tab_w + 3);
        let ty = panel_y + 2;
        let active = state.bottom_panel_tab == *tab;
        let __auto_id_26 = input.next_id();
        let clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_26,
                x: tx,
                y: ty,
                width: tab_w,
                height: tab_h,
                label: label.to_string(),
                toggled: active,
                icon: ButtonIcon::None,
                hint: Some(format!("Open {} panel", label)),

                ..Default::default()
            },
        );
        if clicked {
            state.bottom_panel_tab = *tab;
            if !state.bottom_panel_open {
                state.bottom_panel_open = true;
                state.bottom_panel_height = 280;
            }
        }
    }

    // Consume any unhandled press on the handle so it never bleeds through
    // to the loop ruler behind it when the panel is dragged up over the ruler.
    if handle_hover && input.mouse_pressed && !input.consumed {
        input.consumed = true;
    }

    // ── Panel content ──
    if state.bottom_panel_open {
        let content_y = panel_y + handle_h;
        let content_h = panel_h - handle_h;

        canvas.set_draw_color(Theme::c(state.theme.bg_dark));
        let _ = canvas.fill_rect(Rect::new(0, content_y, w as u32, content_h as u32));
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(0, content_y),
            sdl2::rect::Point::new(w, content_y),
        );

        match state.bottom_panel_tab {
            BottomPanelTab::Mixer => {
                draw_bottom_mixer(canvas, input, state, content_y, w, content_h);
            }
            BottomPanelTab::PianoRoll => {
                // Route to the right editor based on selected clip type
                let clip_type = state.selected_clip.and_then(|(tid, ci)| {
                    state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == tid)
                        .and_then(|t| t.clips.get(ci))
                        .map(|c| match c {
                            crate::models::Clip::Midi(_) => 0,
                            crate::models::Clip::Audio(_) => 1,
                            crate::models::Clip::Automation(_) => 2,
                        })
                });
                match clip_type {
                    Some(1) => {
                        draw_audio_editor(canvas, input, state, content_y, w, content_h);
                    }
                    Some(2) => {
                        draw_automation_editor(canvas, input, state, content_y, w, content_h);
                    }
                    _ => {
                        // MIDI clip or no selection: piano roll
                        draw_piano_roll_at(canvas, input, state, 0, content_y, w, content_h);
                    }
                }
            }
            BottomPanelTab::InstrumentRack => {
                draw_instrument_rack(canvas, input, state, content_y, w, content_h);
            }
            BottomPanelTab::MasterRack => {
                draw_master_rack(canvas, input, state, content_y, w, content_h);
            }
        }
    }
}

// ── Helper: dB-scaled meter color (module-level) ──
