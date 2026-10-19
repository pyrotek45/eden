// Eden DAW — Views: track_lanes

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::mixer::meter_color;
use crate::app::input::{InputState, WidgetId};
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

pub fn draw_track_lanes(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let top = state.track_area_top();
    let left = state.arrangement_left_offset();
    let header_w = state.arrangement.track_header_width;
    let lane_left = left + header_w;
    let w = state.window_width as i32;
    let scroll_x = state.arrangement.scroll_x;
    let scroll_y = state.arrangement.scroll_y;
    let zoom = state.arrangement.zoom_x;
    let lane_area_w = w - lane_left;

    // Set a bounding clip rect to prevent lanes/clips from overlapping track headers and top toolbars
    canvas.set_clip_rect(Rect::new(
        lane_left,
        top,
        (w - lane_left) as u32,
        state.track_area_height() as u32,
    ));

    // Background
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(
        lane_left,
        top,
        lane_area_w as u32,
        state.track_area_height() as u32,
    ));

    // Draw track alternate backgrounds FIRST
    let mut y = top - scroll_y;
    let track_count = state.project.tracks.len();
    for track_idx in 0..track_count {
        let track_height = state.project.tracks[track_idx].height;
        if y + track_height < top {
            y += track_height;
            continue;
        }
        if y > top + state.track_area_height() {
            break;
        }

        if track_idx % 2 == 1 {
            canvas.set_draw_color(Theme::c(state.theme.track_bg_alt));
            let _ = canvas.fill_rect(Rect::new(
                lane_left,
                y,
                lane_area_w as u32,
                track_height as u32,
            ));
        }
        y += track_height;
    }

    // Grid lines AFTER backgrounds — show major bars, beat lines, and sub-divisions matching snap grid
    let beat_px = zoom;
    let snap_beats = state.snap.resolution_beats();
    let start_beat = scroll_x.floor() as i32;
    let end_beat = start_beat + (lane_area_w as f64 / beat_px) as i32 + 2;

    for beat in start_beat..end_beat {
        if beat < 0 {
            continue;
        }
        let x = lane_left + ((beat as f64 - scroll_x) * beat_px) as i32;
        if x < lane_left || x > w {
            continue;
        }

        let is_bar = beat % 4 == 0;
        let color = if is_bar {
            Theme::c(state.theme.grid_line_strong)
        } else {
            Theme::c(state.theme.grid_line)
        };
        canvas.set_draw_color(color);
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(x, top),
            sdl2::rect::Point::new(x, top + state.track_area_height()),
        );
    }

    // Sub-division grid lines (snap resolution finer than 1 beat)
    if snap_beats < 1.0 && beat_px > 8.0 {
        let mut sub = scroll_x - (scroll_x % snap_beats);
        while sub < scroll_x + (lane_area_w as f64 / beat_px) + 2.0 {
            // Only draw lines that aren't already on a beat boundary
            let on_beat = (sub % 1.0).abs() < 1e-6 || (sub % 1.0 - 1.0).abs() < 1e-6;
            if !on_beat {
                let x = lane_left + ((sub - scroll_x) * beat_px) as i32;
                if x > lane_left && x < w {
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        state.theme.grid_line[0],
                        state.theme.grid_line[1],
                        state.theme.grid_line[2],
                        50,
                    ));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(x, top),
                        sdl2::rect::Point::new(x, top + state.track_area_height()),
                    );
                }
            }
            sub += snap_beats;
        }
    }

    // Track lanes + clips
    let mut y = top - scroll_y;

    for track_idx in 0..track_count {
        let track_height = state.project.tracks[track_idx].height;
        let track_id = state.project.tracks[track_idx].id;

        if y + track_height < top {
            y += track_height;
            continue;
        }
        if y > top + state.track_area_height() {
            break;
        }

        // Draw clips (no longer draw backgrounds here, already done above)

        let clip_count = state.project.tracks[track_idx].clips.len();

        // Pre-compute which clip index is topmost (highest ci) under the mouse,
        // so that when clips overlap only the visually-topmost clip receives clicks.
        let topmost_hovered_ci: Option<usize> = {
            let mut top_ci: Option<usize> = None;
            for ci in 0..clip_count {
                let clip_start = state.project.tracks[track_idx].clips[ci].start_time();
                let clip_len = state.project.tracks[track_idx].clips[ci].length();
                let cx = lane_left + ((clip_start - scroll_x) * zoom) as i32;
                let cw = (clip_len * zoom) as i32;
                let clip_y_pre = y + 2;
                let clip_h_pre = (track_height - 4).max(4);
                if input.mouse_in_rect(cx, clip_y_pre, cw.max(4), clip_h_pre)
                    && input.mouse_y < top + state.track_area_height()
                    && input.mouse_y < state.bottom_panel_y()
                {
                    top_ci = Some(ci); // last match wins = topmost drawn
                }
            }
            top_ci
        };

        // Draw clips
        for ci in 0..clip_count {
            let clip_start = state.project.tracks[track_idx].clips[ci].start_time();
            let clip_len = state.project.tracks[track_idx].clips[ci].length();
            let clip_color = state.project.tracks[track_idx].clips[ci].color();

            let cx = lane_left + ((clip_start - scroll_x) * zoom) as i32;
            let cw = (clip_len * zoom) as i32;
            if cx + cw < lane_left || cx > w {
                continue;
            }

            let clip_y = y + 2;
            let clip_h = (track_height - 4).max(4);
            let header_h = 20; // clip title bar height (thicker for easier grabbing)

            let clip_hover = input.mouse_in_rect(cx, clip_y, cw.max(4), clip_h)
                && input.mouse_y < top + state.track_area_height()
                && input.mouse_y < state.bottom_panel_y();
            let is_selected = state.selected_clip == Some((track_id, ci))
                || state.selected_clips.contains(&(track_id, ci));

            // ── Overlapping clip detection ──
            let clip_end = clip_start + clip_len;
            let overlaps_another = state.project.tracks[track_idx]
                .clips
                .iter()
                .enumerate()
                .any(|(other_ci, other_clip)| {
                    if other_ci == ci {
                        return false;
                    }
                    let os = other_clip.start_time();
                    let oe = os + other_clip.length();
                    // Overlaps if ranges intersect (not just touch)
                    clip_start < oe && clip_end > os
                });

            // ── Clip body background — type-specific colors ──
            let type_base: [u8; 4] = match &state.project.tracks[track_idx].clips[ci] {
                crate::app::models::Clip::Midi(_) => [40, 55, 90, 230], // blue-ish
                crate::app::models::Clip::Audio(_) => [35, 65, 45, 230], // green-ish
                crate::app::models::Clip::Automation(_) => [70, 60, 30, 230], // amber-ish
            };
            let bright = if clip_hover { 20u8 } else { 0u8 };
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                type_base[0].saturating_add(bright),
                type_base[1].saturating_add(bright),
                type_base[2].saturating_add(bright),
                type_base[3],
            ));
            let _ = canvas.fill_rect(Rect::new(cx, clip_y, cw.max(4) as u32, clip_h as u32));

            // Subtle Overlap warning
            if overlaps_another {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 60, 60, 100));
                let _ = canvas.draw_rect(Rect::new(cx, clip_y, cw.max(4) as u32, clip_h as u32));
                let _ = canvas.draw_rect(Rect::new(
                    cx + 1,
                    clip_y + 1,
                    (cw.max(4).saturating_sub(2)) as u32,
                    (clip_h.saturating_sub(2)) as u32,
                ));
            }

            // ── Clip header bar (darker strip at top, type-tinted) ──
            let hdr_r = type_base[0].saturating_sub(15);
            let hdr_g = type_base[1].saturating_sub(15);
            let hdr_b = type_base[2].saturating_sub(15);
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(hdr_r, hdr_g, hdr_b, 220));
            let _ = canvas.fill_rect(Rect::new(cx, clip_y, cw.max(4) as u32, header_h as u32));

            // ── Clone-zone indicator: subtle >> marks on right side of header ──
            // Show always so users can discover the zone, brighter when hovered.
            if cw > 30 {
                let header_hover_zone = clip_hit_test(input, cx, clip_y, cw, clip_h, header_h);
                let in_header_zone =
                    header_hover_zone == ClipHitZone::Header && topmost_hovered_ci == Some(ci);
                let arrow_alpha = if in_header_zone { 220u8 } else { 80u8 };
                let arrow_col = sdl2::pixels::Color::RGBA(255, 255, 255, arrow_alpha);
                canvas.set_draw_color(arrow_col);
                // Draw two small ">>" chevrons near right of header
                let ax = cx + cw - 16;
                let ay = clip_y + header_h / 2 - 2;
                // First chevron ">"
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(ax, ay),
                    sdl2::rect::Point::new(ax + 3, ay + 2),
                );
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(ax + 3, ay + 2),
                    sdl2::rect::Point::new(ax, ay + 4),
                );
                // Second chevron ">"
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(ax + 5, ay),
                    sdl2::rect::Point::new(ax + 8, ay + 2),
                );
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(ax + 8, ay + 2),
                    sdl2::rect::Point::new(ax + 5, ay + 4),
                );
            }

            // ── Clip type badge (tiny colored block on left) ──
            let badge_color = match &state.project.tracks[track_idx].clips[ci] {
                crate::app::models::Clip::Midi(_) => [120, 180, 255, 255],
                crate::app::models::Clip::Audio(_) => [100, 230, 140, 255],
                crate::app::models::Clip::Automation(_) => [240, 200, 80, 255],
            };
            // Draw type icon in clip header (small, left side)
            let icon_sz = (header_h - 4).clamp(6, 14);
            let icon_col =
                sdl2::pixels::Color::RGBA(badge_color[0], badge_color[1], badge_color[2], 230);
            let clip_track_type = state.project.tracks[track_idx].track_type;
            draw_track_type_icon(
                canvas,
                &clip_track_type,
                cx + 2,
                clip_y + 2,
                icon_sz,
                icon_col,
            );

            // ── Clip name (pixel-font label, double-click header to rename) ──
            {
                let name = state.project.tracks[track_idx].clips[ci].name().to_string();
                let text_x = cx + icon_sz + 5;
                let max_w = (cw - icon_sz - 10).max(0);
                let header_hover = input.mouse_in_rect(cx, clip_y, cw.max(4), header_h);
                // Unique tf id: encode track_idx + clip index  (avoid collision with track tf ids at 90000)
                let clip_tf_id: u32 = 91000 + (track_idx as u32) * 100 + ci as u32;
                let is_clip_renaming = state.text_field_active_id == clip_tf_id;

                // Double-click on clip header bar → start rename (only topmost clip wins)
                if header_hover
                    && input.mouse_pressed
                    && !input.consumed
                    && input.click_type == Some(crate::app::input::ClickType::Double)
                    && !input.shift()
                    && !input.ctrl()
                    && max_w > 4
                    && topmost_hovered_ci == Some(ci)
                {
                    state.text_field_active_id = clip_tf_id;
                    state.text_field_buffer = name.clone();
                    state.text_field_cursor = name.len();
                }

                if is_clip_renaming && max_w > 4 {
                    let mut buf = state.text_field_buffer.clone();
                    let mut cursor = state.text_field_cursor;
                    let mut active_id = state.text_field_active_id;
                    let (committed, new_val) = text_field(
                        canvas,
                        input,
                        &state.theme,
                        &TextFieldParams {
                            id: clip_tf_id,
                            x: text_x,
                            y: clip_y + 1,
                            width: max_w,
                            height: header_h - 2,
                            hint: None,
                        },
                        &name,
                        &mut active_id,
                        &mut buf,
                        &mut cursor,
                    );
                    state.text_field_active_id = active_id;
                    state.text_field_buffer = buf;
                    state.text_field_cursor = cursor;
                    if committed {
                        if let Some(new_name) = new_val {
                            let old_name = state.project.tracks[track_idx]
                                .clips
                                .get(ci)
                                .map(|c| c.name().to_string())
                                .unwrap_or_default();
                            if old_name != new_name {
                                let snapshot = state.project.clone();
                                match state.project.tracks[track_idx].clips.get_mut(ci) {
                                    Some(crate::app::models::Clip::Midi(m)) => {
                                        m.name = new_name;
                                    }
                                    Some(crate::app::models::Clip::Audio(a)) => {
                                        a.name = new_name;
                                    }
                                    Some(crate::app::models::Clip::Automation(a)) => {
                                        a.name = new_name;
                                    }
                                    None => {}
                                }
                                state.commands.push_undo_snapshot(snapshot, "Rename Clip");
                                state.dirty = true;
                            }
                        }
                    }
                } else if max_w > 4 {
                    let name_col = sdl2::pixels::Color::RGBA(240, 240, 240, 210);
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &name,
                        text_x,
                        clip_y + (header_h - 5) / 2,
                        max_w,
                        name_col,
                    );
                }
            }

            // ── Selection / hover border ──
            if is_selected {
                canvas.set_draw_color(Theme::c(state.theme.accent));
            } else {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                    clip_color[0].saturating_add(40),
                    clip_color[1].saturating_add(40),
                    clip_color[2].saturating_add(40),
                    255,
                ));
            }
            let _ = canvas.draw_rect(Rect::new(cx, clip_y, cw.max(4) as u32, clip_h as u32));

            // ── Track color strip (left edge of clip) ──
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                clip_color[0],
                clip_color[1],
                clip_color[2],
                200,
            ));
            let _ = canvas.fill_rect(Rect::new(cx, clip_y, 3, clip_h as u32));

            // ── MIDI note mini-preview (clamped within clip body) ──
            let body_top = clip_y + header_h;
            let body_h = (clip_h - header_h).max(1);
            let clip_right = cx + cw;

            if let crate::app::models::Clip::Midi(ref midi) =
                state.project.tracks[track_idx].clips[ci]
            {
                for note in &midi.notes {
                    // note positions are relative to the clip's start_time
                    let raw_nx = cx + ((note.start / clip_len) * cw as f64) as i32;
                    let raw_nw = ((note.length / clip_len) * cw as f64).max(2.0) as i32;

                    // Clamp to clip bounds
                    let note_left = raw_nx.max(cx + CLIP_HANDLE_W);
                    let note_right = (raw_nx + raw_nw).min(clip_right - CLIP_HANDLE_W);
                    if note_left >= note_right {
                        continue;
                    }

                    let pitch_t = note.pitch as f32 / 127.0;
                    let ny = body_top + body_h - (pitch_t * body_h as f32) as i32 - 2;
                    let ny = ny.clamp(body_top, body_top + body_h - 2);

                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 180));
                    let _ = canvas.fill_rect(Rect::new(
                        note_left,
                        ny,
                        (note_right - note_left) as u32,
                        2,
                    ));
                }
            }

            // ── Automation curve mini-preview ──
            if let crate::app::models::Clip::Automation(ref auto) =
                state.project.tracks[track_idx].clips[ci]
            {
                // Clip rendering to clip bounds, clamped to lane area (don't overlap headers)
                let prev_clip_rect = canvas.clip_rect();
                let clip_left = cx.max(lane_left);
                let clip_right_edge = cx + cw.max(4);
                let clip_draw_w = (clip_right_edge - clip_left).max(0);
                canvas.set_clip_rect(Rect::new(
                    clip_left,
                    clip_y,
                    clip_draw_w as u32,
                    clip_h as u32,
                ));
                if auto.points.len() >= 2 {
                    let auto_col = sdl2::pixels::Color::RGBA(240, 200, 80, 200);
                    canvas.set_draw_color(auto_col);
                    for pi in 1..auto.points.len() {
                        let p0 = &auto.points[pi - 1];
                        let p1 = &auto.points[pi];
                        // Use absolute beat position (truncate, don't scale)
                        let x0 = cx + (p0.time * zoom) as i32;
                        let y0 =
                            body_top + body_h - (p0.value.clamp(0.0, 1.0) * body_h as f32) as i32;
                        let x1 = cx + (p1.time * zoom) as i32;
                        let y1 =
                            body_top + body_h - (p1.value.clamp(0.0, 1.0) * body_h as f32) as i32;
                        let _ = canvas.draw_line(
                            sdl2::rect::Point::new(
                                x0.clamp(cx, clip_right),
                                y0.clamp(body_top, body_top + body_h),
                            ),
                            sdl2::rect::Point::new(
                                x1.clamp(cx, clip_right),
                                y1.clamp(body_top, body_top + body_h),
                            ),
                        );
                    }
                    // Draw points as small dots
                    for p in &auto.points {
                        let px = cx + (p.time * zoom) as i32;
                        let py =
                            body_top + body_h - (p.value.clamp(0.0, 1.0) * body_h as f32) as i32;
                        if px >= cx && px <= clip_right && py >= body_top && py <= body_top + body_h
                        {
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 240, 120, 240));
                            let _ = canvas.fill_rect(Rect::new(px - 1, py - 1, 3, 3));
                        }
                    }
                } else if auto.points.len() == 1 {
                    // Single point: draw a horizontal line at that value
                    let p = &auto.points[0];
                    let py = body_top + body_h - (p.value.clamp(0.0, 1.0) * body_h as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(240, 200, 80, 150));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(cx, py),
                        sdl2::rect::Point::new(clip_right, py),
                    );
                }
                // Restore the full clipping state, including an empty or disabled rect.
                canvas.set_clip_rect(prev_clip_rect);
            }

            // ── Audio waveform mini-preview ──
            if let crate::app::models::Clip::Audio(ref audio_clip) =
                state.project.tracks[track_idx].clips[ci]
            {
                let wave_data = state.waveform_cache.get(&audio_clip.source_file);
                if let Some((peaks, total_duration)) = wave_data {
                    let num_peaks = peaks.len();
                    if num_peaks > 0 && cw > 4 && *total_duration > 0.0 {
                        let draw_w = (cw - 4).max(1) as usize;
                        let center = body_top + body_h / 2;
                        let half_h = (body_h / 2 - 1).max(1) as f32;

                        // The clip is a window into the audio file.
                        // clip_len_beats * 60 / bpm = how many seconds of audio this clip plays.
                        // We map draw_w pixels linearly across those seconds starting at offset.
                        // Dragging the right handle shortens clip_len → fewer seconds shown (truncate).
                        // Dragging the left handle increases offset → start later in the file (truncate).
                        let bpm = state.project.tempo_map.bpm_at(0.0);
                        let clip_dur_secs = (audio_clip.length * 60.0 / bpm.max(1.0))
                            .min(*total_duration - audio_clip.offset)
                            .max(0.001);
                        let peaks_per_sec = num_peaks as f64 / *total_duration;
                        let offset_secs = audio_clip.offset;

                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(140, 255, 180, 230));
                        for px_i in 0..draw_w {
                            // Fraction through the clip's playback window
                            let frac = px_i as f64 / draw_w as f64;
                            // Corresponding position in the audio file (seconds)
                            let time_secs = offset_secs + frac * clip_dur_secs;

                            let peak_idx = (time_secs * peaks_per_sec) as usize;
                            if peak_idx >= num_peaks {
                                break;
                            }

                            let amp = (peaks[peak_idx].clamp(0.0, 1.0) * audio_clip.gain).min(1.0);
                            let bar_h = (amp * half_h) as i32;
                            if bar_h > 0 {
                                let bx = cx + 2 + px_i as i32;
                                let _ = canvas.fill_rect(Rect::new(
                                    bx,
                                    center - bar_h,
                                    1,
                                    (bar_h * 2) as u32,
                                ));
                            }
                        }
                    }
                } else if !audio_clip.source_file.is_empty() {
                    // No cache entry — draw a placeholder label
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "loading...",
                        cx + 4,
                        body_top + body_h / 2 - 4,
                        cw - 8,
                        sdl2::pixels::Color::RGBA(130, 220, 160, 120),
                    );
                }
            }

            // ── Resize handles ──
            let hover_zone = if clip_hover {
                clip_hit_test(input, cx, clip_y, cw, clip_h, header_h)
            } else {
                ClipHitZone::None
            };
            draw_clip_handles(
                canvas,
                &state.theme,
                cx,
                clip_y,
                cw,
                clip_h,
                hover_zone,
                is_selected,
            );

            if clip_hover
                && input.mouse_pressed
                && !input.consumed
                && input.active_widget == WidgetId::None
                && topmost_hovered_ci == Some(ci)
            {
                let zone = if input.shift() {
                    ClipHitZone::Body
                } else {
                    hover_zone
                };
                if matches!(zone, ClipHitZone::LeftHandle | ClipHitZone::RightHandle) {
                    if !state.selected_clips.contains(&(track_id, ci)) {
                        state.selected_clips.clear();
                        state.selected_clips.insert((track_id, ci));
                    }
                    state.selected_clip = Some((track_id, ci));
                    state.selected_track = Some(track_id);
                }
                match zone {
                    ClipHitZone::LeftHandle => {
                        input.active_widget = WidgetId::ClipLeftHandle(track_id, ci);
                        input.drag_widget = input.active_widget;
                    }
                    ClipHitZone::RightHandle => {
                        input.active_widget = WidgetId::ClipRightHandle(track_id, ci);
                        input.drag_widget = input.active_widget;
                    }
                    ClipHitZone::Header | ClipHitZone::Body => {
                        // Body and header share selection and movement semantics.
                        if input.shift() {
                            // Shift+click: toggle membership without moving clips.
                            if state.selected_clips.contains(&(track_id, ci)) {
                                state.selected_clips.remove(&(track_id, ci));
                                // Clear selected_clip so is_selected becomes false
                                if state.selected_clip == Some((track_id, ci)) {
                                    state.selected_clip = None;
                                }
                            } else {
                                state.selected_clips.insert((track_id, ci));
                                state.selected_clip = Some((track_id, ci));
                            }
                        } else {
                            // Select the clip if not already selected
                            if !state.selected_clips.contains(&(track_id, ci)) {
                                state.selected_clips.clear();
                                state.selected_clips.insert((track_id, ci));
                            }
                            state.selected_clip = Some((track_id, ci));
                            state.selected_track = Some(track_id);
                            state.selected_tracks.clear();
                            state.selected_tracks.insert(track_id);

                            if input.click_type == Some(crate::app::input::ClickType::Double)
                                && !input.ctrl()
                            {
                                // Header double-click renames; body double-click opens Edit.
                                if zone == ClipHitZone::Body {
                                    state.bottom_panel_tab = BottomPanelTab::PianoRoll;
                                    state.bottom_panel_open = true;
                                }
                                input.active_widget = WidgetId::None;
                            } else if input.ctrl() {
                                // Ctrl+drag = copy.
                                state.clip_drag_is_copy = true;
                                input.active_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_start_value = clip_start;
                            } else {
                                // Normal drag = move.
                                state.clip_drag_is_copy = false;
                                input.active_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_start_value = clip_start;
                            }

                        }
                    }
                    ClipHitZone::None => {}
                }
                if matches!(
                    input.drag_widget,
                    WidgetId::ClipBody(_, _)
                        | WidgetId::ClipLeftHandle(_, _)
                        | WidgetId::ClipRightHandle(_, _)
                ) {
                    state.begin_clip_drag(track_id, ci);
                }
                input.consume();
            }
        }

        // Lane separator
        canvas.set_draw_color(Theme::c(state.theme.grid_line));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(lane_left, y + track_height - 1),
            sdl2::rect::Point::new(w, y + track_height - 1),
        );

        // ── VU Meter strip (right edge of track lane) ──
        let rms = state
            .meters
            .track_rms
            .get(track_idx)
            .copied()
            .unwrap_or(0.0);
        if rms > 0.001
            || state
                .meters
                .track_clipping
                .get(track_idx)
                .copied()
                .unwrap_or(false)
        {
            let meter_w = 4i32;
            let meter_h = (track_height - 6).max(2);
            let meter_x = w - meter_w - 2;
            let meter_y = y + 3;
            // Use the same algorithm as draw_meter_bar / mixer meters:
            //   DB_FLOOR = -60, DB_CEIL = +12  (72 dB range)
            //   Colour via meter_color() for a consistent gradient everywhere.
            const DB_FLOOR: f64 = -60.0;
            const DB_CEIL: f64 = 12.0;
            const DB_RANGE: f64 = DB_CEIL - DB_FLOOR; // 72
                                                      // Background
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 20, 160));
            let _ = canvas.fill_rect(Rect::new(meter_x, meter_y, meter_w as u32, meter_h as u32));
            let db = if rms > 1e-6 {
                20.0 * (rms as f64).log10()
            } else {
                DB_FLOOR
            };
            let db_clamped = db.clamp(DB_FLOOR, DB_CEIL);
            let frac = ((db_clamped - DB_FLOOR) / DB_RANGE) as f32; // 0.0 → 1.0
            let fill_h = (frac * meter_h as f32) as i32;
            // Segmented fill with meter_color(), same as draw_meter_bar
            for row in 0..fill_h {
                let py = meter_y + meter_h - 1 - row;
                let row_frac = row as f64 / meter_h as f64;
                let row_db = DB_FLOOR + row_frac * DB_RANGE;
                let col = meter_color(row_db);
                canvas.set_draw_color(col);
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(meter_x, py),
                    sdl2::rect::Point::new(meter_x + meter_w - 1, py),
                );
            }
            // Clip indicator dot
            if state
                .meters
                .track_clipping
                .get(track_idx)
                .copied()
                .unwrap_or(false)
            {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 30, 30, 255));
                let _ = canvas.fill_rect(Rect::new(meter_x, meter_y - 1, meter_w as u32, 3));
            }
        }

        y += track_height;
    }

    // ── Right-click clip to delete (press or drag-erase) + double-click empty lane to create clip ──
    {
        let mut y2 = top - scroll_y;
        let mut did_right_click_clip = false;
        for track_idx in 0..state.project.tracks.len() {
            let track_height = state.project.tracks[track_idx].height;
            let track_id = state.project.tracks[track_idx].id;
            let track_type = state.project.tracks[track_idx].track_type;

            if y2 + track_height < top {
                y2 += track_height;
                continue;
            }
            if y2 > top + state.track_area_height() {
                break;
            }

            let clip_count = state.project.tracks[track_idx].clips.len();

            // Find the TOPMOST (last drawn = highest index) clip under the cursor
            // Delete on right-click press OR right-click drag (erase mode)
            // For drag-erase, require mouse movement to avoid cascade-deleting stacked clips
            #[allow(clippy::nonminimal_bool)]
            if input.right_mouse_pressed
                || (input.right_mouse_down
                    && !input.right_mouse_pressed
                    && (input.mouse_dx != 0 || input.mouse_dy != 0))
            {
                let mut top_clip_idx: Option<usize> = None;
                for ci in 0..clip_count {
                    let clip_start = state.project.tracks[track_idx].clips[ci].start_time();
                    let clip_len = state.project.tracks[track_idx].clips[ci].length();
                    let cx = lane_left + ((clip_start - scroll_x) * zoom) as i32;
                    let cw = (clip_len * zoom) as i32;
                    let clip_y = y2 + 2;
                    let clip_h = (track_height - 4).max(4);
                    if input.mouse_in_rect(cx, clip_y, cw.max(4), clip_h) {
                        top_clip_idx = Some(ci); // last match = topmost
                    }
                }
                if let Some(ci) = top_clip_idx {
                    // Right-click on clip → delete it (undoable)
                    let clip_data = state.project.tracks[track_idx].clips[ci].clone();
                    state.commands.execute(
                        Box::new(crate::app::commands::DeleteClips {
                            clips: vec![(track_id, ci, clip_data)],
                        }),
                        &mut state.project,
                    );
                    // Update selected_clip
                    if state.selected_clip == Some((track_id, ci)) {
                        state.selected_clip = None;
                    } else if let Some((sid, sci)) = state.selected_clip {
                        if sid == track_id && sci > ci {
                            state.selected_clip = Some((sid, sci - 1));
                        }
                    }
                    // Rebuild selected_clips for this track: remove deleted, shift higher indices down
                    let old_sel: Vec<(u32, usize)> = state.selected_clips.iter().cloned().collect();
                    state.selected_clips.clear();
                    for (tid, idx) in old_sel {
                        if tid == track_id {
                            match idx.cmp(&ci) {
                                std::cmp::Ordering::Equal => {
                                    // deleted — drop it
                                }
                                std::cmp::Ordering::Greater => {
                                    state.selected_clips.insert((tid, idx - 1));
                                }
                                std::cmp::Ordering::Less => {
                                    state.selected_clips.insert((tid, idx));
                                }
                            }
                        } else {
                            state.selected_clips.insert((tid, idx));
                        }
                    }
                    state.dirty = true;
                    did_right_click_clip = true;
                    // Mark consumed on press so we don't delete again next frame
                    if input.right_mouse_pressed {
                        input.consumed = true;
                    }
                }
            }

            if did_right_click_clip {
                break;
            }

            // Double-click on empty lane → create new 1-bar clip of the track's type
            let in_lane = input.mouse_in_rect(lane_left, y2, w - lane_left, track_height)
                && input.mouse_y < top + state.track_area_height();
            if in_lane
                && input.mouse_pressed
                && !input.consumed
                && input.click_type == Some(crate::app::input::ClickType::Double)
            {
                // Check that no existing clip was hit at this position
                let mut hit_existing_clip = false;
                for ci in 0..clip_count {
                    let cs = state.project.tracks[track_idx].clips[ci].start_time();
                    let cl = state.project.tracks[track_idx].clips[ci].length();
                    let cx = lane_left + ((cs - scroll_x) * zoom) as i32;
                    let cw = (cl * zoom) as i32;
                    let clip_y = y2 + 2;
                    let clip_h = (track_height - 4).max(4);
                    if input.mouse_in_rect(cx, clip_y, cw.max(4), clip_h) {
                        hit_existing_clip = true;
                        break;
                    }
                }
                if hit_existing_clip {
                    y2 += track_height;
                    continue;
                }
                let beat = scroll_x + (input.mouse_x - lane_left) as f64 / zoom;
                let start = state.snap.snap(beat).max(0.0);
                let clip_name = "Clip".to_string();
                let track_color = state.project.tracks[track_idx].color;
                let new_clip = match track_type {
                    crate::app::models::TrackType::Midi => {
                        crate::app::models::Clip::Midi(crate::app::models::MidiClip {
                            name: clip_name,
                            color: track_color,
                            start_time: start,
                            length: 4.0, // 1 bar
                            notes: Vec::new(),
                        })
                    }
                    crate::app::models::TrackType::Audio => {
                        crate::app::models::Clip::Audio(crate::app::models::AudioClip {
                            name: clip_name,
                            color: track_color,
                            start_time: start,
                            length: 4.0, // 1 bar
                            source_file: String::new(),
                            offset: 0.0,
                            gain: 1.0,
                            fade_in: 0.0,
                            fade_out: 0.0,
                        })
                    }
                    crate::app::models::TrackType::Automation => {
                        crate::app::models::Clip::Automation(crate::app::models::AutomationClip {
                            name: clip_name,
                            color: track_color,
                            start_time: start,
                            length: 4.0, // 1 bar
                            points: Vec::new(),
                            target_param: "volume".to_string(),
                        })
                    }
                };
                state.commands.execute(
                    Box::new(crate::app::commands::CreateClip {
                        track_id,
                        clip: new_clip,
                        added_idx: 0,
                    }),
                    &mut state.project,
                );
                // Open edit panel for the new clip
                let new_ci = state
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .map(|t| t.clips.len().saturating_sub(1))
                    .unwrap_or(0);
                state.selected_clip = Some((track_id, new_ci));
                state.selected_clips.clear();
                state.selected_clips.insert((track_id, new_ci));
                state.bottom_panel_tab = BottomPanelTab::PianoRoll;
                if !state.bottom_panel_open {
                    state.bottom_panel_open = true;
                    state.bottom_panel_height = 320;
                }
                state.dirty = true;
            }

            y2 += track_height;
        }
    }

    // ── Drop generator module onto lane area below all tracks → create MIDI track ──
    // (Add-track buttons live in draw_track_headers; this handles drag-drop only)
    {
        let mut last_track_y = top - scroll_y;
        for t in &state.project.tracks {
            last_track_y += t.height;
        }
        let below_y = last_track_y;
        if input.mouse_released
            && state.module_drag.is_some()
            && input.mouse_y < state.bottom_panel_y()
            && input.mouse_y >= below_y
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

    // Handle clip drag (move / resize)
    handle_clip_drag(state, input, zoom, scroll_x, lane_left);

    // ── Loop region overlay — edge lines ONLY (only when loop enabled) ──
    if state.project.transport.loop_enabled {
        let lr_start = state.project.transport.loop_region.start;
        let lr_end = state.project.transport.loop_region.end;
        let lc = state.theme.loop_region;
        let lx1 = lane_left + ((lr_start - scroll_x) * zoom) as i32;
        let lx2 = lane_left + ((lr_end - scroll_x) * zoom) as i32;
        let area_h = state.track_area_height();
        let edge_alpha = 200u8;
        let handle_alpha = 255u8;
        let sc = state.ui_scale;
        let handle_w = ((10.0 * sc) as i32).max(8);
        let tab_h = ((20.0 * sc) as i32).max(14);

        // Start: 2px edge line across full height + solid flag tab at top
        if lx1 >= lane_left - handle_w && lx1 <= w {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], edge_alpha));
            let _ = canvas.fill_rect(Rect::new(lx1, top, 2, area_h as u32));

            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], handle_alpha));
            let _ = canvas.fill_rect(Rect::new(lx1, top, handle_w as u32, tab_h as u32));
            // Dark right-arrow in the tab
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(15, 15, 15, 220));
            for row in 0..5i32 {
                let tw = ((row + 1) / 2 + 1).min(5) as u32;
                let _ = canvas.fill_rect(Rect::new(lx1 + 2, top + 3 + row * 2, tw, 2));
            }
        }

        // End: 2px edge line + solid flag tab at top (left-pointing)
        if lx2 >= lane_left && lx2 <= w + handle_w {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], edge_alpha));
            let _ = canvas.fill_rect(Rect::new(lx2 - 2, top, 2, area_h as u32));

            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], handle_alpha));
            let _ = canvas.fill_rect(Rect::new(
                lx2 - handle_w,
                top,
                handle_w as u32,
                tab_h as u32,
            ));
            // Dark left-arrow in the tab
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(15, 15, 15, 220));
            for row in 0..5i32 {
                let tw = ((row + 1) / 2 + 1).min(5) as u32;
                let _ = canvas.fill_rect(Rect::new(lx2 - 2 - tw as i32, top + 3 + row * 2, tw, 2));
            }
        }
    }

    // Playhead line over tracks + auto-follow during playback
    let playhead_beat = state.project.transport.position;
    // Auto-scroll to follow playhead during playback (only if enabled)
    if state.project.transport.playing && state.follow_playhead {
        let visible_beats = lane_area_w as f64 / zoom;
        let margin = visible_beats * 0.1; // 10% margin from right edge
        if playhead_beat > scroll_x + visible_beats - margin {
            state.arrangement.scroll_x = playhead_beat - visible_beats * 0.25;
        } else if playhead_beat < scroll_x {
            state.arrangement.scroll_x = (playhead_beat - margin).max(0.0);
        }
    }
    let scroll_x = state.arrangement.scroll_x; // re-read after possible update
    let px = lane_left + ((playhead_beat - scroll_x) * zoom) as i32;
    if px >= lane_left && px <= w {
        canvas.set_draw_color(Theme::c(state.theme.playhead));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(px, top),
            sdl2::rect::Point::new(px, top + state.track_area_height()),
        );
    }

    // Scroll and zoom
    let in_lane = input.mouse_in_rect(lane_left, top, lane_area_w, state.track_area_height())
        && input.mouse_y < state.bottom_panel_y();
    // Also allow Ctrl+scroll zoom when cursor is over the ruler/header area above lanes
    let transport_h = state.transport_bar_height();
    let in_arranger_wide = input.mouse_y >= transport_h
        && input.mouse_y < state.bottom_panel_y()
        && input.mouse_x >= lane_left;

    if (in_lane || (in_arranger_wide && input.ctrl() && input.scroll_y != 0))
        && !input.scroll_consumed
    {
        if input.scroll_y != 0 {
            if input.shift() {
                // Shift+scroll: resize all tracks
                let factor = if input.scroll_y > 0 { 1.1 } else { 0.9 };
                let snapshot = state.project.clone();
                for track in &mut state.project.tracks {
                    track.height = ((track.height as f32 * factor).max(80.0)) as i32;
                }
                state
                    .commands
                    .push_undo_snapshot(snapshot, "Resize All Tracks");
                state.dirty = true;
            } else if input.ctrl() {
                // Ctrl+scroll: zoom horizontally, keeping the beat under the cursor fixed
                let factor = if input.scroll_y > 0 { 1.15 } else { 0.87 };
                let old_zoom = state.arrangement.zoom_x;
                let new_zoom = (old_zoom * factor).clamp(1.0, 1000.0);
                // beat position under the mouse cursor
                let cursor_offset_px = (input.mouse_x - lane_left) as f64;
                let beat_under_cursor = state.arrangement.scroll_x + cursor_offset_px / old_zoom;
                // adjust scroll so that same beat stays under cursor at new zoom
                state.arrangement.scroll_x =
                    (beat_under_cursor - cursor_offset_px / new_zoom).max(0.0);
                state.arrangement.zoom_x = new_zoom;
            } else {
                // Normal scroll: vertical pan
                let max_sy = state.max_arrangement_scroll_y();
                state.arrangement.scroll_y =
                    (state.arrangement.scroll_y - input.scroll_y * 30).clamp(0, max_sy);
            }
        }
        if input.scroll_x != 0 {
            state.arrangement.scroll_x =
                (state.arrangement.scroll_x - input.scroll_x as f64 * 2.0).max(0.0);
        }
    }

    // Middle mouse drag to pan (start when pressing in lane area, continue even if mouse leaves)
    if input.middle_mouse_down
        && input.mouse_in_rect(lane_left, top, lane_area_w, state.track_area_height())
        && input.mouse_y < state.bottom_panel_y()
        && input.middle_drag_widget == WidgetId::None
    {
        input.middle_drag_widget = WidgetId::Auto(85099);
    }
    if input.middle_mouse_down && input.middle_drag_widget == WidgetId::Auto(85099) {
        let max_sy = state.max_arrangement_scroll_y();
        state.arrangement.scroll_x =
            (state.arrangement.scroll_x - input.mouse_dx as f64 / zoom).max(0.0);
        state.arrangement.scroll_y = (state.arrangement.scroll_y - input.mouse_dy).clamp(0, max_sy);
    }

    // ── Ctrl+drag rubberband selection ──────────────────────────────
    let in_lane_area = input.mouse_in_rect(lane_left, top, lane_area_w, state.track_area_height())
        && input.mouse_y < top + state.track_area_height()
        && input.mouse_y < state.bottom_panel_y();

    // ── Focus: clicking in the arrangement lane area claims keyboard focus ─────
    if in_lane_area && input.mouse_pressed && !input.consumed {
        state.focused_panel = crate::app::state::FocusedPanel::Arrangement;

        // Click on empty background (no clip or widget hit) deselects everything
        // unless shift or ctrl is held, or the bottom panel is showing a clip editor.
        // Preserve the currently-focused editor clip (MIDI, Audio, or Automation)
        // so clicking the arranger background doesn't close the active editor.
        let preserve_clip = state.bottom_panel_open
            && state.selected_clip.is_some_and(|(tid, ci)| {
                state
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == tid)
                    .and_then(|t| t.clips.get(ci))
                    .is_some()
            });
        if input.drag_widget == WidgetId::None
            && input.active_widget == WidgetId::None
            && !input.shift()
            && !input.ctrl()
        {
            state.selected_clips.clear();
            if !preserve_clip {
                state.selected_clip = None;
            }
        }
    }

    // Start rubberband on Ctrl+click in lane area (no clip header was clicked).
    // Body clicks pass through so rubber-band works from inside clips too.
    if in_lane_area
        && input.ctrl()
        && input.mouse_pressed
        && !input.consumed
        && input.drag_widget == WidgetId::None
        && input.active_widget == WidgetId::None
    {
        input.drag_widget = WidgetId::Rubberband;
        input.active_widget = WidgetId::Rubberband;
        state.rubberband = Some((input.mouse_x, input.mouse_y, input.mouse_x, input.mouse_y));
        if input.shift() {
            // Shift+rubber-band: preserve existing selection as base, then append/toggle
            state.rubberband_pre_selection = state.selected_clips.clone();
        } else {
            state.selected_clips.clear();
            state.rubberband_pre_selection.clear();
        }
    }

    // Update rubberband extent while dragging
    if input.drag_widget == WidgetId::Rubberband && input.mouse_down {
        // Compute clamp bounds before taking the mutable borrow on state.rubberband
        let rb_clamp_x1 = lane_left;
        let rb_clamp_x2 = lane_left + lane_area_w;
        let rb_clamp_y1 = top;
        let rb_clamp_y2 = (top + state.track_area_height()).min(state.bottom_panel_y());
        if let Some(ref mut rb) = state.rubberband {
            // Clamp the drag extent to the visible lane area so the rect doesn't
            // grow outside the window when the mouse leaves the arrangement.
            rb.2 = input.mouse_x.clamp(rb_clamp_x1, rb_clamp_x2);
            rb.3 = input.mouse_y.clamp(rb_clamp_y1, rb_clamp_y2);
        }
        // Live-select clips inside the rubberband
        if let Some((rx1, ry1, rx2, ry2)) = state.rubberband {
            let sel_x1 = rx1.min(rx2);
            let sel_x2 = rx1.max(rx2);
            let sel_y1 = ry1.min(ry2);
            let sel_y2 = ry1.max(ry2);
            // Collect clips currently inside the band
            let mut in_band: std::collections::HashSet<(u32, usize)> =
                std::collections::HashSet::new();
            let mut ty = top - state.arrangement.scroll_y;
            for track in &state.project.tracks {
                for (ci, clip) in track.clips.iter().enumerate() {
                    let cs = clip.start_time();
                    let cl = clip.length();
                    let cx = lane_left + ((cs - scroll_x) * zoom) as i32;
                    let cw = (cl * zoom) as i32;
                    let clip_y2 = ty + track.height;
                    // Overlap test
                    if cx < sel_x2 && cx + cw > sel_x1 && ty < sel_y2 && clip_y2 > sel_y1 {
                        in_band.insert((track.id, ci));
                    }
                }
                ty += track.height;
            }
            if input.shift() {
                // Shift+rubber-band: start from pre-selection snapshot.
                // Clips inside the band that were NOT in pre-selection → add them.
                // Clips inside the band that WERE in pre-selection → remove them (toggle).
                let mut new_sel = state.rubberband_pre_selection.clone();
                for id in &in_band {
                    if state.rubberband_pre_selection.contains(id) {
                        new_sel.remove(id);
                    } else {
                        new_sel.insert(*id);
                    }
                }
                state.selected_clips = new_sel;
            } else {
                state.selected_clips = in_band;
            }
        }
    }

    // Release rubberband on any mouse release
    if input.mouse_released
        && (input.drag_widget == WidgetId::Rubberband || state.rubberband.is_some())
    {
        state.rubberband = None;
        if input.drag_widget == WidgetId::Rubberband {
            input.drag_widget = WidgetId::None;
        }
    }

    // Reset ctrl+drag copy state on release
    if input.mouse_released {
        state.clip_drag_is_copy = false;
        state.clip_drag_ghost_positions.clear();
        state.clip_drag_target_track = None;
        state.clip_drag_target_valid = false;
        // Clear drag snapshots — they're only valid during an active drag.
        state.clip_drag_baseline.clear();
    }

    // ── Draw clip drag preview (actual clip appearance, red for invalid cross-track) ──
    if !state.clip_drag_ghost_positions.is_empty() {
        let ghosts = state.clip_drag_ghost_positions.clone();
        let target_valid = state.clip_drag_target_valid;
        let mut gy = top - scroll_y;
        for track in &state.project.tracks {
            let th = track.height;
            for &(display_tid, src_tid, g_ci, g_start) in &ghosts {
                if track.id == display_tid {
                    // Look up clip data from source track (may differ from display track)
                    let clip_opt = state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == src_tid)
                        .and_then(|t| t.clips.get(g_ci));
                    if let Some(clip) = clip_opt {
                        let cl = clip.length();
                        let gx = lane_left + ((g_start - scroll_x) * zoom) as i32;
                        let gw = (cl * zoom) as i32;

                        // Determine if this is a cross-track drag to an incompatible track
                        let show_invalid = !target_valid;

                        if show_invalid {
                            // Red block for invalid cross-track target
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 40, 40, 180));
                            let _ = canvas.fill_rect(Rect::new(
                                gx,
                                gy + 2,
                                gw.max(4) as u32,
                                (th - 4).max(4) as u32,
                            ));
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 80, 80, 255));
                            let _ = canvas.draw_rect(Rect::new(
                                gx,
                                gy + 2,
                                gw.max(4) as u32,
                                (th - 4).max(4) as u32,
                            ));
                        } else {
                            // Draw clip with normal appearance (solid, opaque)
                            let gc = clip.color();
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                                gc[0], gc[1], gc[2], 220,
                            ));
                            let _ = canvas.fill_rect(Rect::new(
                                gx,
                                gy + 2,
                                gw.max(4) as u32,
                                (th - 4).max(4) as u32,
                            ));
                            // Solid border
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 180));
                            let _ = canvas.draw_rect(Rect::new(
                                gx,
                                gy + 2,
                                gw.max(4) as u32,
                                (th - 4).max(4) as u32,
                            ));
                            // Clip name label
                            let name = clip.name();
                            if gw > 30 {
                                draw_pixel_label(
                                    canvas,
                                    &state.theme,
                                    name,
                                    gx + 4,
                                    gy + 6,
                                    gw - 8,
                                    sdl2::pixels::Color::RGBA(255, 255, 255, 200),
                                );
                            }
                        }
                    }
                }
            }
            gy += th;
        }
    }

    // ── Arrangement vertical scrollbar (right edge of lane area) ──
    // Always visible so the user knows it exists. When all content fits, the
    // thumb fills the full length and dragging has no effect.
    {
        let total_h = state.total_tracks_content_height();
        let visible_h = state.track_area_height();
        if visible_h > 20 {
            let sb_w = 14i32;
            let sb_x = w - sb_w;
            let sb_top = state.transport_bar_height(); // flush to top, under rulers
            let sb_len = visible_h + (top - sb_top); // extend upward
            let max_scroll = state.max_arrangement_scroll_y().max(1);
            let frac = state.arrangement.scroll_y as f32 / max_scroll as f32;
            let thumb_ratio = (visible_h as f32 / total_h as f32).clamp(0.02, 1.0);
            // Clear clip rect so scrollbar is drawn outside the lane clip area
            canvas.set_clip_rect(None);
            let new_frac = scrollbar(
                canvas,
                input,
                &state.theme,
                WidgetId::Auto(85000),
                sb_x,
                sb_top,
                sb_len,
                sb_w,
                ScrollbarDir::Vertical,
                frac.clamp(0.0, 1.0),
                thumb_ratio,
            );
            state.arrangement.scroll_y = (new_frac * max_scroll as f32) as i32;
            // Restore clip rect for rubberband
            canvas.set_clip_rect(Rect::new(
                lane_left,
                top,
                lane_area_w as u32,
                state.track_area_height() as u32,
            ));
        }
    }

    // ── Arrangement horizontal scroomer (bottom of lane area) ──
    {
        canvas.set_clip_rect(None);
        let sb_h = 14i32;
        let sb_y = top + state.track_area_height() - sb_h;
        let sb_x = lane_left;
        let sb_len = lane_area_w - 14; // leave room for vertical scrollbar

        // Total content beats: max clip end + generous buffer
        let total_beats = {
            let mut max_b = 64.0f64;
            for track in &state.project.tracks {
                for clip in &track.clips {
                    let end = match clip {
                        crate::app::models::Clip::Midi(m) => m.start_time + m.length,
                        crate::app::models::Clip::Audio(a) => a.start_time + a.length,
                        crate::app::models::Clip::Automation(a) => a.start_time + a.length,
                    };
                    if end > max_b {
                        max_b = end;
                    }
                }
            }
            (max_b * 1.25).max(64.0) // 25% buffer
        };

        let visible_beats = lane_area_w as f64 / zoom;
        let thumb_ratio = (visible_beats / total_beats).clamp(0.02, 1.0) as f32;
        let max_scroll_beats = (total_beats - visible_beats).max(0.001);
        let scroll_frac = (scroll_x / max_scroll_beats).clamp(0.0, 1.0) as f32;

        let (new_frac, new_ratio) = scrollbar_with_squeeze(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(85010),
            WidgetId::Auto(85011),
            WidgetId::Auto(85012),
            sb_x,
            sb_y,
            sb_len,
            sb_h,
            ScrollbarDir::Horizontal,
            scroll_frac,
            thumb_ratio,
        );

        // Only update zoom/scroll from scroomer when the user actually interacted with it
        // (squeeze handles change the ratio, scrollbar drag changes the fraction).
        // This prevents the scroomer from overwriting external zoom changes (e.g. Ctrl+scroll).
        let ratio_changed = (new_ratio - thumb_ratio).abs() > 0.001;
        let frac_changed = (new_frac - scroll_frac).abs() > 0.001;
        if ratio_changed {
            let new_visible_beats = (new_ratio as f64 * total_beats).max(1.0);
            let new_zoom = (lane_area_w as f64 / new_visible_beats).clamp(1.0, 1000.0);
            state.arrangement.zoom_x = new_zoom;
        }
        if ratio_changed || frac_changed {
            let cur_zoom = state.arrangement.zoom_x;
            let new_max_scroll_beats = (total_beats - lane_area_w as f64 / cur_zoom).max(0.0);
            state.arrangement.scroll_x = (new_frac as f64 * new_max_scroll_beats).max(0.0);
        }
    }

    // Draw rubberband rectangle (thin outline only — no fill)
    if let Some((rx1, ry1, rx2, ry2)) = state.rubberband {
        let sx = rx1.min(rx2);
        let sy = ry1.min(ry2).max(top);
        let sw = (rx1 - rx2).unsigned_abs();
        let sh = (ry1 - ry2).unsigned_abs();
        let ac = state.theme.accent;
        // Clip to lane area (prevents overflow over headers/bottom panel)
        canvas.set_clip_rect(Some(Rect::new(
            lane_left,
            top,
            lane_area_w as u32,
            state.track_area_height() as u32,
        )));
        // Thin transparent outline only — no filled rectangle
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(ac[0], ac[1], ac[2], 180));
        let _ = canvas.draw_rect(Rect::new(sx, sy, sw, sh));
    }

    // Clear clip rect
    canvas.set_clip_rect(None);
}

fn handle_clip_drag(
    state: &mut AppState,
    input: &InputState,
    zoom: f64,
    _scroll_x: f64,
    _header_w: i32,
) {
    state.update_clip_drag(input, zoom);
}

// ── Bottom panel ─────────────────────────────────────────────────────
