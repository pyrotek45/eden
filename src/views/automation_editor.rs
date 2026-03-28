// Eden DAW — Views: automation_editor

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::{InputState, WidgetId};
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

pub(super) fn draw_automation_editor(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    // ── Layout constants ──
    const TOOLBAR_H: i32 = 22;
    const RULER_H: i32 = 20;
    const LABEL_W: i32 = 40; // left value axis width
    const SCROLL_T: i32 = 14;

    let toolbar_top = top;
    let ruler_top = top + TOOLBAR_H;
    let grid_top = ruler_top + RULER_H;
    let grid_h = h - TOOLBAR_H - RULER_H - SCROLL_T;
    let grid_w = w - LABEL_W - SCROLL_T;

    // ── Background ──
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 18, 22, 255));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // ── Toolbar ──
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(32, 34, 40, 255));
    let _ = canvas.fill_rect(Rect::new(0, toolbar_top, w as u32, TOOLBAR_H as u32));
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, toolbar_top + TOOLBAR_H - 1),
        sdl2::rect::Point::new(w, toolbar_top + TOOLBAR_H - 1),
    );

    let Some((track_id, clip_idx)) = state.selected_clip else {
        draw_pixel_label(
            canvas,
            &state.theme,
            "No automation clip selected",
            10,
            toolbar_top + 6,
            w - 20,
            Theme::c(state.theme.text_dim),
        );
        return;
    };

    // Gather clip info
    let clip_data = {
        state
            .project
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| t.clips.get(clip_idx))
            .and_then(|c| {
                if let crate::app::models::Clip::Automation(ac) = c {
                    Some((ac.target_param.clone(), ac.start_time, ac.length))
                } else {
                    None
                }
            })
    };
    let Some((param_name, clip_start, clip_len)) = clip_data else {
        draw_pixel_label(
            canvas,
            &state.theme,
            "Selected clip is not automation",
            10,
            toolbar_top + 6,
            w - 20,
            Theme::c(state.theme.text_dim),
        );
        return;
    };

    // ── Toolbar buttons ──
    let tb_y = toolbar_top + 2;
    let tb_h = TOOLBAR_H - 4;
    let mut tb_x = 4i32;

    // Param name label
    {
        let info = format!("AUTO: {}", param_name);
        let info_w = info.len() as i32 * 8 + 8;
        draw_pixel_label(
            canvas,
            &state.theme,
            &info,
            tb_x,
            tb_y + 4,
            info_w,
            Theme::c(state.theme.text_primary),
        );
        tb_x += info_w + 8;
    }

    // "GO TO RACK" button - opens the rack panel and highlights the target param
    {
        let go_btn = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(80032),
                x: tb_x,
                y: tb_y,
                width: 70,
                height: tb_h,
                label: "GO TO RACK".to_string(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Open rack and highlight the automated parameter".into()),
                ..Default::default()
            },
        );
        if go_btn {
            // Parse target_param format: "track_id:slot_id:param_id"
            let parts: Vec<&str> = param_name.split(':').collect();
            if parts.len() == 3 {
                if let (Ok(tid), Ok(sid)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    let pid = parts[2].to_string();
                    // Find track index for selection
                    if let Some(ti) = state.project.tracks.iter().position(|t| t.id == tid) {
                        state.selected_track = Some(tid);
                        state.selected_tracks.clear();
                        state.selected_tracks.insert(tid);
                        state.rack_highlight_param = Some((tid, sid, pid));
                        state.rack_highlight_timer = 180; // ~3 seconds at 60fps
                                                          // Show the rack by switching to the RACK tab and opening the panel
                        state.bottom_panel_tab = BottomPanelTab::InstrumentRack;
                        if !state.bottom_panel_open {
                            state.bottom_panel_open = true;
                        }
                        state.mode = crate::app::state::AppMode::Edit;
                        // Also select this track in the project so the rack shows
                        let _ = ti; // Silence unused warning
                    }
                }
            }
        }
        tb_x += 74;
    }

    // Separator
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(tb_x, toolbar_top + 3),
        sdl2::rect::Point::new(tb_x, toolbar_top + TOOLBAR_H - 4),
    );
    tb_x += 6;

    // SNAP toggle
    {
        let snap_btn = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(80030),
                x: tb_x,
                y: tb_y,
                width: 42,
                height: tb_h,
                label: "SNAP".to_string(),
                toggled: state.automation_snap_enabled,
                icon: ButtonIcon::None,
                hint: Some("Toggle snap for automation points".into()),
                ..Default::default()
            },
        );
        if snap_btn {
            state.automation_snap_enabled = !state.automation_snap_enabled;
        }
        tb_x += 46;
    }

    // Automation snap resolution dropdown
    let auto_snap_dropdown_x = tb_x;
    {
        let snap_labels: Vec<&str> = SNAP_RESOLUTIONS.iter().map(|(l, _)| *l).collect();
        let changed = dropdown(
            canvas,
            input,
            &state.theme,
            80031,
            tb_x,
            tb_y,
            52,
            tb_h,
            &snap_labels,
            &mut state.automation_snap_idx,
            &mut state.dropdown_open_id,
        );
        let _ = changed;
        tb_x += 56;
    }

    // Hint
    draw_pixel_label(
        canvas,
        &state.theme,
        "click:add  R-click:del  drag:move",
        tb_x,
        tb_y + 4,
        w - tb_x - 10,
        Theme::c(state.theme.text_dim),
    );

    // ── Zoom / scroll state ──
    let zoom = state.automation_zoom_x; // px per beat
    let scroll_x = state.automation_scroll_x; // beats

    // Coordinate helpers (pre-zoom update — for hit testing zoom gesture)
    let val_to_y =
        |val: f32| -> i32 { grid_top + grid_h - ((val.clamp(0.0, 1.0)) * grid_h as f32) as i32 };
    let y_to_val =
        |y: i32| -> f32 { (1.0 - (y - grid_top) as f32 / grid_h as f32).clamp(0.0, 1.0) };

    // x_to_beat for zoom anchor calculation (uses current state values)
    let x_to_beat_pre = |x: i32| -> f64 { scroll_x + (x - LABEL_W) as f64 / zoom };

    // ── Zoom with scroll wheel when hovering ──
    let in_grid = input.mouse_x >= LABEL_W
        && input.mouse_x < LABEL_W + grid_w
        && input.mouse_y >= grid_top
        && input.mouse_y < grid_top + grid_h;
    let in_ruler = input.mouse_x >= LABEL_W
        && input.mouse_x < LABEL_W + grid_w
        && input.mouse_y >= ruler_top
        && input.mouse_y < ruler_top + RULER_H;

    if (in_grid || in_ruler) && input.scroll_y != 0 && !input.scroll_consumed {
        if input.ctrl() {
            // Ctrl+Scroll = zoom
            let mouse_beat = x_to_beat_pre(input.mouse_x);
            let factor = if input.scroll_y > 0 { 1.15 } else { 1.0 / 1.15 };
            let new_zoom = (state.automation_zoom_x * factor).clamp(8.0, 2000.0);
            // Keep mouse position anchored
            let new_scroll = mouse_beat - (input.mouse_x - LABEL_W) as f64 / new_zoom;
            state.automation_zoom_x = new_zoom;
            state.automation_scroll_x = new_scroll.max(0.0);
        } else {
            // Scroll = pan horizontal
            let delta = -input.scroll_y as f64 * 2.0 / zoom;
            state.automation_scroll_x = (state.automation_scroll_x + delta).max(0.0);
        }
    }

    // Middle-mouse drag to pan
    if input.middle_mouse_down
        && (in_grid || in_ruler)
        && input.middle_drag_widget == WidgetId::None
    {
        input.middle_drag_widget = WidgetId::Auto(86101);
    }
    if input.middle_mouse_down && input.middle_drag_widget == WidgetId::Auto(86101) {
        let dx_beats = input.mouse_dx as f64 / zoom;
        state.automation_scroll_x = (state.automation_scroll_x - dx_beats).max(0.0);
    }

    // Re-read after potential zoom/scroll changes
    let zoom = state.automation_zoom_x;
    let scroll_x = state.automation_scroll_x;
    let beat_to_x = |beat: f64| -> i32 { LABEL_W + ((beat - scroll_x) * zoom) as i32 };
    let x_to_beat = |x: i32| -> f64 { scroll_x + (x - LABEL_W) as f64 / zoom };

    // ── Timeline ruler ──
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(28, 32, 38, 255));
    let _ = canvas.fill_rect(Rect::new(LABEL_W, ruler_top, grid_w as u32, RULER_H as u32));
    {
        let start_beat = scroll_x.floor() as i32 - 1;
        let end_beat = start_beat + (grid_w as f64 / zoom) as i32 + 4;
        for beat in start_beat..=end_beat {
            if beat < 0 {
                continue;
            }
            let bx = beat_to_x(beat as f64);
            if bx < LABEL_W || bx > LABEL_W + grid_w {
                continue;
            }
            let is_bar = beat % 4 == 0;
            if is_bar || zoom > 20.0 {
                canvas.set_draw_color(if is_bar {
                    sdl2::pixels::Color::RGBA(90, 100, 120, 200)
                } else {
                    sdl2::pixels::Color::RGBA(50, 55, 65, 150)
                });
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(bx, ruler_top),
                    sdl2::rect::Point::new(bx, ruler_top + RULER_H),
                );
                if is_bar {
                    let bar_num = beat / 4 + 1;
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &format!("{}", bar_num),
                        bx + 3,
                        ruler_top + 3,
                        24,
                        sdl2::pixels::Color::RGBA(180, 190, 210, 220),
                    );
                } else if zoom > 40.0 {
                    let sub = beat % 4 + 1;
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &format!(".{}", sub),
                        bx + 2,
                        ruler_top + 3,
                        16,
                        sdl2::pixels::Color::RGBA(100, 110, 130, 160),
                    );
                }
            }
        }
    }
    // Ruler bottom border
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(LABEL_W, ruler_top + RULER_H - 1),
        sdl2::rect::Point::new(LABEL_W + grid_w, ruler_top + RULER_H - 1),
    );

    // ── Value axis labels (left) ──
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(22, 22, 26, 255));
    let _ = canvas.fill_rect(Rect::new(0, grid_top, LABEL_W as u32, grid_h as u32));
    for i in 0..=4 {
        let val = 1.0 - i as f32 / 4.0;
        let gy = val_to_y(val);
        if gy >= grid_top && gy <= grid_top + grid_h {
            let label = format!("{:.1}", val);
            draw_pixel_label(
                canvas,
                &state.theme,
                &label,
                2,
                gy - 3,
                LABEL_W - 4,
                Theme::c(state.theme.text_dim),
            );
        }
    }

    // ── Grid background ──
    canvas.set_clip_rect(Rect::new(LABEL_W, grid_top, grid_w as u32, grid_h as u32));
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(LABEL_W, grid_top, grid_w as u32, grid_h as u32));

    // Horizontal value grid lines — fine (8ths)
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
        state.theme.grid_line[0].min(200),
        state.theme.grid_line[1].min(200),
        state.theme.grid_line[2].min(200),
        110,
    ));
    for i in 0..8 {
        let val = 1.0 - i as f32 / 8.0;
        let gy = val_to_y(val);
        if gy >= grid_top && gy <= grid_top + grid_h {
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(LABEL_W, gy),
                sdl2::rect::Point::new(LABEL_W + grid_w, gy),
            );
        }
    }
    // Horizontal value grid lines — strong (quarters)
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
        state.theme.grid_line_strong[0].min(200),
        state.theme.grid_line_strong[1].min(200),
        state.theme.grid_line_strong[2].min(200),
        200,
    ));
    for i in 0..=4 {
        let val = 1.0 - i as f32 / 4.0;
        let gy = val_to_y(val);
        if gy >= grid_top && gy <= grid_top + grid_h {
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(LABEL_W, gy),
                sdl2::rect::Point::new(LABEL_W + grid_w, gy),
            );
        }
    }

    // Vertical beat grid lines
    {
        let snap_res = SNAP_RESOLUTIONS[state.automation_snap_idx].1;
        let start_beat = (scroll_x / snap_res).floor() * snap_res;
        let end_beat = scroll_x + grid_w as f64 / zoom + 2.0;
        let mut b = start_beat;
        while b <= end_beat {
            let gx = beat_to_x(b);
            if gx >= LABEL_W && gx <= LABEL_W + grid_w {
                let is_bar = (b % 4.0).abs() < 0.001;
                canvas.set_draw_color(if is_bar {
                    Theme::c(state.theme.grid_line_strong)
                } else {
                    Theme::c(state.theme.grid_line)
                });
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(gx, grid_top),
                    sdl2::rect::Point::new(gx, grid_top + grid_h),
                );
            }
            b += snap_res;
        }
    }

    // ── Clip boundary indicators ──
    {
        let clip_start_x = beat_to_x(0.0);
        let clip_end_x = beat_to_x(clip_len);
        // Darken before clip
        if clip_start_x > LABEL_W {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 80));
            let dark_w = (clip_start_x - LABEL_W).max(0);
            let _ = canvas.fill_rect(Rect::new(LABEL_W, grid_top, dark_w as u32, grid_h as u32));
        }
        // Darken after clip
        if clip_end_x < LABEL_W + grid_w {
            let dark_x = clip_end_x.max(LABEL_W);
            let dark_w = (LABEL_W + grid_w - dark_x).max(0);
            if dark_w > 0 {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 80));
                let _ = canvas.fill_rect(Rect::new(dark_x, grid_top, dark_w as u32, grid_h as u32));
            }
        }
        // End boundary line
        if clip_end_x >= LABEL_W && clip_end_x <= LABEL_W + grid_w {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, 150));
            let _ = canvas.fill_rect(Rect::new(clip_end_x - 1, grid_top, 2, grid_h as u32));
        }
    }

    // ── Draw automation lines between points ──
    {
        let track = state.project.tracks.iter().find(|t| t.id == track_id);
        if let Some(track) = track {
            if let Some(crate::app::models::Clip::Automation(auto)) = track.clips.get(clip_idx) {
                canvas.set_draw_color(Theme::c(state.theme.clip_automation));
                for i in 1..auto.points.len() {
                    let p0 = &auto.points[i - 1];
                    let p1 = &auto.points[i];
                    let x0 = beat_to_x(p0.time);
                    let y0 = val_to_y(p0.value);
                    let x1 = beat_to_x(p1.time);
                    let y1 = val_to_y(p1.value);
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(x0, y0),
                        sdl2::rect::Point::new(x1, y1),
                    );
                }

                // Draw points
                let selected = &state.automation_selected;
                for (i, p) in auto.points.iter().enumerate() {
                    let px = beat_to_x(p.time);
                    let py = val_to_y(p.value);
                    // Check if another node is at the same pixel position (overlap)
                    let is_overlapped = auto.points.iter().enumerate().any(|(j, q)| {
                        j != i
                            && (beat_to_x(q.time) - px).abs() < 6
                            && (val_to_y(q.value) - py).abs() < 6
                    });
                    let hover = (input.mouse_x - px).abs() < 9 && (input.mouse_y - py).abs() < 9;
                    let is_sel = selected.contains(&i);
                    if is_sel {
                        // Selected: brighter, larger
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 120, 255));
                        let _ = canvas.fill_rect(Rect::new(px - 7, py - 7, 14, 14));
                        canvas.set_draw_color(Theme::c(state.theme.accent));
                        let _ = canvas.draw_rect(Rect::new(px - 7, py - 7, 14, 14));
                    } else if hover {
                        canvas.set_draw_color(Theme::c(state.theme.accent));
                        let _ = canvas.fill_rect(Rect::new(px - 7, py - 7, 14, 14));
                    } else if is_overlapped {
                        // Overlapping node: draw with a bright orange ring so both are visible
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 160, 40, 220));
                        let _ = canvas.draw_rect(Rect::new(px - 7, py - 7, 14, 14));
                        canvas.set_draw_color(Theme::c(state.theme.accent));
                        let _ = canvas.fill_rect(Rect::new(px - 5, py - 5, 10, 10));
                    } else {
                        canvas.set_draw_color(Theme::c(state.theme.accent));
                        let _ = canvas.fill_rect(Rect::new(px - 5, py - 5, 10, 10));
                    }
                }
            }
        }
    }

    // ── Draw rubberband rectangle ──
    if let Some((rb_beat, rb_val)) = state.automation_rubberband_start {
        if input.mouse_down {
            let rb_x0 = beat_to_x(rb_beat);
            let rb_y0 = val_to_y(rb_val);
            let rb_x1 = input.mouse_x;
            let rb_y1 = input.mouse_y;
            let rx = rb_x0.min(rb_x1);
            let ry = rb_y0.min(rb_y1);
            let rw = (rb_x0 - rb_x1).unsigned_abs();
            let rh = (rb_y0 - rb_y1).unsigned_abs();
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(120, 180, 255, 180));
            let _ = canvas.draw_rect(Rect::new(rx, ry, rw, rh));
        }
    }

    canvas.set_clip_rect(None);

    // ── Mouse interactions ──
    let mx = input.mouse_x;
    let my = input.mouse_y;
    if in_grid {
        if input.mouse_pressed || input.mouse_down {
            state.focused_panel = crate::app::state::FocusedPanel::AutomationEditor;
        }
        let beat = x_to_beat(mx);
        let val = y_to_val(my);

        // Right-click: delete closest point (or all selected)
        if input.right_mouse_pressed {
            if !state.automation_selected.is_empty() {
                // Delete all selected points (highest index first)
                let mut to_del = state.automation_selected.clone();
                to_del.sort_unstable();
                to_del.dedup();
                for idx in to_del.into_iter().rev() {
                    state.commands.execute(
                        Box::new(crate::app::commands::DeleteAutomationPoint {
                            track_id,
                            clip_idx,
                            point_idx: idx,
                            removed_point: None,
                        }),
                        &mut state.project,
                    );
                }
                state.automation_selected.clear();
                state.dirty = true;
            } else {
                let delete_info = {
                    let mut info = None;
                    if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                        if let Some(crate::app::models::Clip::Automation(auto)) =
                            track.clips.get(clip_idx)
                        {
                            let close_idx = auto
                                .points
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, p)| {
                                    let dx = beat_to_x(p.time) - mx;
                                    let dy = val_to_y(p.value) - my;
                                    dx * dx + dy * dy
                                })
                                .map(|(i, _)| i);
                            if let Some(idx) = close_idx {
                                let dp = beat_to_x(auto.points[idx].time) - mx;
                                let dv = val_to_y(auto.points[idx].value) - my;
                                if (dp * dp + dv * dv) < 625 {
                                    info = Some(idx);
                                }
                            }
                        }
                    }
                    info
                };
                if let Some(idx) = delete_info {
                    state.commands.execute(
                        Box::new(crate::app::commands::DeleteAutomationPoint {
                            track_id,
                            clip_idx,
                            point_idx: idx,
                            removed_point: None,
                        }),
                        &mut state.project,
                    );
                    state.dirty = true;
                }
            }
        }

        // Right-click drag: erase all points near the cursor path
        if input.right_mouse_down && !input.right_mouse_pressed {
            let erase_radius_sq = 16i32 * 16; // 16px erase radius
            let to_delete: Vec<usize> = {
                let mut indices = Vec::new();
                if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                    if let Some(crate::app::models::Clip::Automation(auto)) =
                        track.clips.get(clip_idx)
                    {
                        for (i, p) in auto.points.iter().enumerate() {
                            let dx = beat_to_x(p.time) - mx;
                            let dy = val_to_y(p.value) - my;
                            if dx * dx + dy * dy < erase_radius_sq {
                                indices.push(i);
                            }
                        }
                    }
                }
                indices
            };
            // Delete from highest index to lowest to preserve indices
            for idx in to_delete.into_iter().rev() {
                state.commands.execute(
                    Box::new(crate::app::commands::DeleteAutomationPoint {
                        track_id,
                        clip_idx,
                        point_idx: idx,
                        removed_point: None,
                    }),
                    &mut state.project,
                );
                state.dirty = true;
            }
        }

        // Left-click
        if input.mouse_pressed && !input.right_mouse_pressed {
            // Find closest existing point
            let close_idx = {
                let mut found = None;
                if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                    if let Some(crate::app::models::Clip::Automation(auto)) =
                        track.clips.get(clip_idx)
                    {
                        found = auto
                            .points
                            .iter()
                            .enumerate()
                            .filter_map(|(i, p)| {
                                let dx = beat_to_x(p.time) - mx;
                                let dy = val_to_y(p.value) - my;
                                let d2 = dx * dx + dy * dy;
                                if d2 < 625 {
                                    Some((i, d2))
                                } else {
                                    None
                                }
                            })
                            .min_by_key(|&(_, d2)| d2)
                            .map(|(i, _)| i);
                    }
                }
                found
            };

            if input.ctrl() {
                // Ctrl+click: toggle selection on a point, or start rubberband
                if let Some(ci) = close_idx {
                    if state.automation_selected.contains(&ci) {
                        state.automation_selected.retain(|&x| x != ci);
                    } else {
                        state.automation_selected.push(ci);
                    }
                } else {
                    // Start rubberband selection
                    state.automation_rubberband_start = Some((beat, val));
                }
            } else if let Some(ci) = close_idx {
                // Click on point without Ctrl
                if state.automation_selected.contains(&ci) {
                    // Dragging a selected point → group move
                    // Store original positions of all selected points
                    state.automation_group_drag_orig.clear();
                    if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                        if let Some(crate::app::models::Clip::Automation(auto)) =
                            track.clips.get(clip_idx)
                        {
                            for &si in &state.automation_selected {
                                if si < auto.points.len() {
                                    state
                                        .automation_group_drag_orig
                                        .push((auto.points[si].time, auto.points[si].value));
                                }
                            }
                        }
                    }
                    state.automation_drag_idx = Some(ci);
                    state.automation_drag_orig = None; // using group drag instead
                    input.drag_start_x = mx;
                    input.drag_start_y = my;
                } else {
                    // Click on unselected point: clear selection, start single drag
                    state.automation_selected.clear();
                    state.automation_drag_idx = Some(ci);
                    if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                        if let Some(crate::app::models::Clip::Automation(auto)) =
                            track.clips.get(clip_idx)
                        {
                            if ci < auto.points.len() {
                                state.automation_drag_orig =
                                    Some((auto.points[ci].time, auto.points[ci].value));
                            }
                        }
                    }
                }
            } else if beat >= 0.0 {
                // Click on empty space:
                // If points are selected, first click just deselects (like piano roll).
                // Only add a new point when nothing is selected.
                if !state.automation_selected.is_empty() {
                    state.automation_selected.clear();
                } else {
                    let snapped_beat = if state.automation_snap_enabled {
                        let r = SNAP_RESOLUTIONS[state.automation_snap_idx].1;
                        (beat.max(0.0) / r).round() * r
                    } else {
                        beat.max(0.0)
                    };
                    state.commands.execute(
                        Box::new(crate::app::commands::AddAutomationPoint {
                            track_id,
                            clip_idx,
                            point: crate::app::models::AutomationPoint {
                                time: snapped_beat,
                                value: val,
                            },
                            inserted_idx: 0,
                        }),
                        &mut state.project,
                    );
                    state.dirty = true;
                }
            }
        }
    }

    // Rubberband drag continuation
    if let Some((rb_beat, rb_val)) = state.automation_rubberband_start {
        if !input.mouse_down {
            // Release: select all points inside the rectangle
            let cur_beat = x_to_beat(input.mouse_x);
            let cur_val = y_to_val(input.mouse_y);
            let b_lo = rb_beat.min(cur_beat);
            let b_hi = rb_beat.max(cur_beat);
            let v_lo = rb_val.min(cur_val);
            let v_hi = rb_val.max(cur_val);
            state.automation_selected.clear();
            if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                if let Some(crate::app::models::Clip::Automation(auto)) = track.clips.get(clip_idx)
                {
                    for (i, p) in auto.points.iter().enumerate() {
                        if p.time >= b_lo && p.time <= b_hi && p.value >= v_lo && p.value <= v_hi {
                            state.automation_selected.push(i);
                        }
                    }
                }
            }
            state.automation_rubberband_start = None;
        }
    }

    // Group drag continuation — works even when mouse moves outside grid area
    if input.mouse_down
        && !input.mouse_pressed
        && state.automation_drag_idx.is_some()
        && !state.automation_group_drag_orig.is_empty()
    {
        // Group move: compute delta from drag start and apply to all selected
        let dx_beats = x_to_beat(input.mouse_x) - x_to_beat(input.drag_start_x);
        let dy_val = y_to_val(input.mouse_y) - y_to_val(input.drag_start_y);
        if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
            if let Some(crate::app::models::Clip::Automation(auto)) = track.clips.get_mut(clip_idx)
            {
                for (oi, &si) in state.automation_selected.iter().enumerate() {
                    if si < auto.points.len() && oi < state.automation_group_drag_orig.len() {
                        let (orig_t, orig_v) = state.automation_group_drag_orig[oi];
                        let new_t = if state.automation_snap_enabled {
                            let r = SNAP_RESOLUTIONS[state.automation_snap_idx].1;
                            ((orig_t + dx_beats).max(0.0) / r).round() * r
                        } else {
                            (orig_t + dx_beats).max(0.0)
                        };
                        auto.points[si].time = new_t;
                        auto.points[si].value = (orig_v + dy_val).clamp(0.0, 1.0);
                    }
                }
                state.dirty = true;
            }
        }
    }
    // Single drag continuation
    else if input.mouse_down && !input.mouse_pressed {
        if let Some(drag_idx) = state.automation_drag_idx {
            if state.automation_group_drag_orig.is_empty() {
                let beat = x_to_beat(input.mouse_x);
                let val = y_to_val(input.mouse_y);
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::app::models::Clip::Automation(auto)) =
                        track.clips.get_mut(clip_idx)
                    {
                        if drag_idx < auto.points.len() {
                            let snapped_beat = if state.automation_snap_enabled {
                                let r = SNAP_RESOLUTIONS[state.automation_snap_idx].1;
                                (beat.max(0.0) / r).round() * r
                            } else {
                                beat.max(0.0)
                            };

                            // Shift-held: clamp time to the range between neighbour nodes
                            // so this node can't cross over adjacent nodes
                            let clamped_beat = if input.shift() {
                                let prev_t = if drag_idx > 0 {
                                    auto.points[drag_idx - 1].time + 1e-6
                                } else {
                                    0.0
                                };
                                let next_t = if drag_idx + 1 < auto.points.len() {
                                    auto.points[drag_idx + 1].time - 1e-6
                                } else {
                                    f64::MAX
                                };
                                snapped_beat.clamp(prev_t, next_t)
                            } else {
                                snapped_beat
                            };

                            auto.points[drag_idx].time = clamped_beat;
                            auto.points[drag_idx].value = val.clamp(0.0, 1.0);
                            state.dirty = true;
                        }
                    }
                }
            }
        }
    }

    if !input.mouse_down {
        if let Some(drag_idx) = state.automation_drag_idx {
            if !state.automation_group_drag_orig.is_empty() {
                // Group drag release: sort points, clear group drag state
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::app::models::Clip::Automation(auto)) =
                        track.clips.get_mut(clip_idx)
                    {
                        auto.points
                            .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
                    }
                }
                state.automation_group_drag_orig.clear();
                // Update selected indices after sort
                state.automation_selected.clear();
            } else {
                // Commit single MoveAutomationPoint command on release
                if let Some((old_time, old_value)) = state.automation_drag_orig.take() {
                    if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                        if let Some(crate::app::models::Clip::Automation(auto)) =
                            track.clips.get(clip_idx)
                        {
                            if drag_idx < auto.points.len() {
                                let new_time = auto.points[drag_idx].time;
                                let new_value = auto.points[drag_idx].value;
                                // Only commit if actually moved
                                if (new_time - old_time).abs() > 1e-9
                                    || (new_value - old_value).abs() > 1e-6
                                {
                                    // Undo back to original first (command will re-apply)
                                    if let Some(track_m) =
                                        state.project.tracks.iter_mut().find(|t| t.id == track_id)
                                    {
                                        if let Some(crate::app::models::Clip::Automation(auto_m)) =
                                            track_m.clips.get_mut(clip_idx)
                                        {
                                            if drag_idx < auto_m.points.len() {
                                                auto_m.points[drag_idx].time = old_time;
                                                auto_m.points[drag_idx].value = old_value;
                                                auto_m.points.sort_by(|a, b| {
                                                    a.time.partial_cmp(&b.time).unwrap()
                                                });
                                            }
                                        }
                                    }
                                    // Find the point index after restoring
                                    let point_idx = {
                                        let mut idx = drag_idx;
                                        if let Some(track) =
                                            state.project.tracks.iter().find(|t| t.id == track_id)
                                        {
                                            if let Some(crate::app::models::Clip::Automation(
                                                auto,
                                            )) = track.clips.get(clip_idx)
                                            {
                                                if let Some(i) = auto.points.iter().position(|p| {
                                                    (p.time - old_time).abs() < 1e-9
                                                        && (p.value - old_value).abs() < 1e-6
                                                }) {
                                                    idx = i;
                                                }
                                            }
                                        }
                                        idx
                                    };
                                    state.commands.execute(
                                        Box::new(crate::app::commands::MoveAutomationPoint {
                                            track_id,
                                            clip_idx,
                                            point_idx,
                                            old_time,
                                            old_value,
                                            new_time,
                                            new_value,
                                        }),
                                        &mut state.project,
                                    );
                                    state.dirty = true;
                                }
                            }
                        }
                    }
                } else {
                    // Sort on release even if no orig stored (fallback)
                    if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id)
                    {
                        if let Some(crate::app::models::Clip::Automation(auto)) =
                            track.clips.get_mut(clip_idx)
                        {
                            auto.points
                                .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
                        }
                    }
                }
            }
        }
        state.automation_drag_idx = None;
    }

    // ── Playhead ──
    let pos = state.project.transport.position;
    let rel_beat = pos - clip_start;
    if rel_beat >= 0.0 && rel_beat <= clip_len {
        let cx = beat_to_x(rel_beat);
        if cx >= LABEL_W && cx <= LABEL_W + grid_w {
            canvas.set_draw_color(Theme::c(state.theme.playhead));
            let _ = canvas.fill_rect(Rect::new(cx, ruler_top, 1, (grid_h + RULER_H) as u32));
        }
    }

    // ── Horizontal scrollbar / scroomer ──────────────────────────────
    {
        // Account for automation points beyond clip end
        let max_point_beat = {
            let mut max_b = 0.0_f64;
            if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                if let Some(crate::app::models::Clip::Automation(auto)) = track.clips.get(clip_idx)
                {
                    for p in &auto.points {
                        if p.time > max_b {
                            max_b = p.time;
                        }
                    }
                }
            }
            max_b
        };
        let total_beats = clip_len.max(max_point_beat).max(16.0) + 4.0;
        let visible_beats = grid_w as f64 / zoom;
        let thumb_ratio = (visible_beats / total_beats).clamp(0.02, 1.0) as f32;
        let max_scroll_beats = (total_beats - visible_beats).max(0.0);
        let scroll_frac = if max_scroll_beats > 0.0 {
            (scroll_x / max_scroll_beats).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let (new_frac, new_ratio) = scrollbar_with_squeeze(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(80040),
            WidgetId::Auto(80041),
            WidgetId::Auto(80042),
            LABEL_W,
            grid_top + grid_h,
            grid_w,
            SCROLL_T,
            ScrollbarDir::Horizontal,
            scroll_frac,
            thumb_ratio,
        );
        let ratio_changed = (new_ratio - thumb_ratio).abs() > 0.001;
        let frac_changed = (new_frac - scroll_frac).abs() > 0.001;
        if ratio_changed {
            let new_visible_beats = (new_ratio as f64 * total_beats).max(1.0);
            let new_zoom = (grid_w as f64 / new_visible_beats).clamp(8.0, 2000.0);
            state.automation_zoom_x = new_zoom;
        }
        if ratio_changed || frac_changed {
            let cur_zoom = state.automation_zoom_x;
            let new_max_scroll = (total_beats - grid_w as f64 / cur_zoom).max(0.0);
            state.automation_scroll_x = (new_frac as f64 * new_max_scroll).max(0.0);
        }
    }

    // ── Borders ──
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(LABEL_W - 1, grid_top),
        sdl2::rect::Point::new(LABEL_W - 1, grid_top + grid_h),
    );
    let _ = canvas.draw_rect(Rect::new(LABEL_W, grid_top, grid_w as u32, grid_h as u32));

    // ── Dropdown popup overlay (draw on top of everything) ───────────
    {
        let snap_labels: Vec<&str> = SNAP_RESOLUTIONS.iter().map(|(l, _)| *l).collect();
        dropdown_popup_overlay(
            canvas,
            &state.theme,
            80031,
            auto_snap_dropdown_x,
            tb_y,
            52,
            tb_h,
            52,
            &snap_labels,
            state.automation_snap_idx,
            state.dropdown_open_id,
            input.mouse_x,
            input.mouse_y,
        );
    }
}
