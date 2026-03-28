// Eden DAW — Views: overlays

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::{InputState, WidgetId};
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

/// Draw all popup overlays that must appear above everything else.
/// Input routing is handled by the UiLayer state machine in draw_arrangement —
/// by the time this function is called, `input` is already the real input
/// (background layers were given dead_input). No restore/block logic needed here.
pub(super) fn draw_overlays(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    canvas.set_clip_rect(None);

    let layer = state.active_layer();

    // Dropdown for snap resolution — only when no popup is shadowing it
    if layer == crate::app::state::UiLayer::Base && state.dropdown_open_id == 200 {
        let snap_labels: Vec<&str> = SNAP_RESOLUTIONS.iter().map(|(l, _)| *l).collect();
        let mut snap_idx = state.snap.resolution_idx;
        let changed = dropdown(
            canvas,
            input,
            &state.theme,
            200,
            424,
            10,
            52,
            28,
            &snap_labels,
            &mut snap_idx,
            &mut state.dropdown_open_id,
        );
        if changed {
            state.snap.resolution_idx = snap_idx;
        }
    }

    // Clear any hover hints set by background widgets before drawing the active
    // popup so no background tooltip bleeds through the popup backdrop.
    if layer > crate::app::state::UiLayer::Base {
        input.hover_hint_text = None;
        input.hover_hint_widget = crate::app::input::WidgetId::None;
        input.hot_widget = crate::app::input::WidgetId::None;
    }

    // Project settings / Options popups (Popup layer)
    if state.project_popup_open {
        draw_project_popup(canvas, input, state);
    }
    if state.options_open {
        draw_options_popup(canvas, input, state);
    }

    // Render popup (RenderDialog layer — above Popup)
    if state.render_popup_open {
        if state.file_browser_open {
            let mut dead = InputState {
                mouse_x: input.mouse_x,
                mouse_y: input.mouse_y,
                mouse_down: false,
                ..Default::default()
            };
            draw_render_popup(canvas, &mut dead, state);
        } else {
            draw_render_popup(canvas, input, state);
        }
    }

    // New-project name prompt popup
    if state.new_project_popup_open {
        draw_new_project_popup(canvas, input, state);
    }

    // Save As popup
    if state.save_as_popup_open {
        draw_save_as_popup(canvas, input, state);
    }

    // Audio Export popup
    if state.audio_export_popup_open {
        if state.file_browser_open {
            // Draw visually but block input when file browser is on top
            let mut dead = InputState {
                mouse_x: input.mouse_x,
                mouse_y: input.mouse_y,
                mouse_down: false,
                ..Default::default()
            };
            draw_audio_export_popup(canvas, &mut dead, state);
        } else {
            draw_audio_export_popup(canvas, input, state);
        }
    }

    // MIDI Export popup
    if state.midi_export_popup_open {
        if state.file_browser_open {
            let mut dead = InputState {
                mouse_x: input.mouse_x,
                mouse_y: input.mouse_y,
                mouse_down: false,
                ..Default::default()
            };
            draw_midi_export_popup(canvas, &mut dead, state);
        } else {
            draw_midi_export_popup(canvas, input, state);
        }
    }

    // Generic file browser popup (drawn on top of export popups)
    if state.file_browser_open {
        if let Some(selected_path) = draw_file_browser_popup(canvas, input, state) {
            match state.file_browser_caller {
                Some(crate::app::state::FileBrowserCaller::AudioExportDir) => {
                    state.audio_export_dir = selected_path.to_string_lossy().to_string();
                    // Update text field buffer if directory field is active
                    if state.text_field_active_id == 303 {
                        state.text_field_buffer = state.audio_export_dir.clone();
                        state.text_field_cursor = state.text_field_buffer.len();
                    }
                }
                Some(crate::app::state::FileBrowserCaller::MidiExportDir) => {
                    state.midi_export_dir = selected_path.to_string_lossy().to_string();
                    if state.text_field_active_id == 304 {
                        state.text_field_buffer = state.midi_export_dir.clone();
                        state.text_field_cursor = state.text_field_buffer.len();
                    }
                }
                Some(crate::app::state::FileBrowserCaller::OpenProject) => {
                    // Handled in draw_home_screen
                }
                Some(crate::app::state::FileBrowserCaller::RenderExportDir) => {
                    state.render_export_dir = selected_path.to_string_lossy().to_string();
                    if state.text_field_active_id == 88002 {
                        state.text_field_buffer = state.render_export_dir.clone();
                        state.text_field_cursor = state.text_field_buffer.len();
                    }
                }
                None => {}
            }
            state.file_browser_caller = None;
        }
    }

    // ── Confirmation dialogs (ConfirmDialog layer — highest priority) ─────────
    if let Some(del_idx) = state.clip_lib_confirm_delete {
        canvas.set_clip_rect(None);
        let clip_name = if del_idx < state.clip_library.len() {
            state.clip_library[del_idx].1.name().to_string()
        } else {
            "Clip".to_string()
        };
        let dlg_w = 260i32;
        let dlg_h = 100i32;
        let dlg_x = (state.window_width as i32 - dlg_w) / 2;
        let dlg_y = (state.window_height as i32 - dlg_h) / 2;

        // Dimmed background — full screen backdrop eats all clicks outside dialog
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 160));
        let _ = canvas.fill_rect(Rect::new(0, 0, state.window_width, state.window_height));

        // Dialog shadow
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(dlg_x + 3, dlg_y + 3, dlg_w as u32, dlg_h as u32));

        // Dialog box
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 54, 255));
        let _ = canvas.fill_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, dlg_h as u32));

        // Title bar
        canvas.set_draw_color(Theme::c(state.theme.bg_light));
        let _ = canvas.fill_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, 22));

        draw_pixel_label(
            canvas,
            &state.theme,
            "CONFIRM DELETE",
            dlg_x + 8,
            dlg_y + 8,
            dlg_w - 16,
            Theme::c(state.theme.text_primary),
        );

        // Accent border
        canvas.set_draw_color(Theme::c(state.theme.accent));
        let _ = canvas.draw_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, dlg_h as u32));

        // Message
        let msg = format!("Delete \"{}\"?", clip_name);
        draw_pixel_label(
            canvas,
            &state.theme,
            &msg,
            dlg_x + 10,
            dlg_y + 32,
            dlg_w - 20,
            sdl2::pixels::Color::RGBA(220, 220, 230, 255),
        );

        // Buttons — use stable WidgetIds (not auto-IDs) so they work reliably
        let btn_w = 90i32;
        let btn_h = 26i32;
        let btn_y = dlg_y + dlg_h - btn_h - 10;
        let yes_x = dlg_x + dlg_w / 2 - btn_w - 6;
        let no_x = dlg_x + dlg_w / 2 + 6;

        let yes_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(60000),
                x: yes_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
                label: "Delete".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: None,
                ..Default::default()
            },
        );
        let no_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(60001),
                x: no_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
                label: "Cancel".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: None,
                ..Default::default()
            },
        );

        // Escape key cancels
        let escape_pressed = input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::Escape);
        // Click outside dialog cancels (only unprocessed clicks)
        let clicked_outside = input.mouse_pressed
            && !input.consumed
            && !input.mouse_in_rect(dlg_x, dlg_y, dlg_w, dlg_h);

        if yes_clicked {
            state.clip_lib_confirm_execute = true;
            state.clip_lib_confirmed_idx = Some(del_idx);
            state.clip_lib_confirm_delete = None;
        } else if no_clicked || escape_pressed || clicked_outside {
            state.clip_lib_confirm_delete = None;
        }
    }

    // ── Track delete confirmation dialog ──
    if let Some((track_id, track_index)) = state.track_confirm_delete {
        canvas.set_clip_rect(None);
        let track_name = state
            .project
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .map(|t| t.name.clone())
            .unwrap_or_else(|| format!("Track {}", track_id));
        let dlg_w = 300i32;
        let dlg_h = 100i32;
        let dlg_x = (state.window_width as i32 - dlg_w) / 2;
        let dlg_y = (state.window_height as i32 - dlg_h) / 2;

        // Full-screen dimmed backdrop
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 160));
        let _ = canvas.fill_rect(Rect::new(0, 0, state.window_width, state.window_height));

        // Dialog shadow
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(dlg_x + 3, dlg_y + 3, dlg_w as u32, dlg_h as u32));

        // Dialog background
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 54, 255));
        let _ = canvas.fill_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, dlg_h as u32));

        // Title bar
        canvas.set_draw_color(Theme::c(state.theme.bg_light));
        let _ = canvas.fill_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, 22));
        draw_pixel_label(
            canvas,
            &state.theme,
            "CONFIRM DELETE",
            dlg_x + 8,
            dlg_y + 8,
            dlg_w - 16,
            Theme::c(state.theme.text_primary),
        );

        // Accent border
        canvas.set_draw_color(Theme::c(state.theme.accent));
        let _ = canvas.draw_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, dlg_h as u32));

        let msg = format!("Delete track \"{}\"?", track_name);
        draw_pixel_label(
            canvas,
            &state.theme,
            &msg,
            dlg_x + 10,
            dlg_y + 32,
            dlg_w - 20,
            sdl2::pixels::Color::RGBA(220, 220, 230, 255),
        );

        let btn_w = 90i32;
        let btn_h = 26i32;
        let btn_y = dlg_y + dlg_h - btn_h - 10;
        let yes_x = dlg_x + dlg_w / 2 - btn_w - 6;
        let no_x = dlg_x + dlg_w / 2 + 6;

        let yes_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(60010),
                x: yes_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
                label: "Delete".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: None,
                ..Default::default()
            },
        );
        let no_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(60011),
                x: no_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
                label: "Cancel".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: None,
                ..Default::default()
            },
        );

        let escape_pressed = input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::Escape);
        let clicked_outside = input.mouse_pressed
            && !input.consumed
            && !input.mouse_in_rect(dlg_x, dlg_y, dlg_w, dlg_h);

        if yes_clicked {
            let id = track_id;
            state.commands.execute(
                Box::new(crate::app::commands::RemoveTrack {
                    track_id: id,
                    removed_track: None,
                    index: track_index,
                }),
                &mut state.project,
            );
            if state.selected_track == Some(id) {
                state.selected_track = None;
            }
            state.selected_tracks.remove(&id);
            state.selected_clips.retain(|&(tid, _)| tid != id);
            if state.selected_clip.map(|(tid, _)| tid) == Some(id) {
                state.selected_clip = None;
            }
            state.dirty = true;
            state.track_confirm_delete = None;
        } else if no_clicked || escape_pressed || clicked_outside {
            state.track_confirm_delete = None;
        }
    }

    // ── Multi-track delete confirmation dialog ──
    if let Some(ref ids_to_delete) = state.track_confirm_multi_delete.clone() {
        canvas.set_clip_rect(None);
        let count = ids_to_delete.len();
        let dlg_w = 320i32;
        let dlg_h = 100i32;
        let dlg_x = (state.window_width as i32 - dlg_w) / 2;
        let dlg_y = (state.window_height as i32 - dlg_h) / 2;

        // Full-screen dimmed backdrop
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 160));
        let _ = canvas.fill_rect(Rect::new(0, 0, state.window_width, state.window_height));

        // Dialog shadow
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(dlg_x + 3, dlg_y + 3, dlg_w as u32, dlg_h as u32));

        // Dialog background
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 54, 255));
        let _ = canvas.fill_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, dlg_h as u32));

        // Title bar
        canvas.set_draw_color(Theme::c(state.theme.bg_light));
        let _ = canvas.fill_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, 22));
        draw_pixel_label(
            canvas,
            &state.theme,
            "CONFIRM DELETE",
            dlg_x + 8,
            dlg_y + 8,
            dlg_w - 16,
            Theme::c(state.theme.text_primary),
        );

        // Accent border
        canvas.set_draw_color(Theme::c(state.theme.accent));
        let _ = canvas.draw_rect(Rect::new(dlg_x, dlg_y, dlg_w as u32, dlg_h as u32));

        let msg = format!("Delete {} selected tracks?", count);
        draw_pixel_label(
            canvas,
            &state.theme,
            &msg,
            dlg_x + 10,
            dlg_y + 32,
            dlg_w - 20,
            sdl2::pixels::Color::RGBA(220, 220, 230, 255),
        );

        let btn_w = 100i32;
        let btn_h = 26i32;
        let btn_y = dlg_y + dlg_h - btn_h - 10;
        let yes_x = dlg_x + dlg_w / 2 - btn_w - 6;
        let no_x = dlg_x + dlg_w / 2 + 6;

        let md_yes = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(60020),
                x: yes_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
                label: "Delete All".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: None,
                ..Default::default()
            },
        );
        let md_no = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(60021),
                x: no_x,
                y: btn_y,
                width: btn_w,
                height: btn_h,
                label: "Cancel".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: None,
                ..Default::default()
            },
        );

        let escape_pressed = input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::Escape);
        let clicked_outside =
            input.mouse_pressed && !input.mouse_in_rect(dlg_x, dlg_y, dlg_w, dlg_h);

        if md_yes {
            // Snapshot-based undo for multi-track delete
            let snapshot = state.project.clone();
            let ids = ids_to_delete.clone();
            state.project.tracks.retain(|t| !ids.contains(&t.id));
            state
                .commands
                .push_undo_snapshot(snapshot, "Delete Tracks (multi)");
            for &tid in &ids {
                state.selected_tracks.remove(&tid);
                state.selected_clips.retain(|&(t, _)| t != tid);
                if state.selected_track == Some(tid) {
                    state.selected_track = None;
                }
                if state.selected_clip.map(|(t, _)| t) == Some(tid) {
                    state.selected_clip = None;
                }
            }
            state.dirty = true;
            state.track_confirm_multi_delete = None;
        } else if md_no || escape_pressed || clicked_outside {
            state.track_confirm_multi_delete = None;
        }
    }
}

fn draw_project_popup(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let w = state.window_width as i32;
    let popup_w = 400i32;
    let popup_h = 360i32;
    let popup_x = w / 2 - popup_w / 2;
    let popup_y = 60i32;

    // Dimmed backdrop
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, state.window_height));

    // Panel shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(
        popup_x + 4,
        popup_y + 4,
        popup_w as u32,
        popup_h as u32,
    ));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Panel border
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Title bar
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, 22));
    draw_pixel_label(
        canvas,
        &state.theme,
        "PROJECT SETTINGS",
        popup_x + 10,
        popup_y + 8,
        popup_w - 40,
        Theme::c(state.theme.text_primary),
    );

    // Close button
    let __auto_id_43 = input.next_id();
    let close_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_43,
            x: popup_x + popup_w - 24,
            y: popup_y + 2,
            width: 20,
            height: 18,
            label: "X".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Close project settings".into()),
            ..Default::default()
        },
    );
    if close_clicked {
        state.project_popup_open = false;
    }

    let lx = popup_x + 14;
    let vx = popup_x + 130;
    let rw = popup_w - 144;
    let mut ry = popup_y + 36;

    // ── Project Name ──
    draw_pixel_label(
        canvas,
        &state.theme,
        "Name",
        lx,
        ry + 3,
        100,
        Theme::c(state.theme.text_secondary),
    );
    let (committed, new_val) = text_field(
        canvas,
        input,
        &state.theme,
        &TextFieldParams {
            id: 200,
            x: vx,
            y: ry,
            width: rw,
            height: 18,
            hint: Some("Project name".into()),
        },
        &state.project.name,
        &mut state.text_field_active_id,
        &mut state.text_field_buffer,
        &mut state.text_field_cursor,
    );
    if committed {
        if let Some(new_name) = new_val {
            let trimmed = new_name.trim().to_string();
            if !trimmed.is_empty() {
                let old_name = state.project.name.clone();
                state.commands.execute(
                    Box::new(crate::app::commands::SetProjectName {
                        old_name,
                        new_name: trimmed,
                    }),
                    &mut state.project,
                );
                state.dirty = true;
            }
        }
    }
    ry += 30;

    // ── BPM ──
    draw_pixel_label(
        canvas,
        &state.theme,
        "BPM",
        lx,
        ry + 3,
        100,
        Theme::c(state.theme.text_secondary),
    );
    let mut bpm = state.project.tempo_map.bpm_at(0.0) as f32;
    let bpm_slider_w = rw - 50; // leave room for the value label
    let __auto_id_44 = input.next_id();
    let bpm_changed = slider(
        canvas,
        input,
        &state.theme,
        &SliderParams {
            id: __auto_id_44,
            x: vx,
            y: ry,
            width: bpm_slider_w,
            height: 14,
            min: 20.0,
            max: 300.0,
            orientation: SliderOrientation::Horizontal,
            label: None,
            default_value: Some(120.0),
        },
        &mut bpm,
    );
    if bpm_changed {
        let bpm_val = bpm as f64;
        // Update the first tempo entry
        if let Some(entry) = state.project.tempo_map.changes.first_mut() {
            entry.bpm = bpm_val;
        }
    }
    let bpm_text = format!("{:.0}", bpm);
    draw_pixel_label(
        canvas,
        &state.theme,
        &bpm_text,
        vx + bpm_slider_w + 6,
        ry + 3,
        44,
        Theme::c(state.theme.text_primary),
    );
    ry += 28;

    // ── Time Signature ──
    draw_pixel_label(
        canvas,
        &state.theme,
        "Time Sig",
        lx,
        ry + 3,
        100,
        Theme::c(state.theme.text_secondary),
    );
    let ts_text = format!(
        "{}/{}",
        state.project.time_signature.0, state.project.time_signature.1
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        &ts_text,
        vx,
        ry + 3,
        rw,
        Theme::c(state.theme.text_primary),
    );
    ry += 28;

    // ── Track Count ──
    draw_pixel_label(
        canvas,
        &state.theme,
        "Tracks",
        lx,
        ry + 3,
        100,
        Theme::c(state.theme.text_secondary),
    );
    let midi_count = state
        .project
        .tracks
        .iter()
        .filter(|t| t.track_type == crate::app::models::TrackType::Midi)
        .count();
    let audio_count = state
        .project
        .tracks
        .iter()
        .filter(|t| t.track_type == crate::app::models::TrackType::Audio)
        .count();
    let auto_count = state
        .project
        .tracks
        .iter()
        .filter(|t| t.track_type == crate::app::models::TrackType::Automation)
        .count();
    let tracks_text = format!(
        "{} total ({} MIDI, {} Audio, {} Auto)",
        state.project.tracks.len(),
        midi_count,
        audio_count,
        auto_count
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        &tracks_text,
        vx,
        ry + 3,
        rw,
        Theme::c(state.theme.text_primary),
    );
    ry += 28;

    // ── Sample Rate ──
    draw_pixel_label(
        canvas,
        &state.theme,
        "Sample Rate",
        lx,
        ry + 3,
        110,
        Theme::c(state.theme.text_secondary),
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        "44100 Hz",
        vx,
        ry + 3,
        rw,
        Theme::c(state.theme.text_primary),
    );
    ry += 28;

    // ── Project File ──
    draw_pixel_label(
        canvas,
        &state.theme,
        "File",
        lx,
        ry + 3,
        100,
        Theme::c(state.theme.text_secondary),
    );
    let file_text = if let Some(ref p) = state.last_save_path {
        p.clone()
    } else {
        "(unsaved)".to_string()
    };
    draw_pixel_label(
        canvas,
        &state.theme,
        &file_text,
        vx,
        ry + 3,
        rw,
        Theme::c(state.theme.text_primary),
    );
    ry += 28;

    // ── Save / Save As buttons ──
    let save_btn_w2 = (rw - 6) / 2;
    let __auto_id_proj_save = input.next_id();
    let proj_save_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_proj_save,
            x: vx,
            y: ry,
            width: save_btn_w2,
            height: 24,
            label: "Save".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Save project (Ctrl+S)".into()),
            ..Default::default()
        },
    );
    if proj_save_clicked {
        match state.quick_save() {
            Ok(()) => println!("[save] Project saved"),
            Err(e) => eprintln!("[save] Error: {}", e),
        }
    }
    let __auto_id_proj_saveas = input.next_id();
    let proj_saveas_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_proj_saveas,
            x: vx + save_btn_w2 + 6,
            y: ry,
            width: save_btn_w2,
            height: 24,
            label: "Save As".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Save project to a new file".into()),
            ..Default::default()
        },
    );
    if proj_saveas_clicked {
        let default_name = if let Some(ref p) = state.last_save_path {
            p.clone()
        } else {
            format!("{}.eden.json", state.project.name)
        };
        state.save_as_name_buffer = default_name;
        state.save_as_popup_open = true;
    }

    // Click outside to close
    if input.mouse_pressed
        && !input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h)
        && input.active_widget == WidgetId::None
    {
        state.project_popup_open = false;
    }
}

/// Generic file browser popup.
/// Returns `Some(path)` when a file/directory was selected, `None` otherwise.
/// The popup is self-contained: it reads/writes `state.file_browser_*` fields.
pub fn draw_file_browser_popup(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) -> Option<std::path::PathBuf> {
    let w = state.window_width as i32;
    let h = state.window_height as i32;

    // Dim background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 160));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    // Browser panel
    let bw = 500i32;
    let bh = 440i32;
    let bx = (w - bw) / 2;
    let by = (h - bh) / 2;

    // Shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(bx + 4, by + 4, bw as u32, bh as u32));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(bx, by, bw as u32, bh as u32));
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_rect(Rect::new(bx, by, bw as u32, bh as u32));

    // Accent top bar
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.fill_rect(Rect::new(bx, by, bw as u32, 3));

    // Title
    let title = state.file_browser_title.clone();
    draw_pixel_label(
        canvas,
        &state.theme,
        &title,
        bx + 16,
        by + 10,
        bw - 80,
        Theme::c(state.theme.text_primary),
    );

    // Close button (X)
    let close_id = input.next_id();
    let close_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: close_id,
            x: bx + bw - 30,
            y: by + 6,
            width: 22,
            height: 18,
            label: "✕".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Close".into()),
            ..Default::default()
        },
    );

    // Current path label
    let path_str = state.file_browser_path.to_string_lossy().to_string();
    let display_path = if path_str.len() > 60 {
        format!("...{}", &path_str[path_str.len() - 60..])
    } else {
        path_str
    };
    draw_pixel_label(
        canvas,
        &state.theme,
        &display_path,
        bx + 16,
        by + 30,
        bw - 32,
        Theme::c(state.theme.text_secondary),
    );

    // Back button
    let back_id = input.next_id();
    let back_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: back_id,
            x: bx + 16,
            y: by + 44,
            width: 80,
            height: 20,
            label: "← Back".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Go to parent folder".into()),
            ..Default::default()
        },
    );
    if back_clicked {
        if let Some(parent) = state.file_browser_path.parent() {
            state.file_browser_path = parent.to_path_buf();
            state.refresh_file_browser();
        }
    }

    // "Select this folder" button (only when in dir-selection mode)
    let select_dir = state.file_browser_select_dir;
    if select_dir {
        let sel_id = input.next_id();
        let sel_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: sel_id,
                x: bx + bw - 150,
                y: by + 44,
                width: 134,
                height: 20,
                label: "Select Folder".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Use this folder".into()),
                ..Default::default()
            },
        );
        if sel_clicked {
            let selected = state.file_browser_path.clone();
            state.file_browser_open = false;
            input.consumed = true;
            return Some(selected);
        }
    }

    // File listing
    let list_y = by + 70;
    let list_h = bh - 80;
    let row_h = 24i32;
    let visible_rows = list_h / row_h;

    // Scroll
    if input.mouse_in_rect(bx, list_y, bw, list_h) && input.scroll_y != 0 && !input.scroll_consumed
    {
        state.file_browser_scroll -= input.scroll_y * 3;
        let total = state.file_browser_entries.len() as i32;
        state.file_browser_scroll = state
            .file_browser_scroll
            .max(0)
            .min((total - visible_rows).max(0));
        input.scroll_consumed = true;
    }

    let entries = state.file_browser_entries.clone();
    let scroll = state.file_browser_scroll;

    if entries.is_empty() {
        let msg = if select_dir {
            "No sub-folders found"
        } else {
            "No matching files found"
        };
        draw_pixel_label(
            canvas,
            &state.theme,
            msg,
            bx + 20,
            list_y + 16,
            bw - 40,
            sdl2::pixels::Color::RGBA(80, 85, 100, 180),
        );
    }

    let mut result: Option<std::path::PathBuf> = None;

    for (i, (name, path, is_dir)) in entries.iter().enumerate().skip(scroll as usize) {
        let row_idx = i as i32 - scroll;
        if row_idx >= visible_rows {
            break;
        }
        let ry = list_y + row_idx * row_h;

        let is_hovered = input.mouse_in_rect(bx + 4, ry, bw - 8, row_h - 2);

        if is_hovered {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 70, 200));
            let _ = canvas.fill_rect(Rect::new(bx + 4, ry, (bw - 8) as u32, (row_h - 2) as u32));
        }

        // Icon + name
        let icon = if *is_dir { "▸" } else { "♫" };
        let col = if *is_dir {
            Theme::c(state.theme.text_secondary)
        } else {
            Theme::c(state.theme.accent)
        };
        draw_pixel_label(canvas, &state.theme, icon, bx + 12, ry + 6, 14, col);
        draw_pixel_label(canvas, &state.theme, name, bx + 28, ry + 6, bw - 48, col);

        // Click handling
        if is_hovered && input.mouse_pressed && !input.consumed {
            if *is_dir {
                state.file_browser_path = path.clone();
                state.refresh_file_browser();
            } else {
                // File selected
                result = Some(path.clone());
            }
            input.consume();
        }
    }

    // Escape to close
    let esc_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Escape);

    if close_clicked || esc_pressed {
        state.file_browser_open = false;
        input.consumed = true;
        return None;
    }

    // Click outside the browser panel to dismiss
    if input.mouse_pressed && !input.consumed && !input.mouse_in_rect(bx, by, bw, bh) {
        state.file_browser_open = false;
        input.consume();
        return None;
    }

    if result.is_some() {
        state.file_browser_open = false;
    }

    // Block all input from passing through
    input.consumed = true;

    result
}

pub(super) fn draw_new_project_popup(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    let w = state.window_width as i32;
    let h = state.window_height as i32;
    let popup_w = 320i32;
    let popup_h = 130i32;
    let popup_x = w / 2 - popup_w / 2;
    let popup_y = h / 2 - popup_h / 2;

    // Dimmed backdrop
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    // Panel shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(
        popup_x + 4,
        popup_y + 4,
        popup_w as u32,
        popup_h as u32,
    ));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Panel border
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Title
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, 22));
    draw_pixel_label(
        canvas,
        &state.theme,
        "NEW PROJECT",
        popup_x + 10,
        popup_y + 8,
        popup_w - 20,
        Theme::c(state.theme.text_primary),
    );

    let lx = popup_x + 14;
    let vx = popup_x + 80;
    let rw = popup_w - 94;
    let ry = popup_y + 38;

    // Name label + text field
    draw_pixel_label(
        canvas,
        &state.theme,
        "Name",
        lx,
        ry + 3,
        60,
        Theme::c(state.theme.text_secondary),
    );
    let (committed, new_val) = text_field(
        canvas,
        input,
        &state.theme,
        &TextFieldParams {
            id: 300,
            x: vx,
            y: ry,
            width: rw,
            height: 20,
            hint: Some("Project name".into()),
        },
        &state.new_project_name_buffer.clone(),
        &mut state.text_field_active_id,
        &mut state.text_field_buffer,
        &mut state.text_field_cursor,
    );
    if committed {
        if let Some(new_name) = new_val {
            let trimmed = new_name.trim().to_string();
            if !trimmed.is_empty() {
                state.new_project_name_buffer = trimmed;
            }
        }
    }

    // Auto-activate the text field on first open
    if state.text_field_active_id == 0 {
        state.text_field_active_id = 300;
        state.text_field_buffer = state.new_project_name_buffer.clone();
        state.text_field_cursor = state.text_field_buffer.len();
    }

    // Buttons: Create / Cancel
    let btn_y = ry + 36;
    let btn_w = (rw - 6) / 2;

    let __auto_id_np_create = input.next_id();
    let create_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_np_create,
            x: vx,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Create".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Create new project with this name".into()),
            ..Default::default()
        },
    );

    let __auto_id_np_cancel = input.next_id();
    let cancel_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_np_cancel,
            x: vx + btn_w + 6,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Cancel".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Cancel".into()),
            ..Default::default()
        },
    );

    // Also accept Enter as Create, Escape as Cancel
    let enter_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Return)
        || input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::KpEnter);
    let esc_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Escape);

    if create_clicked || enter_pressed {
        // Commit any pending text field edit
        let name = if state.text_field_active_id == 300 {
            let s = state.text_field_buffer.trim().to_string();
            if s.is_empty() {
                state.new_project_name_buffer.clone()
            } else {
                s
            }
        } else {
            state.new_project_name_buffer.clone()
        };
        state.project = crate::app::models::Project::default();
        state.project.name = name;
        state.last_save_path = None;
        state.dirty = false;
        state.commands = crate::app::commands::CommandManager::new(1000);
        state.mode = crate::app::state::AppMode::Arrangement;
        state.new_project_popup_open = false;
        state.text_field_active_id = 0;
        state.push_status("New project created");
    }

    if cancel_clicked || esc_pressed {
        state.new_project_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Block clicks outside popup (closing it)
    if input.mouse_pressed
        && !input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h)
        && input.active_widget == WidgetId::None
    {
        state.new_project_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Block all input from passing through
    input.consumed = true;
}

fn draw_save_as_popup(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let w = state.window_width as i32;
    let h = state.window_height as i32;
    let popup_w = 400i32;
    let popup_h = 130i32;
    let popup_x = w / 2 - popup_w / 2;
    let popup_y = h / 2 - popup_h / 2;

    // Dimmed backdrop
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    // Panel shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(
        popup_x + 4,
        popup_y + 4,
        popup_w as u32,
        popup_h as u32,
    ));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Panel border
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Title
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, 22));
    draw_pixel_label(
        canvas,
        &state.theme,
        "SAVE AS",
        popup_x + 10,
        popup_y + 8,
        popup_w - 20,
        Theme::c(state.theme.text_primary),
    );

    let lx = popup_x + 14;
    let vx = popup_x + 80;
    let rw = popup_w - 94;
    let ry = popup_y + 38;

    // File path label + text field
    draw_pixel_label(
        canvas,
        &state.theme,
        "File",
        lx,
        ry + 3,
        60,
        Theme::c(state.theme.text_secondary),
    );
    let (committed, new_val) = text_field(
        canvas,
        input,
        &state.theme,
        &TextFieldParams {
            id: 301,
            x: vx,
            y: ry,
            width: rw,
            height: 20,
            hint: Some("filename.eden.json".into()),
        },
        &state.save_as_name_buffer.clone(),
        &mut state.text_field_active_id,
        &mut state.text_field_buffer,
        &mut state.text_field_cursor,
    );
    if committed {
        if let Some(new_name) = new_val {
            let trimmed = new_name.trim().to_string();
            if !trimmed.is_empty() {
                state.save_as_name_buffer = trimmed;
            }
        }
    }

    // Auto-activate the text field on first open
    if state.text_field_active_id == 0 {
        state.text_field_active_id = 301;
        state.text_field_buffer = state.save_as_name_buffer.clone();
        state.text_field_cursor = state.text_field_buffer.len();
    }

    // Buttons: Save / Cancel
    let btn_y = ry + 36;
    let btn_w = (rw - 6) / 2;

    let __auto_id_sa_save = input.next_id();
    let save_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_sa_save,
            x: vx,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Save".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Save to this file".into()),
            ..Default::default()
        },
    );

    let __auto_id_sa_cancel = input.next_id();
    let cancel_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_sa_cancel,
            x: vx + btn_w + 6,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Cancel".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Cancel".into()),
            ..Default::default()
        },
    );

    // Also accept Enter as Save, Escape as Cancel
    let enter_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Return)
        || input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::KpEnter);
    let esc_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Escape);

    if save_clicked || enter_pressed {
        // Commit any pending text field edit
        let path = if state.text_field_active_id == 301 {
            let s = state.text_field_buffer.trim().to_string();
            if s.is_empty() {
                state.save_as_name_buffer.clone()
            } else {
                s
            }
        } else {
            state.save_as_name_buffer.clone()
        };
        // Ensure .eden.json extension
        let path = if !path.ends_with(".eden.json") {
            format!("{}.eden.json", path)
        } else {
            path
        };
        match state.save_project(&path) {
            Ok(()) => println!("[save-as] Saved to {}", path),
            Err(e) => eprintln!("[save-as] Error: {}", e),
        }
        state.save_as_popup_open = false;
        state.text_field_active_id = 0;
    }

    if cancel_clicked || esc_pressed {
        state.save_as_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Block clicks outside popup (closing it)
    if input.mouse_pressed
        && !input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h)
        && input.active_widget == WidgetId::None
    {
        state.save_as_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Block all input from passing through
    input.consumed = true;
}

fn draw_audio_export_popup(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    let w = state.window_width as i32;
    let h = state.window_height as i32;
    let popup_w = 500i32;
    let popup_h = 170i32;
    let popup_x = w / 2 - popup_w / 2;
    let popup_y = h / 2 - popup_h / 2;

    // Dimmed backdrop
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    // Panel shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(
        popup_x + 4,
        popup_y + 4,
        popup_w as u32,
        popup_h as u32,
    ));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Panel border
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Title bar
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, 22));
    draw_pixel_label(
        canvas,
        &state.theme,
        "EXPORT AUDIO",
        popup_x + 10,
        popup_y + 8,
        popup_w - 20,
        Theme::c(state.theme.text_primary),
    );

    let lx = popup_x + 14;
    let vx = popup_x + 80;
    let rw = popup_w - 94;
    let browse_w = 60i32;
    let field_rw = rw - browse_w - 4;
    let row0_y = popup_y + 34;
    let row_h = 28;

    // ── Row 0: Directory ──────────────────────────────────────────
    draw_pixel_label(
        canvas,
        &state.theme,
        "Folder",
        lx,
        row0_y + 3,
        60,
        Theme::c(state.theme.text_secondary),
    );
    let (dir_committed, dir_new_val) = text_field(
        canvas,
        input,
        &state.theme,
        &TextFieldParams {
            id: 303,
            x: vx,
            y: row0_y,
            width: field_rw,
            height: 20,
            hint: Some("/path/to/export/directory".into()),
        },
        &state.audio_export_dir.clone(),
        &mut state.text_field_active_id,
        &mut state.text_field_buffer,
        &mut state.text_field_cursor,
    );
    // Browse button
    let browse_id = input.next_id();
    let browse_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: browse_id,
            x: vx + field_rw + 4,
            y: row0_y,
            width: browse_w,
            height: 20,
            label: "Browse".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Browse for folder".into()),
            ..Default::default()
        },
    );
    if browse_clicked {
        let dir_clone = state.audio_export_dir.clone();
        let start = std::path::PathBuf::from(&dir_clone);
        let start_path = if start.is_dir() { Some(start) } else { None };
        state.open_file_browser(
            crate::app::state::FileBrowserCaller::AudioExportDir,
            "Select Export Folder",
            "",
            true,
            start_path.as_deref(),
        );
    }
    if dir_committed {
        if let Some(new_dir) = dir_new_val {
            let trimmed = new_dir.trim().to_string();
            if !trimmed.is_empty() {
                state.audio_export_dir = trimmed;
            }
        }
    }

    // ── Row 1: File name ──────────────────────────────────────────
    let row1_y = row0_y + row_h;
    draw_pixel_label(
        canvas,
        &state.theme,
        "Name",
        lx,
        row1_y + 3,
        60,
        Theme::c(state.theme.text_secondary),
    );
    let (committed, new_val) = text_field(
        canvas,
        input,
        &state.theme,
        &TextFieldParams {
            id: 302,
            x: vx,
            y: row1_y,
            width: rw,
            height: 20,
            hint: Some("filename.wav".into()),
        },
        &state.audio_export_name.clone(),
        &mut state.text_field_active_id,
        &mut state.text_field_buffer,
        &mut state.text_field_cursor,
    );
    if committed {
        if let Some(new_name) = new_val {
            let trimmed = new_name.trim().to_string();
            if !trimmed.is_empty() {
                state.audio_export_name = trimmed;
            }
        }
    }

    // Auto-activate directory text field on first open
    if state.text_field_active_id == 0 {
        state.text_field_active_id = 303;
        state.text_field_buffer = state.audio_export_dir.clone();
        state.text_field_cursor = state.text_field_buffer.len();
    }

    // ── Row 2: Preview path ───────────────────────────────────────
    let row2_y = row1_y + row_h;
    let preview_dir = if state.text_field_active_id == 303 {
        state.text_field_buffer.clone()
    } else {
        state.audio_export_dir.clone()
    };
    let preview_name = if state.text_field_active_id == 302 {
        state.text_field_buffer.clone()
    } else {
        state.audio_export_name.clone()
    };
    let preview_name = if preview_name.is_empty() {
        state.audio_export_name.clone()
    } else {
        preview_name
    };
    let preview_path = std::path::Path::new(&preview_dir).join(&preview_name);
    let preview_str = format!("→ {}", preview_path.display());
    draw_pixel_label(
        canvas,
        &state.theme,
        &preview_str,
        vx,
        row2_y + 3,
        rw,
        Theme::c(state.theme.text_secondary),
    );

    // ── Row 3: Buttons ────────────────────────────────────────────
    let btn_y = row2_y + row_h - 4;
    let btn_w = (rw - 6) / 2;

    let export_btn_id = input.next_id();
    let export_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: export_btn_id,
            x: vx,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Export".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Export audio to WAV file".into()),
            ..Default::default()
        },
    );

    let cancel_btn_id = input.next_id();
    let cancel_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: cancel_btn_id,
            x: vx + btn_w + 6,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Cancel".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Cancel export".into()),
            ..Default::default()
        },
    );
    // Fallback: also detect a raw single-frame click on the cancel area
    // (text-field deactivation can occasionally swallow the press/release pair)
    let cancel_raw = input.mouse_pressed && input.mouse_in_rect(vx + btn_w + 6, btn_y, btn_w, 26);

    // Accept Enter as Export, Escape as Cancel
    let enter_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Return)
        || input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::KpEnter);
    let esc_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Escape);

    if export_clicked || enter_pressed {
        // Get the final directory from the text field
        let export_dir = if state.text_field_active_id == 303 {
            let s = state.text_field_buffer.trim().to_string();
            if s.is_empty() {
                state.audio_export_dir.clone()
            } else {
                state.audio_export_dir = s.clone();
                s
            }
        } else {
            state.audio_export_dir.clone()
        };
        // Get the final filename from the text field
        let export_name = if state.text_field_active_id == 302 {
            let s = state.text_field_buffer.trim().to_string();
            if s.is_empty() {
                state.audio_export_name.clone()
            } else {
                state.audio_export_name = s.clone();
                s
            }
        } else {
            state.audio_export_name.clone()
        };
        // Ensure .wav extension
        let export_name = if !export_name.to_lowercase().ends_with(".wav") {
            format!("{}.wav", export_name)
        } else {
            export_name
        };
        // Build export path from directory + filename
        let export_path = std::path::Path::new(&export_dir).join(&export_name);
        // Create directory if it doesn't exist
        if let Some(parent) = export_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Copy the source file to the export path
        match std::fs::copy(&state.audio_export_source, &export_path) {
            Ok(_) => {
                state.push_status(format!("Exported to {}", export_path.display()));
                println!("[audio-export] Exported to {}", export_path.display());
            }
            Err(e) => {
                state.push_status(format!("Export failed: {}", e));
                eprintln!("[audio-export] Error: {}", e);
            }
        }
        state.audio_export_popup_open = false;
        state.text_field_active_id = 0;
    }

    if cancel_clicked || cancel_raw || esc_pressed {
        state.audio_export_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Click outside popup to dismiss
    if input.mouse_pressed && !input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h) {
        state.audio_export_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Block all input from passing through
    input.consumed = true;
}

fn draw_midi_export_popup(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    let w = state.window_width as i32;
    let h = state.window_height as i32;
    let popup_w = 500i32;
    let popup_h = 170i32;
    let popup_x = w / 2 - popup_w / 2;
    let popup_y = h / 2 - popup_h / 2;

    // Dimmed backdrop
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    // Panel shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(
        popup_x + 4,
        popup_y + 4,
        popup_w as u32,
        popup_h as u32,
    ));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Panel border
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Title bar
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, 22));
    draw_pixel_label(
        canvas,
        &state.theme,
        "EXPORT MIDI",
        popup_x + 10,
        popup_y + 8,
        popup_w - 20,
        Theme::c(state.theme.text_primary),
    );

    let lx = popup_x + 14;
    let vx = popup_x + 80;
    let rw = popup_w - 94;
    let browse_w = 60i32;
    let field_rw = rw - browse_w - 4;
    let row0_y = popup_y + 34;
    let row_h = 28;

    // ── Row 0: Directory ──────────────────────────────────────────
    draw_pixel_label(
        canvas,
        &state.theme,
        "Folder",
        lx,
        row0_y + 3,
        60,
        Theme::c(state.theme.text_secondary),
    );
    let (dir_committed, dir_new_val) = text_field(
        canvas,
        input,
        &state.theme,
        &TextFieldParams {
            id: 304,
            x: vx,
            y: row0_y,
            width: field_rw,
            height: 20,
            hint: Some("/path/to/export/directory".into()),
        },
        &state.midi_export_dir.clone(),
        &mut state.text_field_active_id,
        &mut state.text_field_buffer,
        &mut state.text_field_cursor,
    );
    // Browse button
    let midi_browse_id = input.next_id();
    let midi_browse_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: midi_browse_id,
            x: vx + field_rw + 4,
            y: row0_y,
            width: browse_w,
            height: 20,
            label: "Browse".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Browse for folder".into()),
            ..Default::default()
        },
    );
    if midi_browse_clicked {
        let dir_clone = state.midi_export_dir.clone();
        let start = std::path::PathBuf::from(&dir_clone);
        let start_path = if start.is_dir() { Some(start) } else { None };
        state.open_file_browser(
            crate::app::state::FileBrowserCaller::MidiExportDir,
            "Select Export Folder",
            "",
            true,
            start_path.as_deref(),
        );
    }
    if dir_committed {
        if let Some(new_dir) = dir_new_val {
            let trimmed = new_dir.trim().to_string();
            if !trimmed.is_empty() {
                state.midi_export_dir = trimmed;
            }
        }
    }

    // ── Row 1: File name ──────────────────────────────────────────
    let row1_y = row0_y + row_h;
    draw_pixel_label(
        canvas,
        &state.theme,
        "Name",
        lx,
        row1_y + 3,
        60,
        Theme::c(state.theme.text_secondary),
    );
    let (name_committed, name_new_val) = text_field(
        canvas,
        input,
        &state.theme,
        &TextFieldParams {
            id: 305,
            x: vx,
            y: row1_y,
            width: rw,
            height: 20,
            hint: Some("filename.mid".into()),
        },
        &state.midi_export_name.clone(),
        &mut state.text_field_active_id,
        &mut state.text_field_buffer,
        &mut state.text_field_cursor,
    );
    if name_committed {
        if let Some(new_name) = name_new_val {
            let trimmed = new_name.trim().to_string();
            if !trimmed.is_empty() {
                state.midi_export_name = trimmed;
            }
        }
    }

    // Auto-activate directory text field on first open
    if state.text_field_active_id == 0 {
        state.text_field_active_id = 304;
        state.text_field_buffer = state.midi_export_dir.clone();
        state.text_field_cursor = state.text_field_buffer.len();
    }

    // ── Row 2: Preview path ───────────────────────────────────────
    let row2_y = row1_y + row_h;
    let preview_dir = if state.text_field_active_id == 304 {
        state.text_field_buffer.clone()
    } else {
        state.midi_export_dir.clone()
    };
    let preview_name = if state.text_field_active_id == 305 {
        state.text_field_buffer.clone()
    } else {
        state.midi_export_name.clone()
    };
    let preview_name = if preview_name.is_empty() {
        state.midi_export_name.clone()
    } else {
        preview_name
    };
    let preview_path = std::path::Path::new(&preview_dir).join(&preview_name);
    let preview_str = format!("→ {}", preview_path.display());
    draw_pixel_label(
        canvas,
        &state.theme,
        &preview_str,
        vx,
        row2_y + 3,
        rw,
        Theme::c(state.theme.text_secondary),
    );

    // ── Row 3: Buttons ────────────────────────────────────────────
    let btn_y = row2_y + row_h - 4;
    let btn_w = (rw - 6) / 2;

    let export_btn_id = input.next_id();
    let export_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: export_btn_id,
            x: vx,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Export".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Export MIDI file".into()),
            ..Default::default()
        },
    );

    let cancel_btn_id = input.next_id();
    let cancel_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: cancel_btn_id,
            x: vx + btn_w + 6,
            y: btn_y,
            width: btn_w,
            height: 26,
            label: "Cancel".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Cancel export".into()),
            ..Default::default()
        },
    );
    // Fallback: also detect a raw single-frame click on the cancel area
    let cancel_raw = input.mouse_pressed && input.mouse_in_rect(vx + btn_w + 6, btn_y, btn_w, 26);

    // Accept Enter as Export, Escape as Cancel
    let enter_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Return)
        || input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::KpEnter);
    let esc_pressed = input
        .keys_pressed
        .contains(&sdl2::keyboard::Keycode::Escape);

    if export_clicked || enter_pressed {
        // Get the final directory from the text field
        let export_dir = if state.text_field_active_id == 304 {
            let s = state.text_field_buffer.trim().to_string();
            if s.is_empty() {
                state.midi_export_dir.clone()
            } else {
                state.midi_export_dir = s.clone();
                s
            }
        } else {
            state.midi_export_dir.clone()
        };
        // Get the final filename from the text field
        let export_name = if state.text_field_active_id == 305 {
            let s = state.text_field_buffer.trim().to_string();
            if s.is_empty() {
                state.midi_export_name.clone()
            } else {
                state.midi_export_name = s.clone();
                s
            }
        } else {
            state.midi_export_name.clone()
        };
        // Ensure .mid extension
        let export_name = if !export_name.to_lowercase().ends_with(".mid") {
            format!("{}.mid", export_name)
        } else {
            export_name
        };
        // Build export path
        let export_path = std::path::Path::new(&export_dir).join(&export_name);
        // Create directory if it doesn't exist
        if let Some(parent) = export_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Export the selected clip
        let bpm = state.project.tempo_map.bpm_at(0.0);
        let mut export_result: Result<(), String> = Err("No MIDI clip selected".to_string());
        if let Some((tid, ci)) = state.selected_clip {
            if let Some(track) = state.project.tracks.iter().find(|t| t.id == tid) {
                if let Some(crate::app::models::Clip::Midi(m)) = track.clips.get(ci) {
                    let clip_name = if m.name.is_empty() {
                        format!("clip_{}", ci)
                    } else {
                        m.name.clone()
                    };
                    export_result = crate::app::models::export_midi_file(
                        m,
                        &export_path.to_string_lossy(),
                        bpm,
                        &clip_name,
                    );
                }
            }
        }
        match export_result {
            Ok(()) => {
                state.push_status(format!("MIDI exported to {}", export_path.display()));
                println!("[midi-export] Exported to {}", export_path.display());
            }
            Err(e) => {
                state.push_status(format!("MIDI export failed: {}", e));
                eprintln!("[midi-export] Error: {}", e);
            }
        }
        state.midi_export_popup_open = false;
        state.text_field_active_id = 0;
    }

    if cancel_clicked || cancel_raw || esc_pressed {
        state.midi_export_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Click outside popup to dismiss
    if input.mouse_pressed && !input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h) {
        state.midi_export_popup_open = false;
        state.text_field_active_id = 0;
    }

    // Block all input from passing through
    input.consumed = true;
}

fn draw_options_popup(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let w = state.window_width as i32;
    let popup_w = 340i32;
    let popup_h = 560i32;
    let popup_x = w / 2 - popup_w / 2;
    let popup_y = 60i32;

    // Dimmed backdrop (semi-transparent overlay over everything)
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, state.window_height));

    // Panel shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(
        popup_x + 4,
        popup_y + 4,
        popup_w as u32,
        popup_h as u32,
    ));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Panel border
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Title bar
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, 22));
    draw_pixel_label(
        canvas,
        &state.theme,
        "OPTIONS",
        popup_x + 10,
        popup_y + 8,
        popup_w - 40,
        Theme::c(state.theme.text_primary),
    );

    // Close button
    let __auto_id_45 = input.next_id();
    let close_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_45,
            x: popup_x + popup_w - 24,
            y: popup_y + 2,
            width: 20,
            height: 18,
            label: "X".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Close options".into()),
            ..Default::default()
        },
    );
    if close_clicked {
        state.options_open = false;
    }

    // ── UI Scale section ──
    let row_y = popup_y + 36;
    draw_pixel_label(
        canvas,
        &state.theme,
        "UI SCALE",
        popup_x + 12,
        row_y + 4,
        100,
        Theme::c(state.theme.text_secondary),
    );

    // Scale value display — shows pending value
    let scale_text = format!("{:.0}%", state.ui_scale_pending * 100.0);
    draw_pixel_label(
        canvas,
        &state.theme,
        &scale_text,
        popup_x + popup_w - 60,
        row_y + 4,
        50,
        Theme::c(state.theme.text_primary),
    );

    // Scale slider — edits pending value only
    let mut scale_val = state.ui_scale_pending;
    let __auto_id_46 = input.next_id();
    let _scale_changed = slider(
        canvas,
        input,
        &state.theme,
        &SliderParams {
            id: __auto_id_46,
            x: popup_x + 12,
            y: row_y + 16,
            width: popup_w - 24,
            height: 14,
            min: 0.75,
            max: 3.0,
            orientation: SliderOrientation::Horizontal,
            label: None,
            default_value: Some(1.0),
        },
        &mut scale_val,
    );
    state.ui_scale_pending = scale_val;

    // Scale presets
    let presets = [
        ("75%", 0.75f32),
        ("100%", 1.0),
        ("125%", 1.25),
        ("150%", 1.5),
        ("200%", 2.0),
    ];
    let btn_w = 46i32;
    let btn_gap = 6i32;
    let total_btns_w = presets.len() as i32 * (btn_w + btn_gap) - btn_gap;
    let btns_start = popup_x + (popup_w - total_btns_w) / 2;
    for (i, (label, val)) in presets.iter().enumerate() {
        let bx = btns_start + i as i32 * (btn_w + btn_gap);
        let by = row_y + 36;
        let active = (state.ui_scale_pending - val).abs() < 0.01;
        let __auto_id_47 = input.next_id();
        let clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_47,
                x: bx,
                y: by,
                width: btn_w,
                height: 20,
                label: label.to_string(),
                toggled: active,
                icon: ButtonIcon::None,
                hint: Some(format!("Set UI scale to {}", label)),

                ..Default::default()
            },
        );
        if clicked {
            state.ui_scale_pending = *val;
        }
    }

    // Apply button — commits pending scale
    let __auto_id_48 = input.next_id();
    let apply_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_48,
            x: popup_x + (popup_w - 80) / 2,
            y: row_y + 62,
            width: 80,
            height: 20,
            label: "Apply".to_string(),
            toggled: (state.ui_scale - state.ui_scale_pending).abs() > 0.001,
            icon: ButtonIcon::None,
            hint: Some("Apply scale change".into()),
            ..Default::default()
        },
    );
    if apply_clicked {
        state.ui_scale = state.ui_scale_pending;
    }

    // ── Font Scale section ──
    let font_row_y = row_y + 92;
    draw_pixel_label(
        canvas,
        &state.theme,
        "FONT SCALE",
        popup_x + 12,
        font_row_y + 4,
        100,
        Theme::c(state.theme.text_secondary),
    );
    let font_scale_labels = ["S", "M", "L", "XL"];
    let font_scale_values = [1i32, 2, 3, 4];
    let fbtn_w = 46i32;
    let fbtn_gap = 6i32;
    let ftotal_w = font_scale_labels.len() as i32 * (fbtn_w + fbtn_gap) - fbtn_gap;
    let fbtns_start = popup_x + (popup_w - ftotal_w) / 2;
    for (i, (label, val)) in font_scale_labels
        .iter()
        .zip(font_scale_values.iter())
        .enumerate()
    {
        let bx = fbtns_start + i as i32 * (fbtn_w + fbtn_gap);
        let by = font_row_y + 16;
        let active = state.font_scale_pending == *val;
        let __auto_id_49 = input.next_id();
        let clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_49,
                x: bx,
                y: by,
                width: fbtn_w,
                height: 20,
                label: label.to_string(),
                toggled: active,
                icon: ButtonIcon::None,
                hint: Some(format!("Font size {}", label)),

                ..Default::default()
            },
        );
        if clicked {
            state.font_scale_pending = *val;
        }
    }
    // Font scale Apply button
    let __auto_id_50 = input.next_id();
    let font_apply_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_50,
            x: popup_x + (popup_w - 80) / 2,
            y: font_row_y + 42,
            width: 80,
            height: 20,
            label: "Apply".to_string(),
            toggled: state.font_scale != state.font_scale_pending,
            icon: ButtonIcon::None,
            hint: Some("Apply font scale change".into()),
            ..Default::default()
        },
    );
    if font_apply_clicked {
        state.font_scale = state.font_scale_pending;
    }

    // ── Snap grid info ──
    let row3_y = popup_y + 202;
    let snap_label = SNAP_RESOLUTIONS[state.snap.resolution_idx].0;
    let snap_info = format!(
        "SNAP: {} (ENABLED: {})",
        snap_label,
        if state.snap.enabled { "YES" } else { "NO" }
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        &snap_info,
        popup_x + 12,
        row3_y + 4,
        popup_w - 24,
        Theme::c(state.theme.text_dim),
    );

    // ── Follow playhead toggle ──
    let follow_y = row3_y + 22;
    let follow_id = input.next_id();
    let follow_clicked = toggle_button(
        canvas,
        input,
        &state.theme,
        popup_x + 12,
        follow_y,
        18,
        state.theme.accent,
        state.follow_playhead,
        follow_id,
        if state.follow_playhead { "Y" } else { "N" },
        Some("Toggle follow playhead"),
    );
    if follow_clicked {
        state.follow_playhead = !state.follow_playhead;
    }
    draw_pixel_label(
        canvas,
        &state.theme,
        "FOLLOW PLAYHEAD",
        popup_x + 36,
        follow_y + 4,
        popup_w - 48,
        Theme::c(state.theme.text_secondary),
    );

    // ── Autosave section ──
    let autosave_y = follow_y + 28;
    let autosave_toggle_id = input.next_id();
    let autosave_clicked = toggle_button(
        canvas,
        input,
        &state.theme,
        popup_x + 12,
        autosave_y,
        18,
        state.theme.accent,
        state.autosave_enabled,
        autosave_toggle_id,
        if state.autosave_enabled { "Y" } else { "N" },
        Some("Toggle autosave"),
    );
    if autosave_clicked {
        state.autosave_enabled = !state.autosave_enabled;
        if state.autosave_enabled {
            let (_, secs) = crate::app::config::AUTOSAVE_INTERVALS[state.autosave_interval_idx];
            state.autosave_countdown = secs * 60;
        }
        state.config_dirty = true;
        state.config_save_countdown = 60;
    }
    draw_pixel_label(
        canvas,
        &state.theme,
        "AUTOSAVE",
        popup_x + 36,
        autosave_y + 4,
        popup_w - 48,
        Theme::c(state.theme.text_secondary),
    );

    // Autosave interval dropdown (only when enabled)
    if state.autosave_enabled {
        let interval_labels: Vec<&str> = crate::app::config::AUTOSAVE_INTERVALS
            .iter()
            .map(|(label, _)| *label)
            .collect();
        let prev_idx = state.autosave_interval_idx;
        let _changed = dropdown(
            canvas,
            input,
            &state.theme,
            996,
            popup_x + 120,
            autosave_y - 2,
            120,
            22,
            &interval_labels,
            &mut state.autosave_interval_idx,
            &mut state.dropdown_open_id,
        );
        if state.autosave_interval_idx != prev_idx {
            let (_, secs) = crate::app::config::AUTOSAVE_INTERVALS[state.autosave_interval_idx];
            state.autosave_countdown = secs * 60;
            state.config_dirty = true;
            state.config_save_countdown = 60;
        }
    }

    // ── Audio output device ──
    let row4_y = popup_y + 340;
    draw_pixel_label(
        canvas,
        &state.theme,
        "OUTPUT DEVICE",
        popup_x + 12,
        row4_y + 4,
        140,
        Theme::c(state.theme.text_secondary),
    );

    // Populate device list if empty
    if state.audio_device_names.is_empty() {
        state.audio_device_names = crate::engine::list_output_devices();
    }

    // Refresh button (drawn BEFORE dropdown so dropdown popup renders on top)
    let __auto_id_51 = input.next_id();
    let refresh_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_51,
            x: popup_x + popup_w - 80,
            y: row4_y + 44,
            width: 68,
            height: 18,
            label: "Refresh".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: None,
            ..Default::default()
        },
    );
    if refresh_clicked {
        state.audio_device_names = crate::engine::list_output_devices();
        state.audio_device_idx = 0;
    }

    // Device selector dropdown (drawn AFTER Refresh button so popup list appears on top)
    {
        let device_strs: Vec<&str> = state
            .audio_device_names
            .iter()
            .map(|s| s.as_str())
            .collect();
        if !device_strs.is_empty() {
            let prev_idx = state.audio_device_idx;
            let changed = dropdown(
                canvas,
                input,
                &state.theme,
                995,
                popup_x + 12,
                row4_y + 18,
                popup_w - 24,
                22,
                &device_strs,
                &mut state.audio_device_idx,
                &mut state.dropdown_open_id,
            );
            if changed || state.audio_device_idx != prev_idx {
                state.audio_device_changed = true;
            }
        } else {
            draw_pixel_label(
                canvas,
                &state.theme,
                "No devices found",
                popup_x + 12,
                row4_y + 22,
                popup_w - 24,
                Theme::c(state.theme.text_dim),
            );
        }
    }

    // ── Reset Audio Engine ──
    let reset_y = row4_y + 70;
    draw_pixel_label(
        canvas,
        &state.theme,
        "AUDIO ENGINE",
        popup_x + 12,
        reset_y + 4,
        140,
        Theme::c(state.theme.text_secondary),
    );
    let reset_audio_id = input.next_id();
    let reset_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: reset_audio_id,
            x: popup_x + 12,
            y: reset_y + 18,
            width: popup_w - 24,
            height: 22,
            label: "Reset Audio Engine".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Kill all voices and reset audio state (use if audio freezes)".into()),
            ..Default::default()
        },
    );
    if reset_clicked {
        state.panic_triggered = true;
    }

    // Click outside to dismiss
    let in_popup = input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h);
    if !in_popup && input.mouse_pressed {
        state.options_open = false;
    }
}

fn draw_render_popup(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let w = state.window_width as i32;
    let popup_w = 380i32;
    let popup_h = 340i32;
    let popup_x = w / 2 - popup_w / 2;
    let popup_y = 80i32;

    // ── If a render is in progress, show progress bar overlay instead ──
    if state.render_progress.is_some() {
        // Poll for completion
        let mut finished: Option<Result<String, String>> = None;
        if let Some(ref rx) = state.render_result {
            match rx.try_recv() {
                Ok(result) => {
                    finished = Some(result);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = Some(Err("Render thread disconnected".into()));
                }
            }
        }
        if let Some(result) = finished {
            match result {
                Ok(path) => state.push_status(format!("Exported to {}", path)),
                Err(e) => state.push_status(format!("Export error: {}", e)),
            }
            state.render_progress = None;
            state.render_result = None;
            state.render_popup_open = false;
            return;
        }

        // Read current progress (permille 0..1000)
        let permille = state
            .render_progress
            .as_ref()
            .map(|p| p.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0);
        let pct = permille as f64 / 10.0;

        // Dimmed backdrop
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 180));
        let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, state.window_height));

        // Progress popup (smaller)
        let prog_w = 320i32;
        let prog_h = 100i32;
        let prog_x = w / 2 - prog_w / 2;
        let prog_y = 140i32;

        // Shadow
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
        let _ = canvas.fill_rect(Rect::new(
            prog_x + 4,
            prog_y + 4,
            prog_w as u32,
            prog_h as u32,
        ));

        // Background
        canvas.set_draw_color(Theme::c(state.theme.panel_bg));
        let _ = canvas.fill_rect(Rect::new(prog_x, prog_y, prog_w as u32, prog_h as u32));

        // Border
        canvas.set_draw_color(Theme::c(state.theme.accent));
        let _ = canvas.draw_rect(Rect::new(prog_x, prog_y, prog_w as u32, prog_h as u32));

        // Label
        let label = format!("Exporting... {:.1}%", pct);
        draw_pixel_label(
            canvas,
            &state.theme,
            &label,
            prog_x + 12,
            prog_y + 14,
            prog_w - 24,
            Theme::c(state.theme.text_primary),
        );

        // Progress bar track
        let bar_x = prog_x + 16;
        let bar_y = prog_y + 48;
        let bar_w = prog_w - 32;
        let bar_h = 20i32;
        canvas.set_draw_color(Theme::c(state.theme.bg_dark));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, bar_w as u32, bar_h as u32));

        // Progress bar fill
        let fill_w = ((bar_w as f64) * (permille as f64 / 1000.0)) as i32;
        if fill_w > 0 {
            canvas.set_draw_color(Theme::c(state.theme.accent));
            let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, fill_w as u32, bar_h as u32));
        }

        // Bar border
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_rect(Rect::new(bar_x, bar_y, bar_w as u32, bar_h as u32));

        return;
    }

    // Dimmed backdrop
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, state.window_height));

    // Panel shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
    let _ = canvas.fill_rect(Rect::new(
        popup_x + 4,
        popup_y + 4,
        popup_w as u32,
        popup_h as u32,
    ));

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Panel border
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

    // Title bar
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, 22));
    draw_pixel_label(
        canvas,
        &state.theme,
        "EXPORT / RENDER",
        popup_x + 10,
        popup_y + 8,
        popup_w - 40,
        Theme::c(state.theme.text_primary),
    );

    // Close button
    let __auto_id_52 = input.next_id();
    let close_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_52,
            x: popup_x + popup_w - 24,
            y: popup_y + 2,
            width: 20,
            height: 18,
            label: "X".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Close".into()),
            ..Default::default()
        },
    );
    if close_clicked {
        state.render_popup_open = false;
    }

    let cx = popup_x + 12;
    let cw = popup_w - 24;

    // ── Filename row (editable text field) ──
    let row1 = popup_y + 32;
    draw_pixel_label(
        canvas,
        &state.theme,
        "FILENAME",
        cx,
        row1 + 4,
        100,
        Theme::c(state.theme.text_secondary),
    );
    {
        let tf_id: u32 = 88001;
        let fname_clone = state.render_filename.clone();
        let mut buf = state.text_field_buffer.clone();
        let mut cursor = state.text_field_cursor;
        let mut active_id = state.text_field_active_id;
        let (committed, new_val) = text_field(
            canvas,
            input,
            &state.theme,
            &TextFieldParams {
                id: tf_id,
                x: cx + 105,
                y: row1,
                width: cw - 105,
                height: 20,
                hint: Some("output.wav".into()),
            },
            &fname_clone,
            &mut active_id,
            &mut buf,
            &mut cursor,
        );
        state.text_field_active_id = active_id;
        state.text_field_buffer = buf;
        state.text_field_cursor = cursor;
        if committed {
            if let Some(new_name) = new_val {
                state.render_filename = new_name;
            }
        }
    }

    // ── Directory row (text field + Browse button) ──
    let row_dir = popup_y + 58;
    draw_pixel_label(
        canvas,
        &state.theme,
        "DIRECTORY",
        cx,
        row_dir + 4,
        100,
        Theme::c(state.theme.text_secondary),
    );
    {
        let tf_id: u32 = 88002;
        let dir_clone = state.render_export_dir.clone();
        let mut buf = state.text_field_buffer.clone();
        let mut cursor = state.text_field_cursor;
        let mut active_id = state.text_field_active_id;
        let (committed, new_val) = text_field(
            canvas,
            input,
            &state.theme,
            &TextFieldParams {
                id: tf_id,
                x: cx + 105,
                y: row_dir,
                width: cw - 105 - 64,
                height: 20,
                hint: Some("./".into()),
            },
            &dir_clone,
            &mut active_id,
            &mut buf,
            &mut cursor,
        );
        state.text_field_active_id = active_id;
        state.text_field_buffer = buf;
        state.text_field_cursor = cursor;
        if committed {
            if let Some(new_dir) = new_val {
                state.render_export_dir = new_dir;
            }
        }
    }
    // Browse button
    let browse_id = input.next_id();
    let browse_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: browse_id,
            x: cx + cw - 56,
            y: row_dir,
            width: 56,
            height: 20,
            label: "Browse".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Choose export directory".into()),
            ..Default::default()
        },
    );
    if browse_clicked {
        let start = if state.render_export_dir.is_empty() {
            std::env::current_dir().unwrap_or_default()
        } else {
            std::path::PathBuf::from(&state.render_export_dir)
        };
        state.open_file_browser(
            crate::app::state::FileBrowserCaller::RenderExportDir,
            "Select Export Directory",
            "",
            true,
            Some(start.as_path()),
        );
    }

    let sr_labels: &[&str] = &["44100 Hz", "48000 Hz", "96000 Hz"][..];
    let sr_values = [44100u32, 48000, 96000];

    // ── Sample Rate (dropdown) ──
    let row2 = popup_y + 88;
    draw_pixel_label(
        canvas,
        &state.theme,
        "SAMPLE RATE",
        cx,
        row2 + 4,
        120,
        Theme::c(state.theme.text_secondary),
    );
    dropdown(
        canvas,
        input,
        &state.theme,
        8810,
        cx + 130,
        row2,
        200,
        22,
        sr_labels,
        &mut state.render_sample_rate_idx,
        &mut state.dropdown_open_id,
    );

    // ── Bit Depth (dropdown) ──
    let row3 = popup_y + 118;
    draw_pixel_label(
        canvas,
        &state.theme,
        "BIT DEPTH",
        cx,
        row3 + 4,
        120,
        Theme::c(state.theme.text_secondary),
    );
    let bd_labels: &[&str] = &["16-bit PCM", "24-bit PCM", "32-bit Float"][..];
    dropdown(
        canvas,
        input,
        &state.theme,
        8820,
        cx + 130,
        row3,
        200,
        22,
        bd_labels,
        &mut state.render_bit_depth_idx,
        &mut state.dropdown_open_id,
    );

    // ── Song length info ──
    let row4 = popup_y + 150;
    let bpm = state.project.tempo_map.bpm_at(0.0);
    let loop_start = state.project.transport.loop_region.start;
    let loop_end = state.project.transport.loop_region.end;
    let song_len_beats = state
        .project
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .map(|c| c.start_time() + c.length())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let render_beats = if state.render_loop_only {
        (loop_end - loop_start).max(0.25)
    } else {
        song_len_beats
    };
    let render_secs = render_beats * 60.0 / bpm;
    let mins = (render_secs / 60.0) as i32;
    let secs = render_secs % 60.0;
    let info = if state.render_loop_only {
        format!(
            "Loop: {:.1}–{:.1} beats  ({:02}:{:05.2})  @ {:.0} BPM",
            loop_start, loop_end, mins, secs, bpm
        )
    } else {
        format!(
            "Song: {:.1} beats  ({:02}:{:05.2})  @ {:.0} BPM",
            song_len_beats, mins, secs, bpm
        )
    };
    draw_pixel_label(
        canvas,
        &state.theme,
        &info,
        cx,
        row4 + 4,
        cw,
        Theme::c(state.theme.text_dim),
    );

    let sr = sr_values[state.render_sample_rate_idx.min(2)];
    let bd = match state.render_bit_depth_idx {
        0 => "16-bit PCM",
        1 => "24-bit PCM",
        _ => "32-bit float",
    };
    let out_info = format!("Output: {} Hz  {}  WAV", sr, bd);
    draw_pixel_label(
        canvas,
        &state.theme,
        &out_info,
        cx,
        row4 + 20,
        cw,
        Theme::c(state.theme.text_dim),
    );

    // ── Loop-only toggle ──
    let row5 = popup_y + 185;
    draw_pixel_label(
        canvas,
        &state.theme,
        "RANGE",
        cx,
        row5 + 4,
        80,
        Theme::c(state.theme.text_secondary),
    );
    let __auto_id_53 = input.next_id();
    let loop_toggled = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_53,
            x: cx + 90,
            y: row5,
            width: 120,
            height: 20,
            label: if state.render_loop_only {
                "Loop region only"
            } else {
                "Full arrangement"
            }
            .into(),
            toggled: state.render_loop_only,
            icon: ButtonIcon::None,
            hint: Some("Toggle: render full arrangement or loop region only".into()),
            ..Default::default()
        },
    );
    if loop_toggled {
        state.render_loop_only = !state.render_loop_only;
    }

    // ── Render button ──
    let render_btn_y = popup_y + popup_h - 46;
    let __auto_id_54 = input.next_id();
    let render_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_54,
            x: popup_x + popup_w / 2 - 60,
            y: render_btn_y,
            width: 120,
            height: 30,
            label: "RENDER".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Export to WAV file".into()),
            ..Default::default()
        },
    );
    if render_clicked {
        let path = if state.render_export_dir.is_empty() {
            state.render_filename.clone()
        } else {
            let dir = std::path::Path::new(&state.render_export_dir);
            dir.join(&state.render_filename)
                .to_string_lossy()
                .to_string()
        };
        let settings = crate::engine::RenderSettings {
            master_volume: state.master_volume_ui,
            sample_rate_idx: state.render_sample_rate_idx,
            bit_depth_idx: state.render_bit_depth_idx,
            loop_only: state.render_loop_only,
            loop_start_beats: state.project.transport.loop_region.start,
            loop_end_beats: state.project.transport.loop_region.end,
        };
        let project_clone = state.project.clone();
        let progress = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let progress_clone = progress.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::engine::render_to_wav_with_progress(
                &project_clone,
                &path,
                &settings,
                Some(progress_clone),
            );
            let _ = tx.send(result.map(|()| path));
        });
        state.render_progress = Some(progress);
        state.render_result = Some(rx);
    }

    // Cancel button
    let __auto_id_55 = input.next_id();
    let cancel_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_55,
            x: popup_x + popup_w / 2 + 70,
            y: render_btn_y,
            width: 80,
            height: 30,
            label: "Cancel".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: None,
            ..Default::default()
        },
    );
    if cancel_clicked {
        state.render_popup_open = false;
    }

    // ── Dropdown popup overlays (draw on top of everything) ──
    let dd_w = 200i32;
    let dd_h = 22i32;
    let dd_x = cx + 130;
    dropdown_popup_overlay(
        canvas,
        &state.theme,
        8810,
        dd_x,
        row2,
        dd_w,
        dd_h,
        dd_w,
        sr_labels,
        state.render_sample_rate_idx,
        state.dropdown_open_id,
        input.mouse_x,
        input.mouse_y,
    );
    dropdown_popup_overlay(
        canvas,
        &state.theme,
        8820,
        dd_x,
        row3,
        dd_w,
        dd_h,
        dd_w,
        bd_labels,
        state.render_bit_depth_idx,
        state.dropdown_open_id,
        input.mouse_x,
        input.mouse_y,
    );

    // Click outside to dismiss (but not if clicking on an open dropdown list)
    let in_popup = input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h);
    let in_dropdown_list = if state.dropdown_open_id == 8810 {
        let list_h = sr_labels.len() as i32 * dd_h;
        input.mouse_in_rect(dd_x, row2, dd_w, dd_h + list_h)
    } else if state.dropdown_open_id == 8820 {
        let list_h = bd_labels.len() as i32 * dd_h;
        input.mouse_in_rect(dd_x, row3, dd_w, dd_h + list_h)
    } else {
        false
    };
    if !in_popup && !in_dropdown_list && input.mouse_pressed {
        state.render_popup_open = false;
    }
}

// ── Mixer view ───────────────────────────────────────────────────────
