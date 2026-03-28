// Eden DAW — Views: track_headers

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::{gain_to_db_label, vol_gain_to_pos, vol_pos_to_gain};
use crate::app::input::{InputState, WidgetId};
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

pub fn draw_track_headers(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    let top = state.track_area_top();
    let left = state.arrangement_left_offset();
    let header_w = state.arrangement.track_header_width;
    let scroll_y = state.arrangement.scroll_y;

    // Set a bounding clip rect to prevent headers from drawing over toolbars
    canvas.set_clip_rect(Rect::new(
        left,
        top,
        header_w as u32,
        state.track_area_height() as u32,
    ));

    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(
        left,
        top,
        header_w as u32,
        state.track_area_height() as u32,
    ));

    let mut y = top - scroll_y;

    #[allow(clippy::type_complexity)]
    let track_infos: Vec<(
        u32,
        String,
        i32,
        f32,
        f32,
        bool,
        bool,
        [u8; 4],
        crate::app::models::TrackType,
    )> = state
        .project
        .tracks
        .iter()
        .map(|t| {
            (
                t.id,
                t.name.clone(),
                t.height,
                t.volume,
                t.pan,
                t.mute,
                t.solo,
                t.color,
                t.track_type,
            )
        })
        .collect();

    let track_count = track_infos.len();
    for (track_index, (id, _name, height, mut volume, mut pan, mute, solo, color, track_type)) in
        track_infos.into_iter().enumerate()
    {
        if y + height < top {
            y += height;
            continue;
        }
        if y > top + state.track_area_height() {
            break;
        }

        let selected = state.selected_tracks.contains(&id) || state.selected_track == Some(id);
        let bg = if selected {
            Theme::c(state.theme.track_selected)
        } else {
            Theme::c(state.theme.track_header)
        };
        canvas.set_draw_color(bg);
        let _ = canvas.fill_rect(Rect::new(left, y, header_w as u32, height as u32));

        // Color strip
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            color[0], color[1], color[2], color[3],
        ));
        let _ = canvas.fill_rect(Rect::new(left, y, 4, height as u32));

        // Track whether the header was clicked — we'll only act on it AFTER
        // buttons have been drawn so button clicks don't also select the track.
        let header_click = input.mouse_in_rect(left, y, header_w, height)
            && input.mouse_y < top + state.track_area_height()
            && input.mouse_y < state.bottom_panel_y()
            && input.mouse_pressed
            && !input.consumed;

        // ── Track header widget layout ─────────────────────────────
        // Row 1 (top): type icon + track name
        let icon_size = 14i32;
        let icon_x = left + 7;
        let icon_y = y + 5;
        let icon_col = sdl2::pixels::Color::RGBA(color[0], color[1], color[2], 220);
        draw_track_type_icon(canvas, &track_type, icon_x, icon_y, icon_size, icon_col);

        let name_x = left + 26;
        let name_y = y + 4;
        let name_w = header_w - 36;
        let name_h = 16i32;
        // Unique text field id for this track (use track slot index, not id, for stability)
        let tf_id: u32 = 90000 + track_index as u32;
        let is_renaming = state.text_field_active_id == tf_id;

        // Double-click on name area to start rename
        let name_hover = input.mouse_in_rect(name_x, name_y, name_w, name_h);
        if name_hover
            && input.mouse_pressed
            && input.click_type == Some(crate::app::input::ClickType::Double)
        {
            // Start renaming — suppress the header's double-click-to-rack behaviour
            state.text_field_active_id = tf_id;
            state.text_field_buffer = _name.clone();
            state.text_field_cursor = _name.len();
        }

        if is_renaming {
            // Draw the live text field for renaming
            let mut buf = state.text_field_buffer.clone();
            let mut cursor = state.text_field_cursor;
            let mut active_id = state.text_field_active_id;
            let (committed, new_val) = text_field(
                canvas,
                input,
                &state.theme,
                &TextFieldParams {
                    id: tf_id,
                    x: name_x,
                    y: name_y,
                    width: name_w,
                    height: name_h,
                    hint: None,
                },
                &_name,
                &mut active_id,
                &mut buf,
                &mut cursor,
            );
            state.text_field_active_id = active_id;
            state.text_field_buffer = buf;
            state.text_field_cursor = cursor;
            if committed {
                if let Some(new_name) = new_val {
                    if let Some(t) = state.project.tracks.iter().find(|t| t.id == id) {
                        let old_name = t.name.clone();
                        if old_name != new_name {
                            state.commands.execute(
                                Box::new(crate::app::commands::SetTrackName {
                                    track_id: id,
                                    old_name,
                                    new_name,
                                }),
                                &mut state.project,
                            );
                            state.dirty = true;
                        }
                    }
                }
            }
        } else {
            let name_col = sdl2::pixels::Color::RGBA(210, 210, 210, 255);
            draw_pixel_label(
                canvas,
                &state.theme,
                &_name,
                name_x,
                name_y + 2,
                name_w,
                name_col,
            );
        }

        // Row 2: Volume slider + pan knob (skip for Automation tracks)
        if track_type != crate::app::models::TrackType::Automation {
            let vol_y = y + 24;
            let knob_r = 10i32;
            let knob_x = left + header_w - knob_r - 8;
            let knob_y = vol_y + 6;
            let vol_slider_id = input.next_id();
            // Musical gain scaling: slider operates on position [0,1],
            // mapped through a dB-aware curve to gain [0,2].
            let mut vol_pos = vol_gain_to_pos(volume);
            let db_label = gain_to_db_label(volume);
            let vol_changed = slider(
                canvas,
                input,
                &state.theme,
                &SliderParams {
                    id: vol_slider_id,
                    x: left + 8,
                    y: vol_y,
                    width: knob_x - left - 12,
                    height: 10,
                    min: 0.0,
                    max: 1.0,
                    orientation: SliderOrientation::Horizontal,
                    label: Some(db_label),
                    default_value: Some(vol_gain_to_pos(1.0)),
                },
                &mut vol_pos,
            );
            if vol_changed {
                let is_multi = selected && state.selected_tracks.len() > 1;
                volume = vol_pos_to_gain(vol_pos);
                if is_multi {
                    // On first change, snapshot origins and record raw mouse X + slider width
                    if state.multi_vol_drag_origins.is_empty() {
                        state.multi_slider_snapshot = Some(state.project.clone());
                        state.multi_vol_drag_start_x = input.mouse_x;
                        state.multi_vol_slider_w = (knob_x - left - 12).max(1);
                        state.multi_vol_drag_origins = state
                            .project
                            .tracks
                            .iter()
                            .filter(|t| state.selected_tracks.contains(&t.id))
                            .map(|t| (t.id, vol_gain_to_pos(t.volume)))
                            .collect();
                    }
                    // Use raw pixel delta so clamp on one track doesn't freeze others
                    let slider_w = state.multi_vol_slider_w as f32;
                    let pos_delta =
                        (input.mouse_x - state.multi_vol_drag_start_x) as f32 / slider_w;
                    // Apply relative delta in position space, then convert to gain
                    for &(tid, orig_pos) in &state.multi_vol_drag_origins {
                        let new_pos = (orig_pos + pos_delta).clamp(0.0, 1.0);
                        let new_gain = vol_pos_to_gain(new_pos);
                        if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                            t.volume = new_gain;
                        }
                    }
                } else if let Some(t) = state.project.tracks.iter_mut().find(|tx| tx.id == id) {
                    t.volume = volume;
                }
                state.dirty = true;
            }
            if input.mouse_released && input.drag_widget == vol_slider_id {
                if !state.multi_vol_drag_origins.is_empty() {
                    // Multi-track: commit undo from snapshot
                    if let Some(snapshot) = state.multi_slider_snapshot.take() {
                        state
                            .commands
                            .push_undo_snapshot(snapshot, "Set Track Volumes");
                    }
                    state.multi_vol_drag_origins.clear();
                } else {
                    // Convert both old and new positions back to gain for undo
                    let old_gain = vol_pos_to_gain(input.drag_start_value as f32);
                    if (old_gain - volume).abs() > 1e-4 {
                        state.commands.execute(
                            Box::new(crate::app::commands::SetTrackVolume {
                                track_id: id,
                                old_value: old_gain,
                                new_value: volume,
                            }),
                            &mut state.project,
                        );
                    }
                }
            }

            // Pan knob (right of volume slider)
            let pan_knob_id = input.next_id();
            let pan_changed = knob(
                canvas,
                input,
                &state.theme,
                &KnobParams {
                    id: pan_knob_id,
                    x: knob_x,
                    y: knob_y,
                    radius: knob_r,
                    min: -1.0,
                    max: 1.0,
                    sensitivity: 0.008,
                    label: None,
                    bipolar: true,
                    default_value: Some(0.0),
                    hint: Some("Pan".into()),
                    snap_points: vec![0.0],
                },
                &mut pan,
            );
            if pan_changed {
                let is_multi = selected && state.selected_tracks.len() > 1;
                if is_multi {
                    if state.multi_pan_drag_origins.is_empty() {
                        if state.multi_slider_snapshot.is_none() {
                            state.multi_slider_snapshot = Some(state.project.clone());
                        }
                        state.multi_pan_drag_start_x = input.mouse_y;
                        state.multi_pan_drag_origins = state
                            .project
                            .tracks
                            .iter()
                            .filter(|t| state.selected_tracks.contains(&t.id))
                            .map(|t| (t.id, t.pan))
                            .collect();
                    }
                    // Use raw pixel delta (vertical drag for knob, sensitivity matches knob)
                    let delta = (state.multi_pan_drag_start_x - input.mouse_y) as f32 * 0.008;
                    for &(tid, orig_pan) in &state.multi_pan_drag_origins {
                        let new_p = (orig_pan + delta).clamp(-1.0, 1.0);
                        if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                            t.pan = new_p;
                        }
                    }
                } else if let Some(t) = state.project.tracks.iter_mut().find(|tx| tx.id == id) {
                    t.pan = pan;
                }
                state.dirty = true;
            }
            if input.mouse_released && input.drag_widget == pan_knob_id {
                if !state.multi_pan_drag_origins.is_empty() {
                    if let Some(snapshot) = state.multi_slider_snapshot.take() {
                        state
                            .commands
                            .push_undo_snapshot(snapshot, "Set Track Pans");
                    }
                    state.multi_pan_drag_origins.clear();
                } else {
                    let old_pan = input.drag_start_value as f32;
                    if (old_pan - pan).abs() > 1e-4 {
                        state.commands.execute(
                            Box::new(crate::app::commands::SetTrackPan {
                                track_id: id,
                                old_value: old_pan,
                                new_value: pan,
                            }),
                            &mut state.project,
                        );
                    }
                }
            }
        }

        // Row 3 (bottom): Mute + Solo buttons (not for automation tracks)
        let btn_y = y + height - 26;
        if track_type != crate::app::models::TrackType::Automation {
            let mute_id = input.next_id();
            let mute_clicked = toggle_button(
                canvas,
                input,
                &state.theme,
                left + 8,
                btn_y,
                20,
                state.theme.mute_on,
                mute,
                mute_id,
                "M",
                Some("Mute track"),
            );
            if mute_clicked {
                let is_multi = selected && state.selected_tracks.len() > 1;
                if is_multi {
                    // Toggle mute on all selected tracks using snapshot undo
                    let new_mute = !mute;
                    let snapshot = state.project.clone();
                    for t in &mut state.project.tracks {
                        if state.selected_tracks.contains(&t.id) {
                            t.mute = new_mute;
                        }
                    }
                    state
                        .commands
                        .push_undo_snapshot(snapshot, "Toggle Mute (multi)");
                    state.dirty = true;
                } else if let Some(t) = state.project.tracks.iter().find(|t| t.id == id) {
                    state.commands.execute(
                        Box::new(crate::app::commands::SetTrackMute {
                            track_id: id,
                            new_value: !t.mute,
                            old_value: t.mute,
                        }),
                        &mut state.project,
                    );
                }
            }

            let solo_id = input.next_id();
            let solo_clicked = toggle_button(
                canvas,
                input,
                &state.theme,
                left + 32,
                btn_y,
                20,
                state.theme.solo_on,
                solo,
                solo_id,
                "S",
                Some("Solo track"),
            );
            if solo_clicked {
                if input.ctrl() {
                    // Ctrl+click: unsolo ALL tracks
                    let snapshot = state.project.clone();
                    for t in &mut state.project.tracks {
                        t.solo = false;
                    }
                    state.commands.push_undo_snapshot(snapshot, "Unsolo All");
                    state.dirty = true;
                } else {
                    let is_multi = selected && state.selected_tracks.len() > 1;
                    if is_multi {
                        let new_solo = !solo;
                        let snapshot = state.project.clone();
                        for t in &mut state.project.tracks {
                            if state.selected_tracks.contains(&t.id) {
                                t.solo = new_solo;
                            }
                        }
                        state
                            .commands
                            .push_undo_snapshot(snapshot, "Toggle Solo (multi)");
                        state.dirty = true;
                    } else if let Some(t) = state.project.tracks.iter().find(|t| t.id == id) {
                        state.commands.execute(
                            Box::new(crate::app::commands::SetTrackSolo {
                                track_id: id,
                                new_value: !t.solo,
                                old_value: t.solo,
                            }),
                            &mut state.project,
                        );
                    }
                }
            }
        } // end non-automation mute/solo

        // ── Automation enable/disable toggle (only for Automation tracks) ──
        if track_type == crate::app::models::TrackType::Automation {
            let auto_enabled = state
                .project
                .tracks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.automation_enabled)
                .unwrap_or(true);
            // Button shows OFF (untoggled) when enabled, RED (toggled) when disabled
            let auto_id = input.next_id();
            let auto_toggle = toggle_button(
                canvas,
                input,
                &state.theme,
                left + 56,
                btn_y,
                20,
                [220, 60, 60, 255],
                !auto_enabled,
                auto_id,
                "A",
                Some("Enable/disable automation"),
            );
            if auto_toggle {
                if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == id) {
                    t.automation_enabled = !t.automation_enabled;
                    state.dirty = true;
                }
            }
        }

        // ── Track management: delete (X), up (▲), down (▼) ──
        // Place at the right side of the bottom row
        let mgmt_x = left + header_w - 62;
        let __auto_id_19 = input.next_id();
        let del_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_19,
                x: mgmt_x + 40,
                y: btn_y,
                width: 18,
                height: 20,
                label: "X".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Delete track".into()),
                ..Default::default()
            },
        );
        if del_clicked {
            if selected && state.selected_tracks.len() > 1 {
                // Multi-track delete: store all selected track IDs for confirmation
                state.track_confirm_multi_delete =
                    Some(state.selected_tracks.iter().copied().collect());
            } else {
                state.track_confirm_delete = Some((id, track_index));
            }
        }

        let __auto_id_20 = input.next_id();
        let up_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_20,
                x: mgmt_x,
                y: btn_y,
                width: 18,
                height: 20,
                label: "▲".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Move track up".into()),
                ..Default::default()
            },
        );
        if up_clicked && track_index > 0 {
            state.commands.execute(
                Box::new(crate::app::commands::ReorderTrack {
                    track_id: id,
                    old_index: track_index,
                    new_index: track_index - 1,
                }),
                &mut state.project,
            );
            state.dirty = true;
        }

        let __auto_id_21 = input.next_id();
        let down_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_21,
                x: mgmt_x + 20,
                y: btn_y,
                width: 18,
                height: 20,
                label: "▼".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Move track down".into()),
                ..Default::default()
            },
        );
        if down_clicked && track_index + 1 < track_count {
            state.commands.execute(
                Box::new(crate::app::commands::ReorderTrack {
                    track_id: id,
                    old_index: track_index,
                    new_index: track_index + 1,
                }),
                &mut state.project,
            );
            state.dirty = true;
        }

        // ── Track height resize handle (bottom 5px strip) ──
        const TRACK_RESIZE_H: i32 = 5;
        let resize_y = y + height - TRACK_RESIZE_H;
        let resize_hover = input.mouse_in_rect(left, resize_y, header_w, TRACK_RESIZE_H + 2)
            && input.mouse_y < top + state.track_area_height();

        // Draw separator / resize handle line
        let line_col = if resize_hover {
            Theme::c(state.theme.accent)
        } else {
            Theme::c(state.theme.panel_border)
        };
        canvas.set_draw_color(line_col);
        let _ = canvas.fill_rect(Rect::new(left, y + height - 1, header_w as u32, 1));

        // Hover highlight band
        if resize_hover {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                state.theme.accent[0],
                state.theme.accent[1],
                state.theme.accent[2],
                60,
            ));
            let _ = canvas.fill_rect(Rect::new(
                left,
                resize_y,
                header_w as u32,
                TRACK_RESIZE_H as u32,
            ));
        }

        // Start drag on press
        if resize_hover && input.mouse_pressed && input.drag_widget == WidgetId::None {
            input.drag_widget = WidgetId::TrackResize(id);
            input.active_widget = WidgetId::TrackResize(id);
            input.drag_start_y = input.mouse_y;
            input.drag_start_value = height as f64; // original height
            input.drag_start_value2 = height as f64; // same (for undo)
        }

        // Live resize while dragging
        if input.drag_widget == WidgetId::TrackResize(id) && input.mouse_down {
            const MIN_H: i32 = 80;
            let new_h =
                ((input.drag_start_value as i32) + (input.mouse_y - input.drag_start_y)).max(MIN_H);
            if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == id) {
                t.height = new_h;
            }
            state.dirty = true;
        }

        // Commit undo on release
        if input.drag_widget == WidgetId::TrackResize(id) && input.mouse_released {
            let old_h = input.drag_start_value2 as i32;
            let new_h = state
                .project
                .tracks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.height)
                .unwrap_or(old_h);
            if new_h != old_h {
                state.commands.execute(
                    Box::new(crate::app::commands::ResizeTrack {
                        track_id: id,
                        old_height: old_h,
                        new_height: new_h,
                    }),
                    &mut state.project,
                );
            }
        }

        // ── Track selection (deferred to after all buttons) ──
        // Only select the track if the click was not consumed by a button.
        if header_click && !input.consumed {
            state.focused_panel = crate::app::state::FocusedPanel::Arrangement;
            if input.click_type == Some(crate::app::input::ClickType::Double) {
                // Double-click: open rack panel for this track
                state.selected_track = Some(id);
                state.selected_tracks.clear();
                state.selected_tracks.insert(id);
                state.bottom_panel_tab = BottomPanelTab::InstrumentRack;
                if !state.bottom_panel_open {
                    state.bottom_panel_open = true;
                    state.bottom_panel_height = 320;
                }
            } else if input.ctrl() {
                // Ctrl+click: toggle individual track in/out of selection
                if state.selected_tracks.contains(&id) {
                    state.selected_tracks.remove(&id);
                } else {
                    state.selected_tracks.insert(id);
                }
            } else if input.shift() {
                // Shift+click: select range between this track and nearest already-selected track
                let track_ids: Vec<u32> = state.project.tracks.iter().map(|t| t.id).collect();
                if let Some(clicked_idx) = track_ids.iter().position(|&tid| tid == id) {
                    // Find the nearest already-selected track index (or selected_track)
                    let anchor_idx = state
                        .selected_track
                        .and_then(|sid| track_ids.iter().position(|&tid| tid == sid))
                        .or_else(|| {
                            state
                                .selected_tracks
                                .iter()
                                .filter_map(|&sid| track_ids.iter().position(|&tid| tid == sid))
                                .min_by_key(|&idx| (idx as i32 - clicked_idx as i32).unsigned_abs())
                        });
                    if let Some(anchor) = anchor_idx {
                        let lo = anchor.min(clicked_idx);
                        let hi = anchor.max(clicked_idx);
                        for &tid in &track_ids[lo..=hi] {
                            state.selected_tracks.insert(tid);
                        }
                    } else {
                        state.selected_tracks.insert(id);
                    }
                } else {
                    state.selected_tracks.insert(id);
                }
            } else {
                state.selected_tracks.clear();
                state.selected_tracks.insert(id);
            }
            state.selected_track = Some(id);
            // Consume the click so it doesn't propagate to the lane/playhead
            input.active_widget = WidgetId::Auto(80200);
            input.consume();
        }

        y += height;
    }

    // ── Add Track button below last track header (always visible at bottom) ──
    {
        // Fixed position at bottom of track header area so it's always accessible
        let add_btn_w = header_w - 16;
        let add_btn_h = 20;
        let add_btn_x = left + 8;
        let add_btn_y = top + state.track_area_height() - add_btn_h - 6;

        // Draw add button outside clip rect so it's always visible
        canvas.set_clip_rect(None);

        let __auto_id_22 = input.next_id();
        let add_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_22,
                x: add_btn_x,
                y: add_btn_y,
                width: add_btn_w,
                height: add_btn_h,
                label: "+ Add Track".into(),
                toggled: state.add_track_popup_open,
                icon: ButtonIcon::None,
                hint: Some("Add a new track".into()),
                ..Default::default()
            },
        );
        if add_clicked {
            state.add_track_popup_open = !state.add_track_popup_open;
        }

        // Popup: track type options (opens ABOVE the button to avoid clipping)
        if state.add_track_popup_open {
            let popup_h = 3 * 22 + 4;
            let popup_w = add_btn_w;
            let popup_x = add_btn_x;
            let popup_y = add_btn_y - popup_h - 2;

            // Background
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(35, 37, 45, 250));
            let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));
            canvas.set_draw_color(Theme::c(state.theme.panel_border));
            let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

            let types = [
                ("♪ MIDI Track", crate::app::models::TrackType::Midi),
                ("♫ Audio Track", crate::app::models::TrackType::Audio),
                ("~ Auto Track", crate::app::models::TrackType::Automation),
            ];
            for (i, (label, tt)) in types.iter().enumerate() {
                let ry = popup_y + 2 + i as i32 * 22;
                let hover = input.mouse_in_rect(popup_x + 1, ry, popup_w - 2, 20);
                if hover {
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 65, 80, 255));
                    let _ = canvas.fill_rect(Rect::new(popup_x + 1, ry, (popup_w - 2) as u32, 20));
                }
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    label,
                    popup_x + 8,
                    ry + 5,
                    popup_w - 16,
                    Theme::c(state.theme.text_primary),
                );
                if hover && input.mouse_pressed {
                    let new_id = state.project.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                    let name = match tt {
                        crate::app::models::TrackType::Midi => format!("MIDI {}", new_id),
                        crate::app::models::TrackType::Audio => format!("Audio {}", new_id),
                        crate::app::models::TrackType::Automation => format!("Auto {}", new_id),
                    };
                    let new_track = crate::app::models::Track::new(new_id, &name, *tt);
                    state.commands.execute(
                        Box::new(crate::app::commands::AddTrack { track: new_track }),
                        &mut state.project,
                    );
                    state.selected_track = Some(new_id);
                    state.selected_tracks.clear();
                    state.selected_tracks.insert(new_id);
                    state.add_track_popup_open = false;
                    state.dirty = true;
                }
            }

            // Close popup if clicking outside
            if input.mouse_pressed
                && !input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h)
                && !input.mouse_in_rect(add_btn_x, add_btn_y, add_btn_w, add_btn_h)
            {
                state.add_track_popup_open = false;
            }
        }
    }

    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(left + header_w - 1, top),
        sdl2::rect::Point::new(left + header_w - 1, top + state.track_area_height()),
    );

    // ── Scroll handling for track header area ──
    if input.mouse_in_rect(left, top, header_w, state.track_area_height())
        && input.mouse_y < state.bottom_panel_y()
        && input.scroll_y != 0
        && !input.ctrl()
        && !input.scroll_consumed
    // Ctrl+scroll is handled by the arranger zoom code
    {
        let max_sy = state.max_arrangement_scroll_y();
        state.arrangement.scroll_y =
            (state.arrangement.scroll_y - input.scroll_y * 30).clamp(0, max_sy);
    }

    // ── Click in empty header area below tracks → focus arrangement ──
    {
        let scroll_y = state.arrangement.scroll_y;
        let mut y_acc = top - scroll_y;
        for t in &state.project.tracks {
            y_acc += t.height;
        }
        let bottom_panel_y = state.bottom_panel_y();
        let empty_header_area = y_acc < bottom_panel_y
            && input.mouse_in_rect(left, y_acc, header_w, bottom_panel_y - y_acc)
            && input.mouse_pressed
            && !input.consumed;
        if empty_header_area {
            state.focused_panel = crate::app::state::FocusedPanel::Arrangement;
        }
    }

    // ── Drop module/sample onto empty header area below tracks → create MIDI track ──
    if input.mouse_released && state.module_drag.is_some() {
        let scroll_y = state.arrangement.scroll_y;
        let mut y_acc = top - scroll_y;
        for t in &state.project.tracks {
            y_acc += t.height;
        }
        let bottom_panel_y = state.bottom_panel_y();
        // Mouse is in header column AND below all existing tracks
        if input.mouse_in_rect(left, top, header_w, state.track_area_height())
            && input.mouse_y >= y_acc
            && input.mouse_y < bottom_panel_y
        {
            if let Some(module_name) = state.module_drag.take() {
                let is_instrument = crate::modules::is_instrument(&module_name);
                if is_instrument {
                    let new_id = state.project.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                    let mut new_track = crate::app::models::Track::new(
                        new_id,
                        &module_name,
                        crate::app::models::TrackType::Midi,
                    );
                    new_track.rack = vec![crate::app::models::create_rack_slot_for_module(
                        &module_name,
                        1,
                    )];
                    state.commands.execute(
                        Box::new(crate::app::commands::AddTrack { track: new_track }),
                        &mut state.project,
                    );
                    state.selected_track = Some(new_id);
                    state.selected_tracks.clear();
                    state.selected_tracks.insert(new_id);
                    state.dirty = true;
                    state.push_status(format!("Created MIDI track with {}", module_name));
                } else {
                    state.push_status(format!(
                        "{} is not a generator — only generators can create new tracks",
                        module_name
                    ));
                }
                state.module_drag_insert_idx = None;
                state.module_drag_replace_idx = None;
            }
        }
    }

    // Clear clip rect
    canvas.set_clip_rect(None);
}

// ── Track lanes + clips (with resize handles) ────────────────────────
