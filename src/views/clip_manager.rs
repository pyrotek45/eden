// Eden DAW — Views: clip_manager

use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::state::*;
use crate::theme::Theme;
use crate::widgets::*;

/// Draw the clip manager sidebar — scrollable list of all clips across all tracks
/// with mini-preview thumbnails. Click a clip to select it.
pub(super) fn draw_clip_manager(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    use sdl2::rect::Rect;

    let sb_w = 14i32; // scrollbar width (left edge)
    let content_x = sb_w + 2; // content starts right of scrollbar

    let bg = Theme::c(state.theme.panel_bg);
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // Header
    let header_h = 24i32;
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, header_h as u32));
    draw_pixel_label(
        canvas,
        &state.theme,
        "CLIPS",
        content_x + 4,
        top + 6,
        w - content_x - 10,
        Theme::c(state.theme.text_secondary),
    );
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, top + header_h - 1),
        sdl2::rect::Point::new(w, top + header_h - 1),
    );

    let list_top = top + header_h;
    let list_h = h - header_h;
    let item_h = 56i32;
    let remove_btn_w = 16i32;
    let preview_w = 56i32;
    let padding = 4i32;
    // swatch + gap + preview + gap + text — all shifted right of scrollbar
    let swatch_x = content_x;
    let prev_x = swatch_x + 6;
    let text_x = prev_x + preview_w + padding;

    // Handle scroll
    if input.mouse_in_rect(sb_w, list_top, w - sb_w, list_h)
        && input.scroll_y != 0
        && !input.scroll_consumed
    {
        let scroll_delta = input.scroll_y * item_h / 3;
        state.clip_manager_scroll = (state.clip_manager_scroll - scroll_delta).max(0);
        input.scroll_consumed = true;
    }

    state.sync_clip_library();

    // Build display list: (track_id, lib_idx, display_name, color, length, clip_type, in_arrangement)
    #[allow(clippy::type_complexity)]
    let mut all_clips: Vec<(u32, usize, String, [u8; 4], f64, u8, bool)> = Vec::new();
    for (lib_idx, (track_id, clip)) in state.clip_library.iter().enumerate() {
        let (name, color, len, ctype) = match clip {
            crate::models::Clip::Midi(m) => (m.name.clone(), m.color, m.length, 0u8),
            crate::models::Clip::Audio(a) => (a.name.clone(), a.color, a.length, 1u8),
            crate::models::Clip::Automation(a) => (a.name.clone(), a.color, a.length, 2u8),
        };
        let in_arrangement = state
            .project
            .tracks
            .iter()
            .find(|t| t.id == *track_id)
            .map(|t| {
                t.clips.iter().any(|c| {
                    c.name() == clip.name()
                        && std::mem::discriminant(c) == std::mem::discriminant(clip)
                })
            })
            .unwrap_or(false);
        let display_name = if name.is_empty() {
            format!("Clip #{}", lib_idx + 1)
        } else {
            name
        };
        all_clips.push((
            *track_id,
            lib_idx,
            display_name,
            color,
            len,
            ctype,
            in_arrangement,
        ));
    }

    let total_items_h = all_clips.len() as i32 * item_h;
    let max_scroll = (total_items_h - list_h).max(0);
    state.clip_manager_scroll = state.clip_manager_scroll.min(max_scroll);

    let clip_top = list_top - state.clip_manager_scroll;

    // Track which lib_idx to remove (deferred so we don't mutate during iteration)
    let mut remove_lib_idx: Option<usize> = None;

    canvas.set_clip_rect(Rect::new(sb_w, list_top, (w - sb_w) as u32, list_h as u32));

    for (idx, &(track_id, lib_idx, ref clip_name, color, clip_len, ctype, in_arrangement)) in
        all_clips.iter().enumerate()
    {
        let iy = clip_top + idx as i32 * item_h;
        if iy + item_h < list_top || iy > list_top + list_h {
            continue;
        }

        // Find arrangement index (if any)
        let arrangement_clip_idx: Option<usize> = if in_arrangement {
            let lib_clip = &state.clip_library[lib_idx];
            state
                .project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| {
                    t.clips.iter().position(|c| {
                        c.name() == lib_clip.1.name()
                            && std::mem::discriminant(c) == std::mem::discriminant(&lib_clip.1)
                    })
                })
        } else {
            None
        };

        let is_selected = arrangement_clip_idx
            .map(|ci| state.selected_clip == Some((track_id, ci)))
            .unwrap_or(false);
        // Hover: only count over content area, not scrollbar
        let hover = input.mouse_in_rect(sb_w, iy, w - sb_w, item_h);

        // Item background
        let item_bg = if is_selected {
            let a = state.theme.accent;
            sdl2::pixels::Color::RGBA(a[0] / 3, a[1] / 3, a[2] / 3, 255)
        } else if hover {
            sdl2::pixels::Color::RGBA(50, 50, 55, 255)
        } else if !in_arrangement {
            sdl2::pixels::Color::RGBA(26, 26, 28, 255)
        } else {
            sdl2::pixels::Color::RGBA(35, 35, 38, 255)
        };
        canvas.set_draw_color(item_bg);
        let draw_iy = iy.max(list_top);
        let draw_h = ((iy + item_h).min(list_top + list_h) - draw_iy).max(0);
        if draw_h > 0 {
            let _ = canvas.fill_rect(Rect::new(sb_w, draw_iy, (w - sb_w) as u32, draw_h as u32));
        }

        // Color swatch
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(color[0], color[1], color[2], 255));
        let _ = canvas.fill_rect(Rect::new(
            swatch_x,
            iy + padding,
            3,
            (item_h - padding * 2) as u32,
        ));

        // Remove [X] button — top-right of item
        let rmx = w - remove_btn_w - 2;
        let rmy = iy + 2;
        let rm_hover = input.mouse_in_rect(rmx, rmy, remove_btn_w, remove_btn_w);
        canvas.set_draw_color(if rm_hover {
            sdl2::pixels::Color::RGBA(200, 60, 60, 220)
        } else {
            sdl2::pixels::Color::RGBA(100, 40, 40, 120)
        });
        let _ = canvas.fill_rect(Rect::new(
            rmx,
            rmy,
            remove_btn_w as u32,
            remove_btn_w as u32,
        ));
        draw_pixel_label(
            canvas,
            &state.theme,
            "X",
            rmx + 3,
            rmy + 3,
            remove_btn_w - 4,
            sdl2::pixels::Color::RGBA(220, 180, 180, 255),
        );
        if rm_hover && input.mouse_pressed && !input.consumed {
            state.clip_lib_confirm_delete = Some(lib_idx);
            input.consume();
        }

        // Mini-preview area
        let prev_y = iy + padding;
        let prev_h = item_h - padding * 2;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 22, 255));
        let _ = canvas.fill_rect(Rect::new(prev_x, prev_y, preview_w as u32, prev_h as u32));

        let prev_clip_rect = Rect::new(
            prev_x,
            prev_y.max(list_top),
            preview_w as u32,
            ((prev_y + prev_h).min(list_top + list_h) - prev_y.max(list_top)).max(0) as u32,
        );
        canvas.set_clip_rect(prev_clip_rect);
        let lib_clip_ref = &state.clip_library[lib_idx].1;
        match ctype {
            0 => {
                if let crate::models::Clip::Midi(mc) = lib_clip_ref {
                    let len_safe = clip_len.max(0.001);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        color[0], color[1], color[2], 200,
                    ));
                    for note in &mc.notes {
                        let nx = prev_x + (note.start / len_safe * preview_w as f64) as i32;
                        let nw = ((note.length / len_safe * preview_w as f64) as i32).max(1);
                        let pitch_t = note.pitch as f32 / 127.0;
                        let ny = prev_y + prev_h - (pitch_t * prev_h as f32) as i32 - 1;
                        let _ = canvas.fill_rect(Rect::new(
                            nx,
                            ny.clamp(prev_y, prev_y + prev_h - 1),
                            nw.min(preview_w) as u32,
                            1,
                        ));
                    }
                }
            }
            1 => {
                // Real waveform preview from cache
                let src = if let crate::models::Clip::Audio(ac) = lib_clip_ref {
                    ac.source_file.clone()
                } else {
                    String::new()
                };
                if let Some((l_max, l_min, r_max, r_min)) =
                    state.waveform_stereo_cache.get(&src).cloned()
                {
                    let num = l_max.len();
                    let amp_h = (prev_h / 2 - 1).max(1);
                    let ch0_cy = prev_y + prev_h / 4;
                    let ch1_cy = prev_y + 3 * prev_h / 4;
                    for px_i in 0..preview_w as usize {
                        let frac = px_i as f64 / preview_w as f64;
                        let i = ((frac * num as f64) as usize).min(num.saturating_sub(1));
                        let bx = prev_x + px_i as i32;
                        // Left
                        let lh_up = (l_max[i] * amp_h as f32) as i32;
                        let lh_dn = (l_min[i].abs() * amp_h as f32) as i32;
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                            color[0], color[1], color[2], 180,
                        ));
                        let _ = canvas.fill_rect(Rect::new(
                            bx,
                            ch0_cy - lh_up,
                            1,
                            (lh_up + lh_dn).max(1) as u32,
                        ));
                        // Right
                        let rh_up = (r_max[i] * amp_h as f32) as i32;
                        let rh_dn = (r_min[i].abs() * amp_h as f32) as i32;
                        let _ = canvas.fill_rect(Rect::new(
                            bx,
                            ch1_cy - rh_up,
                            1,
                            (rh_up + rh_dn).max(1) as u32,
                        ));
                    }
                } else {
                    // Fallback placeholder waveform
                    let segs = (preview_w / 2).max(1);
                    let amp_h = (prev_h / 2 - 1).max(1);
                    let cy = prev_y + prev_h / 2;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        color[0], color[1], color[2], 120,
                    ));
                    for i in 0..segs {
                        let t = i as f64 / segs as f64;
                        let amp = ((t * std::f64::consts::TAU * 3.0).sin() * 0.6
                            + (t * std::f64::consts::TAU * 7.3).sin() * 0.4)
                            .abs();
                        let bh = (amp * amp_h as f64) as i32 + 1;
                        let _ = canvas.fill_rect(Rect::new(
                            prev_x + i * 2,
                            cy - bh,
                            1,
                            (bh * 2) as u32,
                        ));
                    }
                }
            }
            2 => {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(color[0], color[1], color[2], 200));
                if let crate::models::Clip::Automation(ac) = lib_clip_ref {
                    let len_safe = clip_len.max(0.001);
                    for i in 1..ac.points.len() {
                        let p0 = &ac.points[i - 1];
                        let p1 = &ac.points[i];
                        let x0 = prev_x + (p0.time / len_safe * preview_w as f64) as i32;
                        let y0 = prev_y + prev_h - (p0.value * prev_h as f32) as i32;
                        let x1 = prev_x + (p1.time / len_safe * preview_w as f64) as i32;
                        let y1 = prev_y + prev_h - (p1.value * prev_h as f32) as i32;
                        let _ = canvas.draw_line(
                            sdl2::rect::Point::new(x0, y0.clamp(prev_y, prev_y + prev_h)),
                            sdl2::rect::Point::new(x1, y1.clamp(prev_y, prev_y + prev_h)),
                        );
                    }
                }
            }
            _ => {}
        }

        canvas.set_clip_rect(Rect::new(sb_w, list_top, (w - sb_w) as u32, list_h as u32));

        // Clip name
        let text_w = rmx - text_x - 2;
        draw_pixel_label(
            canvas,
            &state.theme,
            clip_name,
            text_x,
            iy + padding,
            text_w.max(0),
            if in_arrangement {
                Theme::c(state.theme.text_primary)
            } else {
                sdl2::pixels::Color::RGBA(90, 90, 100, 180)
            },
        );

        // Type + status tag
        let type_label = match (ctype, in_arrangement) {
            (0, true) => "MIDI",
            (1, true) => "AUDIO",
            (2, true) => "AUTO",
            (0, false) => "MIDI · not in arranger",
            (1, false) => "AUDIO · not in arranger",
            (2, false) => "AUTO · not in arranger",
            _ => "",
        };
        draw_pixel_label(
            canvas,
            &state.theme,
            type_label,
            text_x,
            iy + padding + 14,
            text_w.max(0),
            sdl2::pixels::Color::RGBA(color[0], color[1], color[2], 180),
        );

        let bars = clip_len / 4.0;
        let len_label = format!("{:.1}b", bars);
        draw_pixel_label(
            canvas,
            &state.theme,
            &len_label,
            text_x,
            iy + padding + 26,
            text_w.max(0),
            Theme::c(state.theme.text_dim),
        );

        // Separator
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(sb_w, iy + item_h - 1),
            sdl2::rect::Point::new(w, iy + item_h - 1),
        );

        // Click/drag interaction — skip the remove button area and scrollbar
        let click_area = hover && input.mouse_pressed && input.mouse_y < state.bottom_panel_y()
            && input.mouse_x >= sb_w   // exclude scrollbar column
            && input.mouse_x < rmx
            && input.active_widget == WidgetId::None;
        if click_area {
            if let Some(ci) = arrangement_clip_idx {
                // Select in arranger
                state.selected_clip = Some((track_id, ci));
                state.selected_clips.clear();
                state.selected_clips.insert((track_id, ci));
                state.selected_track = Some(track_id);
                state.selected_tracks.clear();
                state.selected_tracks.insert(track_id);
                if input.click_type != Some(crate::input::ClickType::Double) {
                    state.clip_sidebar_drag = Some((track_id, ci));
                }
                if input.click_type == Some(crate::input::ClickType::Double) {
                    // Open in editor
                    state.bottom_panel_tab = BottomPanelTab::PianoRoll;
                    if !state.bottom_panel_open {
                        state.bottom_panel_open = true;
                        state.bottom_panel_height = 320;
                    }
                    state.piano_roll_scroll_x = 0.0;
                    if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                        if let Some(crate::models::Clip::Midi(mc)) = track.clips.get(ci) {
                            if !mc.notes.is_empty() {
                                let avg_pitch =
                                    mc.notes.iter().map(|n| n.pitch as f64).sum::<f64>()
                                        / mc.notes.len() as f64;
                                let row_y = (127.0 - avg_pitch) as i32 * 12;
                                let panel_h = state.bottom_panel_effective_h();
                                let visible_h = (panel_h - 20 - 68 - 10).max(40);
                                state.piano_roll_scroll_y = (row_y - visible_h / 2).max(0);
                            }
                        }
                    }
                }
            } else {
                // Not in arrangement — double-click to restore, single-click starts a "place" drag
                if input.click_type == Some(crate::input::ClickType::Double) {
                    let clip_to_add = state.clip_library[lib_idx].1.clone();
                    if state.project.tracks.iter().any(|t| t.id == track_id) {
                        state.commands.execute(
                            Box::new(crate::commands::AddClips {
                                clips: vec![(track_id, clip_to_add)],
                                added_indices: Vec::new(),
                            }),
                            &mut state.project,
                        );
                        state.dirty = true;
                        state.push_status("Clip restored to arrangement");
                    }
                } else {
                    // Start a library drag — show ghost in arrangement, drop places the clip
                    let clip_clone = state.clip_library[lib_idx].1.clone();
                    state.library_drag_clip = Some((lib_idx, clip_clone));
                }
            }
        }
    }

    canvas.set_clip_rect(None);

    // (Confirmation dialog is drawn in draw_overlays to avoid being clipped)
    // Handle the delete action if confirmed from the overlay dialog
    if state.clip_lib_confirm_execute {
        state.clip_lib_confirm_execute = false;
        if let Some(del_idx) = state.clip_lib_confirmed_idx.take() {
            remove_lib_idx = Some(del_idx);
        }
    }

    // ── Deferred remove (only executed when confirmed) ──────────────
    if let Some(li) = remove_lib_idx {
        if li < state.clip_library.len() {
            let (rem_tid, ref rem_clip) = state.clip_library[li].clone();
            // Find matching arrangement clips and remove via command (undoable)
            let mut to_delete: Vec<(u32, usize, crate::models::Clip)> = Vec::new();
            if let Some(track) = state.project.tracks.iter().find(|t| t.id == rem_tid) {
                for (ci, c) in track.clips.iter().enumerate() {
                    if c.name() == rem_clip.name()
                        && std::mem::discriminant(c) == std::mem::discriminant(rem_clip)
                    {
                        to_delete.push((rem_tid, ci, c.clone()));
                    }
                }
            }
            if !to_delete.is_empty() {
                state.commands.execute(
                    Box::new(crate::commands::DeleteClips { clips: to_delete }),
                    &mut state.project,
                );
            }
            state.clip_library.remove(li);
            // Clear selected_clip if it pointed to a now-gone clip
            state.selected_clip = None;
            state.push_status("Clip removed from project (Ctrl+Z to undo)");
            state.dirty = true;
        }
    }

    // ── Scrollbar on LEFT edge (matches instruments/themes style) ────
    {
        let sb_x = 0i32;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 32, 40, 160));
        let _ = canvas.fill_rect(Rect::new(sb_x, list_top, sb_w as u32, list_h as u32));
        if max_scroll > 0 {
            let thumb_h = ((list_h as f32 / total_items_h as f32) * list_h as f32).max(16.0) as i32;
            let thumb_y = list_top
                + (state.clip_manager_scroll as f32 / max_scroll as f32 * (list_h - thumb_h) as f32)
                    as i32;
            let hover_sb = input.mouse_in_rect(sb_x, list_top, sb_w, list_h);
            let hover_thumb = input.mouse_in_rect(sb_x, thumb_y, sb_w, thumb_h.max(8));
            let thumb_color = if hover_sb {
                sdl2::pixels::Color::RGBA(140, 150, 180, 240)
            } else {
                sdl2::pixels::Color::RGBA(100, 110, 140, 200)
            };
            canvas.set_draw_color(thumb_color);
            let _ = canvas.fill_rect(Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h as u32));
            if hover_thumb && input.mouse_pressed {
                input.active_widget = WidgetId::Auto(87000);
                input.drag_widget = WidgetId::Auto(87000);
                input.drag_start_value = state.clip_manager_scroll as f64;
            }
            if hover_sb && !hover_thumb && input.mouse_pressed {
                let rel = (input.mouse_y - list_top) as f32 / list_h as f32;
                state.clip_manager_scroll = (rel * max_scroll as f32) as i32;
                state.clip_manager_scroll = state.clip_manager_scroll.clamp(0, max_scroll);
                input.active_widget = WidgetId::Auto(87000);
                input.drag_widget = WidgetId::Auto(87000);
                input.drag_start_value = state.clip_manager_scroll as f64;
            }
            if input.drag_widget == WidgetId::Auto(87000) && input.mouse_down {
                let dy = input.mouse_y - input.drag_start_y;
                let scroll_range = list_h - thumb_h;
                if scroll_range > 0 {
                    let delta = (dy as f32 / scroll_range as f32 * max_scroll as f32) as i32;
                    state.clip_manager_scroll =
                        (input.drag_start_value as i32 + delta).clamp(0, max_scroll);
                }
            }
        }
    }
}
