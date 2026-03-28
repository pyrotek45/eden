// Eden DAW — Views: mixer_view

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::transport::{draw_mode_tabs, draw_transport};
use super::{gain_to_db_label, vol_gain_to_pos, vol_pos_to_gain};
use crate::app::input::{InputState, WidgetId};
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

pub fn draw_mixer(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    draw_transport(canvas, input, state);
    draw_mode_tabs(canvas, input, state);

    let top = state.transport_bar_height() + state.mode_tab_height();
    let w = state.window_width as i32;
    let h = state.window_height as i32 - top;

    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    let strip_w = 80;
    let track_count = state.project.tracks.len();
    let scrollbar_h = 18i32;
    let mixer_scroll = state.mixer_scroll_x as i32;

    // Clip content above scrollbar
    canvas.set_clip_rect(Rect::new(0, top, w as u32, (h - scrollbar_h) as u32));

    for i in 0..track_count {
        let x = 10 + i as i32 * (strip_w + 6) - mixer_scroll;
        let sy = top + 10;
        let sh = h - 20 - scrollbar_h;
        let track_id = state.project.tracks[i].id;
        let track_color = state.project.tracks[i].color;

        let selected =
            state.selected_tracks.contains(&track_id) || state.selected_track == Some(track_id);
        let is_multi = selected && state.selected_tracks.len() > 1;

        // Strip background (brighter if selected)
        if selected {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                state.theme.panel_bg[0].saturating_add(18),
                state.theme.panel_bg[1].saturating_add(18),
                state.theme.panel_bg[2].saturating_add(25),
                255,
            ));
        } else {
            canvas.set_draw_color(Theme::c(state.theme.panel_bg));
        }
        let _ = canvas.fill_rect(Rect::new(x, sy, strip_w as u32, sh as u32));

        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            track_color[0],
            track_color[1],
            track_color[2],
            track_color[3],
        ));
        let _ = canvas.fill_rect(Rect::new(x, sy, strip_w as u32, 6));

        // Track name label (clickable area for selection)
        let name_label = state.project.tracks[i].name.clone();
        draw_pixel_label(
            canvas,
            &state.theme,
            &name_label,
            x + 4,
            sy + 8,
            strip_w - 8,
            sdl2::pixels::Color::RGBA(200, 200, 210, 255),
        );

        // Volume fader (vertical)
        let mut vol_pos = vol_gain_to_pos(state.project.tracks[i].volume);
        let mixer_db_label = gain_to_db_label(state.project.tracks[i].volume);
        let btm_vol_id = input.next_id();
        let vol_changed = slider(
            canvas,
            input,
            &state.theme,
            &SliderParams {
                id: btm_vol_id,
                x: x + 25,
                y: sy + 30,
                width: 30,
                height: sh - 120,
                min: 0.0,
                max: 1.0,
                orientation: SliderOrientation::Vertical,
                label: Some(mixer_db_label),
                default_value: Some(vol_gain_to_pos(1.0)),
            },
            &mut vol_pos,
        );
        if vol_changed {
            let volume = vol_pos_to_gain(vol_pos);
            if is_multi {
                // Multi-track: snapshot origins on first change, apply raw-pixel delta
                if state.multi_vol_drag_origins.is_empty() {
                    state.multi_slider_snapshot = Some(state.project.clone());
                    state.multi_vol_drag_start_x = input.mouse_y; // vertical slider → use Y
                    state.multi_vol_slider_w = (sh - 120).max(1);
                    state.multi_vol_drag_origins = state
                        .project
                        .tracks
                        .iter()
                        .filter(|t| state.selected_tracks.contains(&t.id))
                        .map(|t| (t.id, vol_gain_to_pos(t.volume)))
                        .collect();
                }
                // Vertical slider: upward movement = increase (invert Y delta)
                let slider_h = state.multi_vol_slider_w as f32;
                let pos_delta = (state.multi_vol_drag_start_x - input.mouse_y) as f32 / slider_h;
                for &(tid, orig_pos) in &state.multi_vol_drag_origins {
                    let new_pos = (orig_pos + pos_delta).clamp(0.0, 1.0);
                    let new_gain = vol_pos_to_gain(new_pos);
                    if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                        t.volume = new_gain;
                    }
                }
            } else {
                state.project.tracks[i].volume = volume;
            }
            state.dirty = true;
        }
        // Commit volume on release
        if input.mouse_released && input.drag_widget == btm_vol_id {
            if !state.multi_vol_drag_origins.is_empty() {
                if let Some(snapshot) = state.multi_slider_snapshot.take() {
                    state
                        .commands
                        .push_undo_snapshot(snapshot, "Set Track Volumes");
                }
                state.multi_vol_drag_origins.clear();
            } else {
                let old_gain = vol_pos_to_gain(input.drag_start_value as f32);
                let new_gain = state.project.tracks[i].volume;
                if (old_gain - new_gain).abs() > 1e-4 {
                    state.commands.execute(
                        Box::new(crate::app::commands::SetTrackVolume {
                            track_id,
                            old_value: old_gain,
                            new_value: new_gain,
                        }),
                        &mut state.project,
                    );
                }
            }
        }

        // Pan knob (bipolar)
        let mut pan_val = state.project.tracks[i].pan;
        let btm_pan_id = input.next_id();
        let pan_changed = knob(
            canvas,
            input,
            &state.theme,
            &KnobParams {
                id: btm_pan_id,
                x: x + strip_w / 2,
                y: sy + sh - 60,
                radius: 14,
                min: -1.0,
                max: 1.0,
                sensitivity: 0.008,
                label: None,
                bipolar: true,
                default_value: Some(0.0),
                hint: Some("Pan".into()),
                snap_points: vec![0.0],
            },
            &mut pan_val,
        );
        if pan_changed {
            if is_multi {
                if state.multi_pan_drag_origins.is_empty() {
                    state.multi_slider_snapshot = Some(state.project.clone());
                    state.multi_pan_drag_start_x = input.mouse_y;
                    state.multi_pan_drag_origins = state
                        .project
                        .tracks
                        .iter()
                        .filter(|t| state.selected_tracks.contains(&t.id))
                        .map(|t| (t.id, t.pan))
                        .collect();
                }
                let pan_delta = (state.multi_pan_drag_start_x - input.mouse_y) as f32 * 0.008;
                for &(tid, orig_pan) in &state.multi_pan_drag_origins {
                    let new_pan = (orig_pan + pan_delta).clamp(-1.0, 1.0);
                    if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                        t.pan = new_pan;
                    }
                }
            } else {
                state.project.tracks[i].pan = pan_val;
            }
            state.dirty = true;
        }
        // Commit pan on release
        if input.mouse_released && input.drag_widget == btm_pan_id {
            if !state.multi_pan_drag_origins.is_empty() {
                if let Some(snapshot) = state.multi_slider_snapshot.take() {
                    state
                        .commands
                        .push_undo_snapshot(snapshot, "Set Track Pans");
                }
                state.multi_pan_drag_origins.clear();
            } else {
                let old_pan = input.drag_start_value as f32;
                if (old_pan - pan_val).abs() > 1e-4 {
                    state.commands.execute(
                        Box::new(crate::app::commands::SetTrackPan {
                            track_id,
                            old_value: old_pan,
                            new_value: pan_val,
                        }),
                        &mut state.project,
                    );
                }
            }
        }

        // Mute / Solo (skip for automation tracks)
        let is_auto_track =
            state.project.tracks[i].track_type == crate::app::models::TrackType::Automation;
        let mute_on = state.project.tracks[i].mute;
        let solo_on = state.project.tracks[i].solo;

        if !is_auto_track {
            let btm_mute_id = input.next_id();
            let mute_clicked = toggle_button(
                canvas,
                input,
                &state.theme,
                x + 8,
                sy + sh - 28,
                18,
                state.theme.mute_on,
                mute_on,
                btm_mute_id,
                "M",
                Some("Mute track"),
            );
            if mute_clicked {
                if is_multi {
                    let snapshot = state.project.clone();
                    let new_mute = !mute_on;
                    for &tid in &state.selected_tracks.clone() {
                        if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                            t.mute = new_mute;
                        }
                    }
                    state
                        .commands
                        .push_undo_snapshot(snapshot, "Toggle Mute (multi)");
                } else {
                    state.commands.execute(
                        Box::new(crate::app::commands::SetTrackMute {
                            track_id,
                            new_value: !mute_on,
                            old_value: mute_on,
                        }),
                        &mut state.project,
                    );
                }
                state.dirty = true;
            }

            let btm_solo_id = input.next_id();
            let solo_clicked = toggle_button(
                canvas,
                input,
                &state.theme,
                x + 30,
                sy + sh - 28,
                18,
                state.theme.solo_on,
                solo_on,
                btm_solo_id,
                "S",
                Some("Solo track"),
            );
            if solo_clicked {
                if is_multi {
                    let snapshot = state.project.clone();
                    let new_solo = !solo_on;
                    for &tid in &state.selected_tracks.clone() {
                        if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                            t.solo = new_solo;
                        }
                    }
                    state
                        .commands
                        .push_undo_snapshot(snapshot, "Toggle Solo (multi)");
                } else {
                    state.commands.execute(
                        Box::new(crate::app::commands::SetTrackSolo {
                            track_id,
                            new_value: !solo_on,
                            old_value: solo_on,
                        }),
                        &mut state.project,
                    );
                }
                state.dirty = true;
            }
        } // end non-automation mute/solo

        // Selection border
        if selected {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 200));
            let _ = canvas.draw_rect(Rect::new(x, sy, strip_w as u32, sh as u32));
            let _ = canvas.draw_rect(Rect::new(
                x + 1,
                sy + 1,
                (strip_w - 2) as u32,
                (sh - 2) as u32,
            ));
        } else {
            canvas.set_draw_color(Theme::c(state.theme.panel_border));
            let _ = canvas.draw_rect(Rect::new(x, sy, strip_w as u32, sh as u32));
        }

        // ── Click-to-select on mixer strip ──
        // Only process if click was not consumed by a widget (slider, knob, button)
        let strip_hover = input.mouse_in_rect(x, sy, strip_w, sh);
        if strip_hover && input.mouse_pressed && !input.consumed {
            if input.ctrl() {
                // Ctrl+click: toggle track in/out of selection
                if state.selected_tracks.contains(&track_id) {
                    state.selected_tracks.remove(&track_id);
                } else {
                    state.selected_tracks.insert(track_id);
                }
            } else if input.shift() {
                // Shift+click: range selection
                let track_ids: Vec<u32> = state.project.tracks.iter().map(|t| t.id).collect();
                if let Some(clicked_idx) = track_ids.iter().position(|&tid| tid == track_id) {
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
                        state.selected_tracks.insert(track_id);
                    }
                }
            } else {
                // Plain click: select only this track
                state.selected_tracks.clear();
                state.selected_tracks.insert(track_id);
            }
            state.selected_track = Some(track_id);
            input.consume();
        }
    }

    // Reset clip rect
    canvas.set_clip_rect(None);

    // Horizontal scrollbar at bottom of mixer
    let total_content_w = track_count as i32 * (strip_w + 6) + 10;
    if total_content_w > w {
        let sb_y = top + h - scrollbar_h;
        let max_scroll = (total_content_w - w).max(0) as f32;
        let frac = if max_scroll > 0.0 {
            state.mixer_scroll_x / max_scroll
        } else {
            0.0
        };
        let visible_frac = (w as f32 / total_content_w as f32).clamp(0.05, 1.0);
        let new_frac = scrollbar(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(84100),
            0,
            sb_y,
            w,
            scrollbar_h,
            ScrollbarDir::Horizontal,
            frac,
            visible_frac,
        );
        state.mixer_scroll_x = new_frac * max_scroll;

        // Scroll wheel in mixer area
        if input.mouse_y >= top
            && input.mouse_y < top + h
            && input.scroll_y != 0
            && !input.scroll_consumed
        {
            state.mixer_scroll_x = (state.mixer_scroll_x - input.scroll_y as f32 * 30.0)
                .max(0.0)
                .min(max_scroll);
        }

        // Middle-click drag: pan the mixer horizontally
        let mixer_drag_id = WidgetId::Auto(87003);
        if input.middle_mouse_down
            && input.mouse_y >= top
            && input.mouse_y < top + h
            && input.middle_drag_widget == WidgetId::None
        {
            input.middle_drag_widget = mixer_drag_id;
        }
        if input.middle_mouse_down && input.middle_drag_widget == mixer_drag_id {
            state.mixer_scroll_x =
                (state.mixer_scroll_x - input.mouse_dx as f32).clamp(0.0, max_scroll);
        }
    } else {
        state.mixer_scroll_x = 0.0;
    }
}

// ── Edit view (context-sensitive) ────────────────────────────────────
