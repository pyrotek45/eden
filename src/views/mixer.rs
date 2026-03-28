// Eden DAW — Views: mixer

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::{vol_gain_to_pos, vol_pos_to_gain};
use crate::app::input::{InputState, WidgetId};
use crate::app::models::create_rack_slot_for_module;
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

pub(crate) fn meter_color(db: f64) -> sdl2::pixels::Color {
    if db >= 0.0 {
        sdl2::pixels::Color::RGBA(220, 40, 30, 240)
    } else {
        let frac = ((db + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
        if frac < 0.5 {
            let t = frac / 0.5;
            sdl2::pixels::Color::RGBA(
                (20.0 + t * 40.0) as u8,
                (90.0 + t * 130.0) as u8,
                (40.0 + t * 40.0) as u8,
                230,
            )
        } else if frac < 0.8 {
            let t = (frac - 0.5) / 0.3;
            sdl2::pixels::Color::RGBA(
                (60.0 + t * 180.0) as u8,
                (220.0 - t * 30.0) as u8,
                (80.0 - t * 50.0) as u8,
                230,
            )
        } else {
            let t = (frac - 0.8) / 0.2;
            sdl2::pixels::Color::RGBA((240.0 - t * 20.0) as u8, (190.0 - t * 90.0) as u8, 30, 230)
        }
    }
}

// ── Helper: draw one vertical meter bar (module-level) ──
fn draw_meter_bar(canvas: &mut Canvas<Window>, rms: f32, x: i32, y: i32, w_px: u32, h_px: i32) {
    const DB_FLOOR: f64 = -60.0;
    const DB_CEIL: f64 = 12.0;
    const DB_RANGE: f64 = DB_CEIL - DB_FLOOR;

    canvas.set_draw_color(sdl2::pixels::Color::RGBA(16, 16, 20, 220));
    let _ = canvas.fill_rect(Rect::new(x, y, w_px, h_px as u32));
    if rms > 0.001 {
        let db = if rms > 1e-6 {
            20.0 * (rms as f64).log10()
        } else {
            DB_FLOOR
        };
        let frac = ((db - DB_FLOOR) / DB_RANGE).clamp(0.0, 1.0) as f32;
        let fill = (frac * h_px as f32) as i32;
        let seg_h = 2i32;
        let gap = 1i32;
        let mut py = y + h_px - fill;
        while py < y + h_px {
            let seg_bottom = (py + seg_h).min(y + h_px);
            let seg_top_db = DB_FLOOR + DB_RANGE * (1.0 - (py - y) as f64 / h_px as f64);
            let c = meter_color(seg_top_db);
            canvas.set_draw_color(c);
            let _ = canvas.fill_rect(Rect::new(x, py, w_px, (seg_bottom - py) as u32));
            py = seg_bottom + gap;
        }
    }
    let zero_frac = ((0.0 - DB_FLOOR) / DB_RANGE) as f32;
    let zero_y = y + h_px - (zero_frac * h_px as f32) as i32;
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 50, 40, 160));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(x, zero_y),
        sdl2::rect::Point::new(x + w_px as i32 - 1, zero_y),
    );
    {
        let mut db_tick = -55.0_f64;
        while db_tick <= DB_CEIL {
            if (db_tick - 0.0).abs() < 0.1 {
                db_tick += 5.0;
                continue;
            }
            let frac = ((db_tick - DB_FLOOR) / DB_RANGE) as f32;
            let ty = y + h_px - (frac * h_px as f32) as i32;
            if ty > y && ty < y + h_px {
                let alpha = if db_tick % 10.0 == 0.0 { 70u8 } else { 40u8 };
                let tick_w = if db_tick % 10.0 == 0.0 {
                    w_px
                } else {
                    (w_px / 2).max(1)
                };
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(180, 190, 200, alpha));
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(x, ty),
                    sdl2::rect::Point::new(x + tick_w as i32 - 1, ty),
                );
            }
            db_tick += 5.0;
        }
    }
}

/// Draw dB labels to the right of a stereo meter pair.
/// `label_x` is the x position where labels start (right edge of right meter + gap).
fn draw_meter_db_labels(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    label_x: i32,
    y: i32,
    h_px: i32,
    max_label_w: i32,
) {
    const DB_FLOOR: f64 = -60.0;
    const DB_CEIL: f64 = 12.0;
    const DB_RANGE: f64 = DB_CEIL - DB_FLOOR;
    let label_scale = 1i32; // tiny pixel font
    let gh = 5 * label_scale; // glyph height at scale=1
                              // Draw labels at 0, -10, -20, -30, -40, -50, +10
    for &db in &[0.0_f64, -10.0, -20.0, -30.0, -40.0, -50.0, 10.0] {
        let frac = ((db - DB_FLOOR) / DB_RANGE) as f32;
        let ty = y + h_px - (frac * h_px as f32) as i32;
        if ty > y + gh / 2 && ty < y + h_px - gh / 2 {
            let label = if db == 0.0 {
                " 0".to_string()
            } else {
                format!("{}", db as i32)
            };
            let lx = label_x;
            let ly = ty - gh / 2; // vertically center on the tick
            let col = if db >= 0.0 {
                sdl2::pixels::Color::RGBA(220, 90, 70, 180)
            } else {
                sdl2::pixels::Color::RGBA(120, 130, 140, 140)
            };
            draw_pixel_label_scaled(canvas, theme, &label, lx, ly, max_label_w, col, label_scale);
        }
    }
}

/// Draw a peak hold line on a meter bar.
fn draw_peak_hold(canvas: &mut Canvas<Window>, ph: f32, x: i32, y: i32, w_px: u32, h_px: i32) {
    const DB_FLOOR: f64 = -60.0;
    const DB_CEIL: f64 = 12.0;
    const DB_RANGE: f64 = DB_CEIL - DB_FLOOR;
    if ph > 0.01 {
        let pk_db = if ph > 1e-6 {
            20.0 * (ph as f64).log10()
        } else {
            DB_FLOOR
        };
        let pk_frac = ((pk_db - DB_FLOOR) / DB_RANGE).clamp(0.0, 1.0) as f32;
        let ph_y = y + h_px - (pk_frac * h_px as f32) as i32;
        let ph_col = if pk_db >= 0.0 {
            sdl2::pixels::Color::RGBA(200, 40, 30, 255)
        } else if pk_db >= -6.0 {
            sdl2::pixels::Color::RGBA(220, 180, 40, 240)
        } else {
            sdl2::pixels::Color::RGBA(100, 220, 120, 200)
        };
        canvas.set_draw_color(ph_col);
        let _ = canvas.fill_rect(Rect::new(x, ph_y, w_px, 2));
    }
}

pub(super) fn draw_bottom_mixer(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    if h < 40 {
        return;
    }

    let strip_w_full = 260i32;
    let strip_w_slim = 80i32;
    let strip_gap = 6i32;
    let track_count = state.project.tracks.len();
    let scrollbar_h = 14i32;
    let master_strip_w = 220i32;
    let preview_strip_w = 60i32;
    let content_w = w - master_strip_w - preview_strip_w - 12;

    let non_auto_count = state
        .project
        .tracks
        .iter()
        .filter(|t| t.track_type != crate::app::models::TrackType::Automation)
        .count() as i32;
    let _ = non_auto_count; // used only for legacy uniform-width fallback
                            // Compute total width accounting for slim tracks
    let total_content_w = {
        let mut tw = 12i32;
        for t in &state.project.tracks {
            if t.track_type == crate::app::models::TrackType::Automation {
                continue;
            }
            let sw = if state.mixer_slim_tracks.contains(&t.id) {
                strip_w_slim
            } else {
                strip_w_full
            };
            tw += sw + strip_gap;
        }
        tw
    };
    let needs_scroll = total_content_w > content_w;
    let max_scroll = if needs_scroll {
        (total_content_w - content_w).max(0) as f32
    } else {
        0.0
    };
    let scroll_offset = state.bottom_mixer_scroll_x as i32;
    let clip_h = if needs_scroll { h - scrollbar_h } else { h };
    canvas.set_clip_rect(Rect::new(0, top, content_w as u32, clip_h as u32));

    // ── Per-track strips ──
    let mut _strip_idx = 0;
    let mut strip_x_accum = 8i32; // accumulated x position for variable-width strips
    for i in 0..track_count {
        if state.project.tracks[i].track_type == crate::app::models::TrackType::Automation {
            continue;
        }

        let is_slim = state
            .mixer_slim_tracks
            .contains(&state.project.tracks[i].id);
        let strip_w = if is_slim { strip_w_slim } else { strip_w_full };
        let x = strip_x_accum - scroll_offset;
        let sy = top + 4;
        let sh = (clip_h - 8).max(10);
        let track_id = state.project.tracks[i].id;
        let track_color = state.project.tracks[i].color;
        let selected = state.selected_tracks.contains(&track_id);

        // Strip background
        let bg = if selected {
            sdl2::pixels::Color::RGBA(
                state.theme.panel_bg[0].saturating_add(14),
                state.theme.panel_bg[1].saturating_add(14),
                state.theme.panel_bg[2].saturating_add(18),
                255,
            )
        } else {
            Theme::c(state.theme.panel_bg)
        };
        canvas.set_draw_color(bg);
        let _ = canvas.fill_rect(Rect::new(x, sy, strip_w as u32, sh as u32));

        // Color cap
        let cap_h = if selected { 4u32 } else { 3u32 };
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            track_color[0],
            track_color[1],
            track_color[2],
            if selected { 255 } else { 200 },
        ));
        let _ = canvas.fill_rect(Rect::new(x, sy, strip_w as u32, cap_h));

        // Track name
        let name_y = sy + cap_h as i32 + 2;
        let name_col = if selected {
            sdl2::pixels::Color::RGBA(255, 255, 255, 255)
        } else {
            sdl2::pixels::Color::RGBA(190, 195, 200, 220)
        };
        // ── Expand / Slim toggle button (top-right corner of strip) ──
        let expand_btn_sz = 14i32;
        let expand_btn_x = x + strip_w - expand_btn_sz - 4;
        let expand_btn_y = sy + cap_h as i32 + 1;
        let expand_btn_id = input.next_id();
        let expand_label = if is_slim { "+" } else { "-" };
        let expand_hint = if is_slim {
            "Expand strip"
        } else {
            "Collapse to slim"
        };
        let expand_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: expand_btn_id,
                x: expand_btn_x,
                y: expand_btn_y,
                width: expand_btn_sz,
                height: expand_btn_sz,
                label: expand_label.into(),
                toggled: is_slim,
                icon: ButtonIcon::None,
                hint: Some(expand_hint.into()),
                ..Default::default()
            },
        );
        if expand_clicked {
            if is_slim {
                state.mixer_slim_tracks.remove(&track_id);
            } else {
                state.mixer_slim_tracks.insert(track_id);
            }
        }

        draw_pixel_label(
            canvas,
            &state.theme,
            &state.project.tracks[i].name.clone(),
            x + 6,
            name_y,
            strip_w - expand_btn_sz - 12,
            name_col,
        );

        // ── Click to select strip ──
        let name_zone_h = 16i32;
        let strip_hover = input.mouse_in_rect(x, sy, strip_w, name_zone_h);
        if strip_hover && input.mouse_pressed && !input.consumed {
            if input.ctrl() {
                if state.selected_tracks.contains(&track_id) {
                    state.selected_tracks.remove(&track_id);
                } else {
                    state.selected_tracks.insert(track_id);
                }
            } else if input.shift() {
                let track_ids: Vec<u32> = state
                    .project
                    .tracks
                    .iter()
                    .filter(|t| t.track_type != crate::app::models::TrackType::Automation)
                    .map(|t| t.id)
                    .collect();
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
                state.selected_tracks.clear();
                state.selected_tracks.insert(track_id);
            }
            state.selected_track = Some(track_id);
            input.consume();
        }

        // ── Layout zones (top to bottom within strip) ──
        let content_top = name_y + 18; // more padding below name
        let bottom_bar_h = 40i32; // mute/solo/pan row (taller for label)
        let content_bottom = sy + sh - bottom_bar_h;
        let avail_h = (content_bottom - content_top).max(10);

        // ── VU meter gauge (top of content area, full width) — skip in slim mode ──
        let vu_h = if !is_slim { 73i32.min(avail_h / 3) } else { 0 };
        if vu_h >= 30 && !is_slim {
            let vu_pos = state.meters.vu_needle.get(i).copied().unwrap_or(0.0);
            let vu_peak = state.meters.vu_peak_needle.get(i).copied().unwrap_or(0.0);
            vu_meter(
                canvas,
                &state.theme,
                x + 6,
                content_top,
                strip_w - 12,
                vu_h,
                vu_pos,
                vu_peak,
            );
        }

        let below_vu = content_top + if vu_h >= 30 { vu_h + 8 } else { 0 }; // more padding after VU
        let below_vu_avail = (content_bottom - below_vu).max(10);

        // ── Left column: Fader + Stereo meters ──
        let fader_x = x + 8;
        let fader_w = 18i32;
        let meter_bar_w = 5u32;
        let meter_gap = 2i32;
        let left_col_w = fader_w + 4 + (meter_bar_w as i32) * 2 + meter_gap;
        let fader_h = (below_vu_avail - 8).max(20); // leave gap at bottom

        // Fader groove
        {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(12, 12, 16, 255));
            let _ = canvas.fill_rect(Rect::new(fader_x + 6, below_vu, 6, fader_h as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 50, 200));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(fader_x + 12, below_vu),
                sdl2::rect::Point::new(fader_x + 12, below_vu + fader_h),
            );
            // dB tick marks
            for &db_mark in &[-48.0_f32, -24.0, -12.0, -6.0, 0.0, 6.0] {
                let gain = if db_mark <= -60.0 {
                    0.0
                } else {
                    10.0_f32.powf(db_mark / 20.0)
                };
                let pos = vol_gain_to_pos(gain);
                let tick_y = below_vu + fader_h - (pos * fader_h as f32) as i32;
                let tc = if db_mark >= 0.0 {
                    sdl2::pixels::Color::RGBA(200, 80, 60, 140)
                } else {
                    sdl2::pixels::Color::RGBA(70, 75, 85, 120)
                };
                canvas.set_draw_color(tc);
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(fader_x, tick_y),
                    sdl2::rect::Point::new(fader_x + 4, tick_y),
                );
            }
        }

        // Volume slider
        let mut vol_pos = vol_gain_to_pos(state.project.tracks[i].volume);
        let mixer_vol_id = input.next_id();
        let vol_changed = slider(
            canvas,
            input,
            &state.theme,
            &SliderParams {
                id: mixer_vol_id,
                x: fader_x,
                y: below_vu,
                width: fader_w,
                height: fader_h,
                min: 0.0,
                max: 1.0,
                orientation: SliderOrientation::Vertical,
                label: None,
                default_value: Some(vol_gain_to_pos(1.0)),
            },
            &mut vol_pos,
        );
        if vol_changed {
            let new_vol = vol_pos_to_gain(vol_pos);
            let old_vol = state.project.tracks[i].volume;
            let delta = new_vol - old_vol;
            state.project.tracks[i].volume = new_vol;
            // Propagate volume delta to all other selected tracks
            if state.selected_tracks.contains(&track_id) {
                for j in 0..state.project.tracks.len() {
                    if j == i {
                        continue;
                    }
                    let other_id = state.project.tracks[j].id;
                    if state.selected_tracks.contains(&other_id) {
                        state.project.tracks[j].volume =
                            (state.project.tracks[j].volume + delta).clamp(0.0, 2.0);
                    }
                }
            }
        }
        if input.mouse_released && input.drag_widget == mixer_vol_id {
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

        // Stereo L/R meters (right of fader)
        let meter_x = fader_x + fader_w + 4;
        let rms_l = state.meters.track_rms_l.get(i).copied().unwrap_or(0.0);
        let rms_r = state.meters.track_rms_r.get(i).copied().unwrap_or(0.0);
        draw_meter_bar(canvas, rms_l, meter_x, below_vu, meter_bar_w, fader_h);
        draw_meter_bar(
            canvas,
            rms_r,
            meter_x + meter_bar_w as i32 + meter_gap,
            below_vu,
            meter_bar_w,
            fader_h,
        );

        // Peak hold lines
        let ph_l = state
            .meters
            .track_peak_hold_l
            .get(i)
            .copied()
            .unwrap_or(0.0);
        let ph_r = state
            .meters
            .track_peak_hold_r
            .get(i)
            .copied()
            .unwrap_or(0.0);
        draw_peak_hold(canvas, ph_l, meter_x, below_vu, meter_bar_w, fader_h);
        draw_peak_hold(
            canvas,
            ph_r,
            meter_x + meter_bar_w as i32 + meter_gap,
            below_vu,
            meter_bar_w,
            fader_h,
        );

        // dB labels to the right of the meters
        // dB labels — skip in slim mode
        if !is_slim {
            let labels_x = meter_x + meter_bar_w as i32 * 2 + meter_gap + 2;
            let label_max_w = strip_w - (labels_x - x) - 2;
            if label_max_w > 8 {
                draw_meter_db_labels(
                    canvas,
                    &state.theme,
                    labels_x,
                    below_vu,
                    fader_h,
                    label_max_w,
                );
            }
        }

        // Clip LEDs above meters
        for (ch, mx_off) in [(0i32, 0i32), (1i32, meter_bar_w as i32 + meter_gap)] {
            let clip_flag = if ch == 0 {
                state
                    .meters
                    .track_clipping_l
                    .get(i)
                    .copied()
                    .unwrap_or(false)
            } else {
                state
                    .meters
                    .track_clipping_r
                    .get(i)
                    .copied()
                    .unwrap_or(false)
            };
            let led_x = meter_x + mx_off;
            let led_y = below_vu - 6;
            if clip_flag {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 20, 20, 255));
                let _ = canvas.fill_rect(Rect::new(led_x, led_y, meter_bar_w, 4));
                if input.mouse_in_rect(led_x, led_y, meter_bar_w as i32, 4) && input.mouse_pressed {
                    if ch == 0 {
                        state.meters.track_clipping_l[i] = false;
                    } else {
                        state.meters.track_clipping_r[i] = false;
                    }
                }
            } else {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 50, 30, 160));
                let _ = canvas.fill_rect(Rect::new(led_x, led_y, meter_bar_w, 4));
            }
        }

        // L/R labels
        draw_pixel_label_scaled(
            canvas,
            &state.theme,
            "L",
            meter_x,
            below_vu + fader_h + 1,
            meter_bar_w as i32,
            sdl2::pixels::Color::RGBA(100, 110, 120, 140),
            1,
        );
        draw_pixel_label_scaled(
            canvas,
            &state.theme,
            "R",
            meter_x + meter_bar_w as i32 + meter_gap,
            below_vu + fader_h + 1,
            meter_bar_w as i32,
            sdl2::pixels::Color::RGBA(100, 110, 120, 140),
            1,
        );

        // ── Right column: CStrip2 knobs + EQ + Comp curve (skip in slim mode) ──
        let right_x = x + 8 + left_col_w + 30;
        let right_w = strip_w - (right_x - x) - 6;
        if !is_slim && right_w > 40 && below_vu_avail > 60 {
            let cs_descs = crate::modules::get_param_descs("CStrip2");
            if state.project.tracks[i].cstrip2_params.is_empty() && !cs_descs.is_empty() {
                state.project.tracks[i].cstrip2_params = cs_descs
                    .iter()
                    .map(|d| (d.id.to_string(), d.default))
                    .collect();
            }

            // ── CStrip2 bypass is now in the bottom bar (right of Solo) ──

            let knob_r = 13i32;
            let cell_w = (right_w / 2).max(30);
            let cell_h = 42i32;
            let knob_base_y = below_vu + 4;

            for (pi, desc) in cs_descs.iter().enumerate() {
                let col = (pi / 5) as i32;
                let row = (pi % 5) as i32;
                let kx = right_x + cell_w / 2 + col * cell_w;
                let ky = knob_base_y + 12 + row * cell_h;
                if ky + knob_r + 8 > content_bottom {
                    break;
                }

                let cur_val = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == desc.id)
                    .map(|(_, v)| *v)
                    .unwrap_or(desc.default);
                let mut val = cur_val;
                let cs_knob_id = input.next_id();
                let is_bipolar = desc.id == "output"
                    || desc.id == "treble"
                    || desc.id == "mid"
                    || desc.id == "bass";
                let changed = knob(
                    canvas,
                    input,
                    &state.theme,
                    &KnobParams {
                        id: cs_knob_id,
                        x: kx,
                        y: ky,
                        radius: knob_r,
                        min: desc.min,
                        max: desc.max,
                        sensitivity: 0.005,
                        label: None,
                        bipolar: is_bipolar,
                        default_value: Some(desc.default),
                        hint: Some(desc.name.into()),
                        snap_points: vec![],
                    },
                    &mut val,
                );
                // Label above knob
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    desc.name,
                    kx - cell_w / 2,
                    ky - knob_r - 9,
                    cell_w,
                    sdl2::pixels::Color::RGBA(130, 135, 145, 170),
                );
                if changed {
                    // Snapshot before first CStrip2 knob change in this drag
                    if state.cstrip2_knob_snapshot.is_none() {
                        state.cstrip2_knob_snapshot = Some(state.project.clone());
                    }
                    let delta = val - cur_val;
                    if let Some(entry) = state.project.tracks[i]
                        .cstrip2_params
                        .iter_mut()
                        .find(|(id, _)| id == desc.id)
                    {
                        entry.1 = val;
                    }
                    // Propagate to all other selected tracks
                    if state.selected_tracks.contains(&track_id) {
                        for j in 0..state.project.tracks.len() {
                            if j == i {
                                continue;
                            }
                            let other_id = state.project.tracks[j].id;
                            if state.selected_tracks.contains(&other_id) {
                                if let Some(entry) = state.project.tracks[j]
                                    .cstrip2_params
                                    .iter_mut()
                                    .find(|(id, _)| id == desc.id)
                                {
                                    entry.1 = (entry.1 + delta).clamp(desc.min, desc.max);
                                }
                            }
                        }
                    }
                    state.dirty = true;
                }
            }

            // Commit CStrip2 knob snapshot on mouse release
            if input.mouse_released {
                if let Some(snapshot) = state.cstrip2_knob_snapshot.take() {
                    state
                        .commands
                        .push_undo_snapshot(snapshot, "Adjust CStrip2");
                }
            }

            // ── EQ curve visual (below knobs) ──
            let eq_y = knob_base_y + 12 + 5 * cell_h + 4;
            let eq_h = 50i32;
            let eq_w = right_w.min(cell_w * 2);
            if eq_y + eq_h < content_bottom && eq_w > 20 {
                // Background
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 20, 26, 200));
                let _ = canvas.fill_rect(Rect::new(right_x, eq_y, eq_w as u32, eq_h as u32));
                // 0 dB center line
                let mid_line = eq_y + eq_h / 2;
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 100));
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(right_x, mid_line),
                    sdl2::rect::Point::new(right_x + eq_w, mid_line),
                );

                // Read all relevant params
                let treble = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == "treble")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.5);
                let mid = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == "mid")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.5);
                let bass = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == "bass")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.5);
                let treb_frq = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == "treb_frq")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.55);
                let bass_frq = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == "bass_frq")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.15);
                let lo_cap = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == "lo_cap")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let hi_cap = state.project.tracks[i]
                    .cstrip2_params
                    .iter()
                    .find(|(id, _)| id == "hi_cap")
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);

                // Compute frequency response at each pixel column
                // X axis: log frequency from 20 Hz to 20 kHz
                // Y axis: dB, scaled to fit eq_h (±12 dB range)
                let db_range = 14.0_f64; // ±7 dB visible range (enough for ±3.5 dB EQ + HP/LP)
                let log_min = (20.0_f64).ln();
                let log_max = (20000.0_f64).ln();
                let sr = 44100.0_f64;

                // Filter coefficients (matching DSP)
                let hp_coef = if lo_cap > 0.0 {
                    (lo_cap as f64).powf(2.0) * 0.4995 + 0.0001
                } else {
                    0.0
                };
                let lp_coef = if hi_cap > 0.0 {
                    (hi_cap as f64).powf(2.0) * 0.4995 + 0.0001
                } else {
                    0.0
                };
                let bass_coef = (bass_frq as f64) * (bass_frq as f64) * 0.499 + 0.001;
                let treb_coef = treb_frq as f64 * treb_frq as f64 * 0.499 + 0.001;
                let bass_g = ((bass as f64) * 2.0 - 1.0) * 0.5 + 1.0;
                let mid_g = ((mid as f64) * 2.0 - 1.0) * 0.5 + 1.0;
                let treb_g = ((treble as f64) * 2.0 - 1.0) * 0.5 + 1.0;

                // Compute EQ response once per pixel column, then draw fill + curve in two passes.

                // Helper: evaluate EQ magnitude (dB) at a normalised angular frequency omega.
                let eval_eq_db = |omega: f64| -> f64 {
                    let lp_complex = |c: f64| -> (f64, f64) {
                        if c < 1e-8 {
                            return (1.0, 0.0);
                        }
                        let omc = 1.0 - c;
                        let dr = 1.0 - omc * omega.cos();
                        let di = omc * omega.sin();
                        let ds = dr * dr + di * di;
                        if ds < 1e-15 {
                            (1.0, 0.0)
                        } else {
                            (c * dr / ds, c * di / ds)
                        }
                    };

                    let hp_mag = if hp_coef > 1e-8 {
                        let omc = 1.0 - hp_coef;
                        let cos_w = omega.cos();
                        let sin_w = omega.sin();
                        let den_sq = 1.0 - 2.0 * omc * cos_w + omc * omc;
                        if den_sq < 1e-15 {
                            1.0
                        } else {
                            let re_lp = hp_coef * (1.0 - omc * cos_w) / den_sq;
                            let im_lp = hp_coef * omc * sin_w / den_sq;
                            let re_hp = 1.0 - re_lp;
                            let im_hp = -im_lp;
                            (re_hp * re_hp + im_hp * im_hp).sqrt()
                        }
                    } else {
                        1.0
                    };

                    let lp_mag = if lp_coef > 1e-8 {
                        let omc = 1.0 - lp_coef;
                        let num = lp_coef * lp_coef;
                        let den = 1.0 - 2.0 * omc * omega.cos() + omc * omc;
                        if den > 1e-12 {
                            (num / den).sqrt()
                        } else {
                            1.0
                        }
                    } else {
                        1.0
                    };

                    let (lbr, lbi) = lp_complex(bass_coef);
                    let (ltr, lti) = lp_complex(treb_coef);
                    let eq_re = lbr * (bass_g - mid_g) + ltr * (mid_g - treb_g) + treb_g;
                    let eq_im = lbi * (bass_g - mid_g) + lti * (mid_g - treb_g);
                    let eq_mag = (eq_re * eq_re + eq_im * eq_im).sqrt();

                    let total = hp_mag * lp_mag * eq_mag;
                    if total > 1e-10 {
                        20.0 * total.log10()
                    } else {
                        -db_range
                    }
                };

                // Build pixel array: one Y value per screen pixel column (no gaps).
                // We iterate over integer pixel columns [0 .. eq_w) directly so each
                // column is covered exactly once — no aliasing / missing columns.
                let mut curve_py: Vec<i32> = Vec::with_capacity(eq_w as usize);
                for col in 0..eq_w {
                    let t = col as f64 / (eq_w - 1).max(1) as f64;
                    let freq = (log_min + t * (log_max - log_min)).exp();
                    let omega = 2.0 * std::f64::consts::PI * freq / sr;
                    let db = eval_eq_db(omega).clamp(-db_range, db_range);
                    let py = mid_line - (db / db_range * (eq_h as f64 / 2.0)) as i32;
                    // Clamp to widget bounds so fill rects never escape.
                    curve_py.push(py.clamp(eq_y, eq_y + eq_h - 1));
                }

                // Pass 1: filled area between curve and centre line — use fill_rect so
                // every pixel column is fully covered with no gaps or bleed.
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 200, 140, 30));
                for col in 0..eq_w {
                    let px = right_x + col;
                    let py = curve_py[col as usize];
                    let (ya, yb) = if py <= mid_line {
                        (py, mid_line)
                    } else {
                        (mid_line, py)
                    };
                    let fill_h = (yb - ya + 1).max(1) as u32;
                    let _ = canvas.fill_rect(Rect::new(px, ya, 1, fill_h));
                }

                // Pass 2: curve line — connect adjacent columns with draw_line so
                // steep slopes have no holes.
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 200, 140, 200));
                for col in 1..eq_w {
                    let ppx = right_x + col - 1;
                    let ppy = curve_py[(col - 1) as usize];
                    let px = right_x + col;
                    let py = curve_py[col as usize];
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(ppx, ppy),
                        sdl2::rect::Point::new(px, py),
                    );
                }

                // Label and border
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    "EQ",
                    right_x + 2,
                    eq_y + 1,
                    16,
                    sdl2::pixels::Color::RGBA(80, 200, 140, 110),
                );
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
                let _ = canvas.draw_rect(Rect::new(right_x, eq_y, eq_w as u32, eq_h as u32));

                // ── Compressor curve + GR bar (actual gain reduction from audio) ──
                let comp_y = eq_y + eq_h + 4;
                let comp_h = 50i32;
                if comp_y + comp_h < content_bottom && eq_w > 20 {
                    let compress = state.project.tracks[i]
                        .cstrip2_params
                        .iter()
                        .find(|(id, _)| id == "compress")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    // Get actual gain reduction from metering (sum of all effect slot GR for this track)
                    let gr_db = state
                        .meters
                        .track_effect_gr
                        .get(i)
                        .map(|slots| slots.iter().sum::<f32>())
                        .unwrap_or(0.0);
                    let track_rms = state.meters.track_rms.get(i).copied().unwrap_or(0.0);
                    comp_curve_widget(
                        canvas,
                        &state.theme,
                        right_x,
                        comp_y,
                        eq_w,
                        comp_h,
                        compress,
                        gr_db,
                        track_rms,
                    );
                }
            }
        }

        // ── Bottom bar: Pan + Mute/Solo ──
        let bottom_y = sy + sh - bottom_bar_h;

        // Pan knob with label
        if bottom_bar_h >= 28 {
            let pan_knob_x = if is_slim { x + strip_w / 2 } else { x + 20 };
            let pan_knob_y = bottom_y + 14;
            let mut pan_val = state.project.tracks[i].pan;
            let mixer_pan_id = input.next_id();
            let pan_changed = knob(
                canvas,
                input,
                &state.theme,
                &KnobParams {
                    id: mixer_pan_id,
                    x: pan_knob_x,
                    y: pan_knob_y,
                    radius: 10,
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
                let old_pan = state.project.tracks[i].pan;
                let delta = pan_val - old_pan;
                state.project.tracks[i].pan = pan_val;
                // Propagate pan delta to all other selected tracks
                if state.selected_tracks.contains(&track_id) {
                    for j in 0..state.project.tracks.len() {
                        if j == i {
                            continue;
                        }
                        let other_id = state.project.tracks[j].id;
                        if state.selected_tracks.contains(&other_id) {
                            state.project.tracks[j].pan =
                                (state.project.tracks[j].pan + delta).clamp(-1.0, 1.0);
                        }
                    }
                }
            }
            if input.mouse_released && input.drag_widget == mixer_pan_id {
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

        // Mute / Solo
        {
            let mute_on = state.project.tracks[i].mute;
            let solo_on = state.project.tracks[i].solo;
            let (mute_x, solo_x, btn_y, btn_sz) = if is_slim {
                // In slim mode: M, S, B side by side, centered, below pan
                let bsz = 14i32;
                let total = bsz * 3 + 4 * 2; // 3 buttons + 2 gaps
                let start_x = x + (strip_w - total) / 2;
                (start_x, start_x + bsz + 4, bottom_y + 26, bsz)
            } else {
                (x + 42, x + 64, bottom_y + 10, 18i32)
            };
            let mix_mute_id = input.next_id();
            let mute_clicked = toggle_button(
                canvas,
                input,
                &state.theme,
                mute_x,
                btn_y,
                btn_sz,
                state.theme.mute_on,
                mute_on,
                mix_mute_id,
                "M",
                Some("Mute track"),
            );
            if mute_clicked {
                let new_mute = !mute_on;
                state.commands.execute(
                    Box::new(crate::app::commands::SetTrackMute {
                        track_id,
                        new_value: new_mute,
                        old_value: mute_on,
                    }),
                    &mut state.project,
                );
                // Propagate to all other selected tracks
                if state.selected_tracks.contains(&track_id) {
                    for j in 0..state.project.tracks.len() {
                        if j == i {
                            continue;
                        }
                        let other_id = state.project.tracks[j].id;
                        if state.selected_tracks.contains(&other_id) {
                            state.project.tracks[j].mute = new_mute;
                        }
                    }
                }
            }
            let mix_solo_id = input.next_id();
            let solo_clicked = toggle_button(
                canvas,
                input,
                &state.theme,
                solo_x,
                btn_y,
                btn_sz,
                state.theme.solo_on,
                solo_on,
                mix_solo_id,
                "S",
                Some("Solo track"),
            );
            if solo_clicked {
                if input.ctrl() {
                    let snapshot = state.project.clone();
                    for t in &mut state.project.tracks {
                        t.solo = false;
                    }
                    state.commands.push_undo_snapshot(snapshot, "Unsolo All");
                    state.dirty = true;
                } else {
                    let new_solo = !solo_on;
                    state.commands.execute(
                        Box::new(crate::app::commands::SetTrackSolo {
                            track_id,
                            new_value: new_solo,
                            old_value: solo_on,
                        }),
                        &mut state.project,
                    );
                    // Propagate to all other selected tracks
                    if state.selected_tracks.contains(&track_id) {
                        for j in 0..state.project.tracks.len() {
                            if j == i {
                                continue;
                            }
                            let other_id = state.project.tracks[j].id;
                            if state.selected_tracks.contains(&other_id) {
                                state.project.tracks[j].solo = new_solo;
                            }
                        }
                    }
                }
            }

            // ── CStrip2 bypass toggle (right of Solo) ──
            let byp_x = solo_x + btn_sz + 4;
            let bypass_on = state.project.tracks[i].cstrip2_bypass;
            let byp_btn_id = input.next_id();
            let byp_label = if bypass_on { "B" } else { "C" };
            let byp_hint = if bypass_on {
                "Channel strip bypassed — click to enable"
            } else {
                "Channel strip active — click to bypass"
            };
            let byp_color = if bypass_on {
                state.theme.mute_on // yellow-ish to indicate bypassed
            } else {
                [60, 65, 75, 200]
            };
            let byp_clicked = toggle_button(
                canvas,
                input,
                &state.theme,
                byp_x,
                btn_y,
                btn_sz,
                byp_color,
                bypass_on,
                byp_btn_id,
                byp_label,
                Some(byp_hint),
            );
            if byp_clicked {
                let snapshot = state.project.clone();
                state.project.tracks[i].cstrip2_bypass = !bypass_on;
                state
                    .commands
                    .push_undo_snapshot(snapshot, "Toggle CStrip2 Bypass");
                state.dirty = true;
            }
        }

        // Strip border
        let border_col = if selected {
            Theme::c(state.theme.accent)
        } else {
            sdl2::pixels::Color::RGBA(
                state.theme.panel_border[0],
                state.theme.panel_border[1],
                state.theme.panel_border[2],
                state.theme.panel_border[3],
            )
        };
        canvas.set_draw_color(border_col);
        let _ = canvas.draw_rect(Rect::new(x, sy, strip_w as u32, sh as u32));

        strip_x_accum += strip_w + strip_gap;
        _strip_idx += 1;
    }

    canvas.set_clip_rect(None);

    // ── Scrollbar ──
    if needs_scroll {
        let sb_y = top + h - scrollbar_h;
        let frac = if max_scroll > 0.0 {
            state.bottom_mixer_scroll_x / max_scroll
        } else {
            0.0
        };
        let visible_frac = (content_w as f32 / total_content_w as f32).clamp(0.05, 1.0);
        let new_frac = scrollbar(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(84200),
            0,
            sb_y,
            content_w,
            scrollbar_h,
            ScrollbarDir::Horizontal,
            frac,
            visible_frac,
        );
        state.bottom_mixer_scroll_x = new_frac * max_scroll;
    } else {
        state.bottom_mixer_scroll_x = 0.0;
    }

    // Scroll wheel
    if input.mouse_y >= top
        && input.mouse_y < top + h
        && input.mouse_x < content_w
        && input.scroll_y != 0
        && needs_scroll
        && !input.scroll_consumed
    {
        state.bottom_mixer_scroll_x = (state.bottom_mixer_scroll_x - input.scroll_y as f32 * 30.0)
            .max(0.0)
            .min(max_scroll);
    }

    // Middle-click drag
    if needs_scroll {
        let bottom_mixer_drag_id = WidgetId::Auto(87004);
        if input.middle_mouse_down
            && input.mouse_y >= top
            && input.mouse_y < top + h
            && input.mouse_x < content_w
            && input.middle_drag_widget == WidgetId::None
        {
            input.middle_drag_widget = bottom_mixer_drag_id;
        }
        if input.middle_mouse_down && input.middle_drag_widget == bottom_mixer_drag_id {
            state.bottom_mixer_scroll_x =
                (state.bottom_mixer_scroll_x - input.mouse_dx as f32).clamp(0.0, max_scroll);
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // ── PREVIEW STRIP (between track list and master bus) ──
    // ══════════════════════════════════════════════════════════════════
    {
        let px = w - master_strip_w - preview_strip_w - 4;
        let py = top + 4;
        let ph = (h - 8).max(4);

        // Background
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 28, 255));
        let _ = canvas.fill_rect(Rect::new(px, py, preview_strip_w as u32, ph as u32));

        // Accent cap
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(180, 140, 220, 220));
        let _ = canvas.fill_rect(Rect::new(px, py, preview_strip_w as u32, 4));

        // Label
        draw_pixel_label(
            canvas,
            &state.theme,
            "PRV",
            px + 4,
            py + 7,
            preview_strip_w - 8,
            sdl2::pixels::Color::RGBA(180, 160, 220, 240),
        );

        // ── Volume fader (left side of strip) ──
        let prv_content_y = py + 20;
        let prv_bottom_pad = 8i32;
        let prv_fader_h = (ph - 20 - prv_bottom_pad).max(20);
        let prv_fader_x = px + 4;
        let prv_fader_w = 14i32;
        {
            // Fader groove
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(12, 12, 16, 255));
            let _ = canvas.fill_rect(Rect::new(
                prv_fader_x + 4,
                prv_content_y,
                6,
                prv_fader_h as u32,
            ));

            let mut prv_vol_pos = vol_gain_to_pos(state.preview_volume);
            let prv_vol_id = input.next_id();
            let prv_vol_changed = slider(
                canvas,
                input,
                &state.theme,
                &SliderParams {
                    id: prv_vol_id,
                    x: prv_fader_x,
                    y: prv_content_y,
                    width: prv_fader_w,
                    height: prv_fader_h,
                    min: 0.0,
                    max: 1.0,
                    orientation: SliderOrientation::Vertical,
                    label: None,
                    default_value: Some(vol_gain_to_pos(1.0)),
                },
                &mut prv_vol_pos,
            );
            if prv_vol_changed {
                state.preview_volume = vol_pos_to_gain(prv_vol_pos);
            }
        }

        // Stereo meters (right of fader)
        let prv_meter_y = prv_content_y;
        let prv_meter_h = prv_fader_h;
        let bar_w = 8u32;
        let bar_gap = 2i32;
        let meter_x = prv_fader_x + prv_fader_w + 4;

        let prv_rms_l = state.meters.preview_rms_l;
        let prv_rms_r = state.meters.preview_rms_r;
        draw_meter_bar(canvas, prv_rms_l, meter_x, prv_meter_y, bar_w, prv_meter_h);
        draw_meter_bar(
            canvas,
            prv_rms_r,
            meter_x + bar_w as i32 + bar_gap,
            prv_meter_y,
            bar_w,
            prv_meter_h,
        );

        // Border
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 55, 80, 180));
        let _ = canvas.draw_rect(Rect::new(px, py, preview_strip_w as u32, ph as u32));
    }

    // ══════════════════════════════════════════════════════════════════
    // ── MASTER OUTPUT STRIP (right side) ──
    // ══════════════════════════════════════════════════════════════════
    let mx = w - master_strip_w;
    let my = top + 4;
    let mh = (h - 8).max(4);

    // Background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(24, 24, 32, 255));
    let _ = canvas.fill_rect(Rect::new(mx, my, master_strip_w as u32, mh as u32));
    // Accent cap
    canvas.set_draw_color(Theme::c(state.theme.accent));
    let _ = canvas.fill_rect(Rect::new(mx, my, master_strip_w as u32, 4));
    draw_pixel_label(
        canvas,
        &state.theme,
        "MASTER",
        mx + 8,
        my + 7,
        master_strip_w - 16,
        sdl2::pixels::Color::RGBA(180, 200, 255, 240),
    );

    // ── Master VU meter (below label, full width) ──
    let m_vu_h = 76i32;
    let m_vu_y = my + 18;
    vu_meter(
        canvas,
        &state.theme,
        mx + 6,
        m_vu_y,
        master_strip_w - 12,
        m_vu_h,
        state.meters.master_vu_needle,
        state.meters.master_vu_peak_needle,
    );

    // ── Master volume fader ──
    let m_fader_top = m_vu_y + m_vu_h + 12;
    let m_bottom_bar = 14i32;
    let m_fader_h = (mh - (m_fader_top - my) - m_bottom_bar - 4).max(20);
    let m_fader_x = mx + 10;
    let m_fader_w = 20i32;

    // Fader groove
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(12, 12, 16, 255));
    let _ = canvas.fill_rect(Rect::new(m_fader_x + 7, m_fader_top, 6, m_fader_h as u32));
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 50, 200));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(m_fader_x + 13, m_fader_top),
        sdl2::rect::Point::new(m_fader_x + 13, m_fader_top + m_fader_h),
    );
    // dB ticks
    for &db_mark in &[-48.0_f32, -36.0, -24.0, -12.0, -6.0, 0.0, 6.0] {
        let gain = if db_mark <= -60.0 {
            0.0
        } else {
            10.0_f32.powf(db_mark / 20.0)
        };
        let pos = vol_gain_to_pos(gain);
        let tick_y = m_fader_top + m_fader_h - (pos * m_fader_h as f32) as i32;
        let tc = if db_mark >= 0.0 {
            sdl2::pixels::Color::RGBA(200, 80, 60, 140)
        } else {
            sdl2::pixels::Color::RGBA(70, 75, 85, 120)
        };
        canvas.set_draw_color(tc);
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(m_fader_x, tick_y),
            sdl2::rect::Point::new(m_fader_x + 4, tick_y),
        );
    }

    {
        let mut mvol_pos = vol_gain_to_pos(state.master_volume_ui);
        let mvol_id = input.next_id();
        let mvol_changed = slider(
            canvas,
            input,
            &state.theme,
            &SliderParams {
                id: mvol_id,
                x: m_fader_x,
                y: m_fader_top,
                width: m_fader_w,
                height: m_fader_h,
                min: 0.0,
                max: 1.0,
                orientation: SliderOrientation::Vertical,
                label: None,
                default_value: Some(vol_gain_to_pos(1.0)),
            },
            &mut mvol_pos,
        );
        if mvol_changed {
            state.master_volume_ui = vol_pos_to_gain(mvol_pos);
        }
    }

    // ── Master stereo meters (Pre = pre-volume, Out = post-everything) ──
    let m_meter_x = mx + 38;
    let m_meter_bar_w = 7u32;
    let m_meter_gap = 2i32;
    let m_pair_gap = 4i32; // gap between Pre and Out pairs

    // Pre-effects pair
    let m_rms_l = state.meters.master_rms_l;
    let m_rms_r = state.meters.master_rms_r;
    draw_meter_bar(
        canvas,
        m_rms_l,
        m_meter_x,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );
    draw_meter_bar(
        canvas,
        m_rms_r,
        m_meter_x + m_meter_bar_w as i32 + m_meter_gap,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );

    // Out (post-everything) pair — uses smoothed peak so the bar matches the limiter ceiling
    let m_out_x = m_meter_x + (m_meter_bar_w as i32) * 2 + m_meter_gap + m_pair_gap;
    let m_out_l = state.meters.master_peak_smooth_post_l;
    let m_out_r = state.meters.master_peak_smooth_post_r;
    draw_meter_bar(
        canvas,
        m_out_l,
        m_out_x,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );
    draw_meter_bar(
        canvas,
        m_out_r,
        m_out_x + m_meter_bar_w as i32 + m_meter_gap,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );

    // Master peak hold (pre pair)
    draw_peak_hold(
        canvas,
        state.meters.master_peak_hold_l,
        m_meter_x,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );
    draw_peak_hold(
        canvas,
        state.meters.master_peak_hold_r,
        m_meter_x + m_meter_bar_w as i32 + m_meter_gap,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );

    // Master peak hold (out pair)
    draw_peak_hold(
        canvas,
        state.meters.master_peak_hold_post_l,
        m_out_x,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );
    draw_peak_hold(
        canvas,
        state.meters.master_peak_hold_post_r,
        m_out_x + m_meter_bar_w as i32 + m_meter_gap,
        m_fader_top,
        m_meter_bar_w,
        m_fader_h,
    );

    // dB labels to the right of out meters
    {
        let m_labels_x = m_out_x + m_meter_bar_w as i32 * 2 + m_meter_gap + 2;
        let m_label_max_w = mx + master_strip_w - m_labels_x - 4;
        if m_label_max_w > 8 {
            draw_meter_db_labels(
                canvas,
                &state.theme,
                m_labels_x,
                m_fader_top,
                m_fader_h,
                m_label_max_w,
            );
        }
    }

    // Master clip LEDs (on out pair — final output clipping)
    for (ch, (flag, mx_off)) in [
        (state.meters.master_clipping_l, (m_out_x - m_meter_x)),
        (
            state.meters.master_clipping_r,
            (m_out_x - m_meter_x) + m_meter_bar_w as i32 + m_meter_gap,
        ),
    ]
    .iter()
    .enumerate()
    {
        let led_x = m_meter_x + mx_off;
        let led_y = m_fader_top - 7;
        if *flag {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 20, 20, 255));
            let _ = canvas.fill_rect(Rect::new(led_x, led_y, m_meter_bar_w, 4));
            if input.mouse_in_rect(led_x, led_y, m_meter_bar_w as i32, 4) && input.mouse_pressed {
                if ch == 0 {
                    state.meters.master_clipping_l = false;
                } else {
                    state.meters.master_clipping_r = false;
                }
            }
        } else {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 50, 30, 160));
            let _ = canvas.fill_rect(Rect::new(led_x, led_y, m_meter_bar_w, 4));
        }
    }

    // Pre / Out labels above meter pairs
    draw_pixel_label(
        canvas,
        &state.theme,
        "Pre",
        m_meter_x,
        m_fader_top + m_fader_h + 1,
        m_meter_bar_w as i32 * 2 + m_meter_gap,
        sdl2::pixels::Color::RGBA(100, 110, 130, 150),
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        "Out",
        m_out_x,
        m_fader_top + m_fader_h + 1,
        m_meter_bar_w as i32 * 2 + m_meter_gap,
        sdl2::pixels::Color::RGBA(140, 180, 120, 180),
    );

    // ── Master info column (right of meters) ──
    let info_x = m_out_x + (m_meter_bar_w as i32) * 2 + m_meter_gap + 10 + 10;
    let info_w = master_strip_w - (info_x - mx) - 8;
    if info_w > 20 {
        let peak = state.meters.master_peak_l.max(state.meters.master_peak_r);
        let rms = state.meters.master_rms;

        let peak_db_str = if peak > 1e-6 {
            format!("Peak {:.1}dB", 20.0 * peak.log10())
        } else {
            "Peak -∞".to_string()
        };
        draw_pixel_label(
            canvas,
            &state.theme,
            &peak_db_str,
            info_x,
            m_fader_top + 2,
            info_w,
            sdl2::pixels::Color::RGBA(180, 190, 200, 220),
        );

        let rms_db_str = if rms > 1e-6 {
            format!("RMS {:.1}dB", 20.0 * rms.log10())
        } else {
            "RMS -∞".to_string()
        };
        draw_pixel_label(
            canvas,
            &state.theme,
            &rms_db_str,
            info_x,
            m_fader_top + 16,
            info_w,
            sdl2::pixels::Color::RGBA(160, 170, 180, 200),
        );

        // Balance
        let bal = if (m_rms_l + m_rms_r) > 1e-6 {
            (m_rms_r - m_rms_l) / (m_rms_l + m_rms_r)
        } else {
            0.0
        };
        let bal_str = if bal.abs() < 0.05 {
            "Bal: C".to_string()
        } else if bal > 0.0 {
            format!("Bal: R{:.0}%", bal * 100.0)
        } else {
            format!("Bal: L{:.0}%", -bal * 100.0)
        };
        draw_pixel_label(
            canvas,
            &state.theme,
            &bal_str,
            info_x,
            m_fader_top + 30,
            info_w,
            sdl2::pixels::Color::RGBA(140, 150, 160, 170),
        );

        // Crest
        if peak > 1e-6 && rms > 1e-6 {
            let crest = 20.0 * (peak / rms).log10();
            draw_pixel_label(
                canvas,
                &state.theme,
                &format!("Crest {:.1}dB", crest),
                info_x,
                m_fader_top + 44,
                info_w,
                sdl2::pixels::Color::RGBA(140, 150, 160, 170),
            );
        }

        // ── Mini oscilloscope ──
        let osc_y = m_fader_top + 62;
        let osc_h = (m_fader_h - 70).clamp(0, 60);
        let osc_w = info_w;
        if osc_h > 16 && osc_w > 20 {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(14, 16, 22, 240));
            let _ = canvas.fill_rect(Rect::new(info_x, osc_y, osc_w as u32, osc_h as u32));
            let osc_mid = osc_y + osc_h / 2;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 100));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(info_x, osc_mid),
                sdl2::rect::Point::new(info_x + osc_w, osc_mid),
            );
            let osc_data = &state.meters.oscilloscope;
            if !osc_data.is_empty() {
                let step = (osc_data.len() as f32 / osc_w as f32).max(1.0);
                let mut prev_y = osc_mid;
                for px in 0..osc_w {
                    let idx = (px as f32 * step) as usize;
                    let s = osc_data.get(idx).copied().unwrap_or(0.0);
                    let sy_sample = (osc_mid - (s * osc_h as f32 * 0.45) as i32)
                        .clamp(osc_y, osc_y + osc_h - 1);
                    if px > 0 {
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 220, 160, 200));
                        let _ = canvas.draw_line(
                            sdl2::rect::Point::new(info_x + px - 1, prev_y),
                            sdl2::rect::Point::new(info_x + px, sy_sample),
                        );
                    }
                    prev_y = sy_sample;
                }
            }
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
            let _ = canvas.draw_rect(Rect::new(info_x, osc_y, osc_w as u32, osc_h as u32));
        }

        // ── Mastering section (below oscilloscope) ──
        let mas_y = osc_y + osc_h + 6;
        let mas_bot = my + mh - 16; // leave room for border + bottom label
        let mas_avail = (mas_bot - mas_y).max(0);
        let mas_x = info_x;
        let mas_w = info_w;
        if mas_avail >= 30 && mas_w > 20 {
            let mut cy = mas_y;

            // ── LUFS meters ──
            let lufs_m = state.meters.master_lufs_momentary;
            let lufs_st = state.meters.master_lufs_short;
            // Momentary LUFS label
            let lufs_m_str = if lufs_m < -60.0 || lufs_m == 0.0 {
                "M: -∞ LUFS".to_string()
            } else {
                format!("M: {:.1} LUFS", lufs_m)
            };
            let lufs_col = if lufs_m > -6.0 {
                sdl2::pixels::Color::RGBA(230, 80, 60, 220)
            } else if lufs_m > -14.0 {
                sdl2::pixels::Color::RGBA(220, 190, 60, 200)
            } else {
                sdl2::pixels::Color::RGBA(120, 200, 150, 190)
            };
            draw_pixel_label(
                canvas,
                &state.theme,
                &lufs_m_str,
                mas_x,
                cy,
                mas_w,
                lufs_col,
            );
            cy += 11;
            let lufs_st_str = if lufs_st < -60.0 || lufs_st == 0.0 {
                "S: -∞ LUFS".to_string()
            } else {
                format!("S: {:.1} LUFS", lufs_st)
            };
            draw_pixel_label(
                canvas,
                &state.theme,
                &lufs_st_str,
                mas_x,
                cy,
                mas_w,
                sdl2::pixels::Color::RGBA(100, 170, 130, 170),
            );
            cy += 13;

            // ── Stereo correlation bar ──
            // -1 = fully out of phase (mono danger), +1 = perfectly in phase
            let corr = state.meters.master_correlation.clamp(-1.0, 1.0);
            if cy + 13 < mas_bot {
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    "Phase",
                    mas_x,
                    cy,
                    mas_w,
                    sdl2::pixels::Color::RGBA(110, 120, 140, 160),
                );
                cy += 9;
                // Background track
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(22, 24, 30, 220));
                let _ = canvas.fill_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                // Centre tick (0 = mono)
                let mid_x = mas_x + mas_w / 2;
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(70, 75, 85, 180));
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(mid_x, cy),
                    sdl2::rect::Point::new(mid_x, cy + 5),
                );
                // Fill: map corr -1..+1 to bar width
                let fill_frac = (corr + 1.0) / 2.0; // 0..1
                let fill_w = (fill_frac * mas_w as f32) as i32;
                let bar_col = if corr < 0.0 {
                    sdl2::pixels::Color::RGBA(200, 70, 50, 200)
                } else if corr < 0.5 {
                    sdl2::pixels::Color::RGBA(200, 180, 50, 200)
                } else {
                    sdl2::pixels::Color::RGBA(60, 190, 110, 200)
                };
                canvas.set_draw_color(bar_col);
                let _ = canvas.fill_rect(Rect::new(mas_x, cy, fill_w.max(1) as u32, 6));
                // Border
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
                let _ = canvas.draw_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                // Numeric label
                let corr_str = format!("{:+.2}", corr);
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &corr_str,
                    mas_x + mas_w - 26,
                    cy - 1,
                    26,
                    sdl2::pixels::Color::RGBA(130, 140, 155, 160),
                );
                cy += 10;
            }

            // ── Dynamic range (crest factor) bar ──
            if cy + 16 < mas_bot {
                let peak = state.meters.master_peak_l.max(state.meters.master_peak_r);
                let rms2 = state.meters.master_rms;
                let dr_db = if peak > 1e-6 && rms2 > 1e-6 {
                    (20.0 * (peak / rms2).log10()).clamp(0.0, 30.0)
                } else {
                    0.0_f32
                };
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    "DR",
                    mas_x,
                    cy,
                    mas_w,
                    sdl2::pixels::Color::RGBA(110, 120, 140, 160),
                );
                cy += 9;
                // Background
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(22, 24, 30, 220));
                let _ = canvas.fill_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                // Fill: 0–30 dB range
                let dr_frac = (dr_db / 30.0).clamp(0.0, 1.0);
                let dr_fill = (dr_frac * mas_w as f32) as i32;
                let dr_col = if dr_db < 6.0 {
                    // Heavy limiting / brickwall
                    sdl2::pixels::Color::RGBA(200, 60, 50, 200)
                } else if dr_db < 12.0 {
                    sdl2::pixels::Color::RGBA(200, 175, 50, 200)
                } else {
                    sdl2::pixels::Color::RGBA(60, 180, 100, 200)
                };
                canvas.set_draw_color(dr_col);
                let _ = canvas.fill_rect(Rect::new(mas_x, cy, dr_fill.max(1) as u32, 6));
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
                let _ = canvas.draw_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                let dr_str = format!("{:.0}dB", dr_db);
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &dr_str,
                    mas_x + mas_w - 22,
                    cy - 1,
                    22,
                    sdl2::pixels::Color::RGBA(130, 140, 155, 160),
                );
                cy += 10;
            }

            // ── True peak indicator ──
            if cy + 10 < mas_bot {
                let tp = state.meters.master_peak_l.max(state.meters.master_peak_r);
                let tp_db = if tp > 1e-6 {
                    20.0 * tp.log10()
                } else {
                    -60.0_f32
                };
                let tp_col = if tp_db > -0.1 {
                    sdl2::pixels::Color::RGBA(235, 55, 40, 240)
                } else if tp_db > -3.0 {
                    sdl2::pixels::Color::RGBA(220, 185, 50, 210)
                } else {
                    sdl2::pixels::Color::RGBA(100, 170, 130, 180)
                };
                let tp_str = if tp > 1e-6 {
                    format!("TP: {:.1}dBFS", tp_db)
                } else {
                    "TP: -∞".to_string()
                };
                draw_pixel_label(canvas, &state.theme, &tp_str, mas_x, cy, mas_w, tp_col);
            }
        }
    }

    // Master border
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(70, 80, 120, 200));
    let _ = canvas.draw_rect(Rect::new(mx, my, master_strip_w as u32, mh as u32));
}

pub(super) fn draw_instrument_rack(
    canvas: &mut Canvas<Window>,
    input: &mut crate::app::input::InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    let sel_track_idx = state
        .selected_track
        .and_then(|tid| state.project.tracks.iter().position(|t| t.id == tid));

    let (rack_label, track_type) = if let Some(ti) = sel_track_idx {
        let t = &state.project.tracks[ti];
        let chain_label = match t.track_type {
            crate::app::models::TrackType::Midi => "MIDI -> Instrument -> FX -> Out",
            crate::app::models::TrackType::Audio => "Audio -> FX -> Out",
            crate::app::models::TrackType::Automation => "Automation (no rack)",
        };
        (
            format!("RACK - {} [{}]", t.name, chain_label),
            Some(t.track_type),
        )
    } else {
        ("RACK - NO TRACK SELECTED".to_string(), None)
    };

    draw_pixel_label(
        canvas,
        &state.theme,
        &rack_label,
        8,
        top + 6,
        w - 16,
        Theme::c(state.theme.text_secondary),
    );

    // Automation tracks have no rack
    if track_type == Some(crate::app::models::TrackType::Automation) || sel_track_idx.is_none() {
        draw_pixel_label(
            canvas,
            &state.theme,
            "Select a MIDI or Audio track",
            8,
            top + 30,
            200,
            Theme::c(state.theme.text_dim),
        );
        return;
    }

    let ti = sel_track_idx.unwrap();
    let slot_count = state.project.tracks[ti].rack.len();

    // Draw each rack slot as a module in the chain
    let scrollbar_h = 18i32; // chunky scrollbar at bottom
                             // Modules maintain a comfortable minimum height and overflow/clip gracefully
                             // rather than squishing when the panel is short
    let natural_h = (h - 40 - scrollbar_h).max(60);
    let slot_h = natural_h.max(300); // tall enough for 4 rows of knobs (need ~266px)
    let slot_gap = 8i32;
    let scroll_offset = state.rack_scroll_x as i32;
    let mut sx = 10i32 - scroll_offset;

    // Clip drawing to rack content area (above scrollbar)
    canvas.set_clip_rect(Rect::new(0, top + 20, w as u32, (slot_h + 24) as u32));

    for slot_idx in 0..slot_count {
        // Draw signal flow arrow between slots
        if slot_idx > 0 {
            let arrow_x = sx - slot_gap / 2;
            let arrow_y = top + 24 + slot_h / 2;
            canvas.set_draw_color(Theme::c(state.theme.accent));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(arrow_x - 4, arrow_y),
                sdl2::rect::Point::new(arrow_x + 4, arrow_y),
            );
            // Arrowhead
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(arrow_x + 2, arrow_y - 3),
                sdl2::rect::Point::new(arrow_x + 4, arrow_y),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(arrow_x + 2, arrow_y + 3),
                sdl2::rect::Point::new(arrow_x + 4, arrow_y),
            );
        }

        let sy = top + 24;

        // Dynamic slot width based on param count
        let plugin_name_ref = &state.project.tracks[ti].rack[slot_idx].plugin_name;
        let _param_descs = crate::modules::get_param_descs(plugin_name_ref);
        let param_count = state.project.tracks[ti].rack[slot_idx].params.len();
        // Calculate columns so that we have max 4 rows
        // cols = ceil(param_count / 4)
        let _is_inst = crate::modules::is_instrument(plugin_name_ref);
        let is_sampler = plugin_name_ref == "Sampler";
        let max_rows = 4usize;
        let cols = if param_count == 0 {
            2
        } else {
            param_count.div_ceil(max_rows)
        }
        .max(2);
        let knob_cell_w = 80i32;
        let knob_cols_w = cols as i32 * knob_cell_w + 20;

        // Decide if we show an effect visual panel on the right side.
        // Only for specific effect modules that have useful graphs.
        let has_vis_panel = crate::modules::has_vis_panel(plugin_name_ref.as_str());
        let vis_col_w = if has_vis_panel { 120i32 } else { 0i32 };

        let base_w = knob_cols_w + vis_col_w;
        let slot_w = if is_sampler {
            base_w.max(340)
        } else {
            base_w.max(160)
        };

        // Slot background
        let slot_enabled = state.project.tracks[ti].rack[slot_idx].enabled;
        let bg = if slot_enabled {
            Theme::c(state.theme.panel_bg)
        } else {
            sdl2::pixels::Color::RGBA(
                state.theme.panel_bg[0].saturating_sub(20),
                state.theme.panel_bg[1].saturating_sub(20),
                state.theme.panel_bg[2].saturating_sub(20),
                200,
            )
        };
        canvas.set_draw_color(bg);
        let _ = canvas.fill_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));

        // Slot header (plugin name)
        let plugin_name = state.project.tracks[ti].rack[slot_idx].plugin_name.clone();
        draw_pixel_label(
            canvas,
            &state.theme,
            &plugin_name,
            sx + 8,
            sy + 6,
            slot_w - 40,
            if slot_enabled {
                Theme::c(state.theme.text_primary)
            } else {
                Theme::c(state.theme.text_dim)
            },
        );

        // Enable/disable toggle
        let toggle_id = input.next_id();
        let toggle_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: toggle_id,
                x: sx + slot_w - 58,
                y: sy + 3,
                width: 28,
                height: 14,
                label: if slot_enabled { "ON" } else { "OFF" }.into(),
                toggled: slot_enabled,
                icon: ButtonIcon::None,
                hint: Some("Toggle effect on/off".into()),

                ..Default::default()
            },
        );
        if toggle_clicked {
            let track_id = state.project.tracks[ti].id;
            state.commands.execute(
                Box::new(crate::app::commands::RackSlotToggle {
                    track_id,
                    slot_idx,
                    old_enabled: slot_enabled,
                }),
                &mut state.project,
            );
            state.dirty = true;
        }

        // Delete button
        let del_id = input.next_id();
        let del_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: del_id,
                x: sx + slot_w - 26,
                y: sy + 3,
                width: 20,
                height: 14,
                label: "X".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Remove module".into()),
                ..Default::default()
            },
        );
        if del_clicked {
            let track_id = state.project.tracks[ti].id;
            state.commands.execute(
                Box::new(crate::app::commands::RackSlotRemove {
                    track_id,
                    slot_idx,
                    removed_slot: None,
                }),
                &mut state.project,
            );
            state.dirty = true;
            state.push_status("Module removed".to_string());
            break; // Re-render next frame after removal
        }

        // ── Drag-to-reorder: right-click drag on header to reorder ──
        let header_hover = input.mouse_in_rect(sx, sy, slot_w - 54, 20);
        if header_hover && input.right_mouse_pressed {
            state.rack_reorder_drag = Some(slot_idx);
        }
        // Show drop target indicator when reordering
        if state.rack_reorder_drag.is_some() {
            let slot_center = sx + slot_w / 2;
            let in_slot = input.mouse_in_rect(sx - slot_gap / 2, sy, slot_w + slot_gap, slot_h);
            if in_slot {
                let insert_before = input.mouse_x < slot_center;
                let target = if insert_before {
                    slot_idx
                } else {
                    slot_idx + 1
                };
                state.rack_reorder_target = Some(target);
                // Draw insertion indicator
                let ind_x = if insert_before {
                    sx - 2
                } else {
                    sx + slot_w + 2
                };
                canvas.set_draw_color(Theme::c(state.theme.accent));
                let _ = canvas.fill_rect(Rect::new(ind_x, sy, 3, slot_h as u32));
            }
        }

        // ── Module panel drag: show drop zone indicators between modules ──
        if let Some(ref drag_name) = state.module_drag.clone() {
            let slot_center = sx + slot_w / 2;
            let in_slot = input.mouse_in_rect(sx - slot_gap / 2, sy, slot_w + slot_gap, slot_h);
            if in_slot {
                // Check if the dragged module is the same category as this slot
                let drag_is_instrument = crate::modules::is_instrument(drag_name);
                let drag_is_midi_fx = crate::modules::is_midi_effect(drag_name);
                let drag_is_fx = crate::modules::is_effect(drag_name);
                let slot_is_instrument = crate::modules::is_instrument(
                    &state.project.tracks[ti].rack[slot_idx].plugin_name,
                );
                let slot_is_midi_fx = crate::modules::is_midi_effect(
                    &state.project.tracks[ti].rack[slot_idx].plugin_name,
                );
                let slot_is_fx =
                    crate::modules::is_effect(&state.project.tracks[ti].rack[slot_idx].plugin_name);

                let same_category = (drag_is_instrument && slot_is_instrument)
                    || (drag_is_midi_fx && slot_is_midi_fx)
                    || (drag_is_fx && slot_is_fx);

                // Only treat as "replace" when mouse is in the center of the slot.
                // Edges (left 25%, right 25%) are treated as "insert" even for
                // same-category, so dragging past the last module inserts a new
                // one instead of replacing the last.
                let edge_frac = slot_w / 4;
                let mouse_in_center =
                    input.mouse_x >= sx + edge_frac && input.mouse_x <= sx + slot_w - edge_frac;

                if same_category && mouse_in_center {
                    // Highlight the slot for replacement
                    state.module_drag_replace_idx = Some(slot_idx);
                    state.module_drag_insert_idx = None;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 180, 60, 80));
                    let _ = canvas.fill_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 180, 60, 220));
                    let _ = canvas.draw_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));
                } else {
                    // Insert between slots (or at edges of same-category slots)
                    state.module_drag_replace_idx = None;
                    let insert_before = input.mouse_x < slot_center;
                    let target = if insert_before {
                        slot_idx
                    } else {
                        slot_idx + 1
                    };
                    state.module_drag_insert_idx = Some(target);
                    let ind_x = if insert_before {
                        sx - 2
                    } else {
                        sx + slot_w + 2
                    };
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 100, 220));
                    let _ = canvas.fill_rect(Rect::new(ind_x, sy, 3, slot_h as u32));
                }
            }
        }

        // Draw parameters as knobs (or dropdowns for option params)
        let knob_y_start = sy + 24;

        // Get track_id and slot_id for highlight check
        let track_id = state.project.tracks[ti].id;
        let slot_id = state.project.tracks[ti].rack[slot_idx].slot_id;

        for pi in 0..param_count {
            let col = pi % cols;
            let row = pi / cols;
            let kx = sx + (knob_cell_w / 2) + col as i32 * knob_cell_w;
            let ky = knob_y_start + 20 + row as i32 * 64;

            if ky + 30 > sy + slot_h {
                break; // Don't draw knobs that would overflow
            }

            let param = &state.project.tracks[ti].rack[slot_idx].params[pi];
            let knob_id = input.next_id();
            let mut val = param.value;
            let p_min = param.min;
            let p_max = param.max;
            let p_default = param.default;
            let p_name = param.name.clone();
            let p_id = param.id.clone();
            let is_bipolar = p_min < 0.0;

            // Check if this param should be highlighted
            let is_highlighted = state
                .rack_highlight_param
                .as_ref()
                .map(|(ht, hs, hp)| *ht == track_id && *hs == slot_id && hp == &p_id)
                .unwrap_or(false);

            // Draw highlight glow behind the knob if it's the target
            if is_highlighted && state.rack_highlight_timer > 0 {
                let pulse = ((state.rack_highlight_timer as f32 / 15.0).sin().abs() * 100.0) as u8;
                let alpha = 120 + pulse;
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, alpha));
                let _ = canvas.fill_rect(Rect::new(kx - 22, ky - 22, 44, 72));
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 220, 100, 255));
                let _ = canvas.draw_rect(Rect::new(kx - 22, ky - 22, 44, 72));
            }

            // Look up static param desc for options
            let static_descs = crate::modules::get_param_descs(&plugin_name);
            let has_options = static_descs
                .iter()
                .find(|d| d.id == p_id)
                .and_then(|d| d.options);

            if let Some(opts) = has_options {
                // ── Render as draggable selector (drag up/down or click to cycle) ──
                let idx = val.round() as usize;
                let label_text = if idx < opts.len() { opts[idx] } else { "?" };
                let sel_w = (knob_cell_w - 8).max(36);
                let sel_x = kx - sel_w / 2;
                let sel_y = ky - 8;
                let sel_h = 18;

                // Draw background
                let hover = input.mouse_in_rect(sel_x, sel_y, sel_w, sel_h);
                let dragging = input.drag_widget == knob_id;
                let bg_color = if dragging {
                    sdl2::pixels::Color::RGBA(70, 80, 110, 255)
                } else if hover {
                    sdl2::pixels::Color::RGBA(60, 65, 80, 255)
                } else {
                    sdl2::pixels::Color::RGBA(40, 44, 56, 255)
                };
                canvas.set_draw_color(bg_color);
                let _ = canvas.fill_rect(Rect::new(sel_x, sel_y, sel_w as u32, sel_h as u32));
                canvas.set_draw_color(Theme::c(state.theme.panel_border));
                let _ = canvas.draw_rect(Rect::new(sel_x, sel_y, sel_w as u32, sel_h as u32));

                // Draw label text (param name above)
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &p_name,
                    sel_x,
                    sel_y - 12,
                    sel_w,
                    Theme::c(state.theme.text_dim),
                );
                // Draw current value
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    label_text,
                    sel_x + 4,
                    sel_y + 4,
                    sel_w - 8,
                    Theme::c(state.theme.text_primary),
                );

                // Scroll wheel cycles through options
                if hover && input.scroll_y != 0 && !input.scroll_consumed {
                    let n = opts.len();
                    let old_val = val;
                    let new_idx = if input.scroll_y > 0 {
                        idx.saturating_sub(1)
                    } else {
                        (idx + 1).min(n - 1)
                    };
                    val = new_idx as f32;
                    state.project.tracks[ti].rack[slot_idx].params[pi].value = val;
                    state.dirty = true;
                    input.scroll_consumed = true;
                    if (old_val - val).abs() > 1e-4 {
                        let track_id = state.project.tracks[ti].id;
                        state.commands.execute(
                            Box::new(crate::app::commands::SetRackParam {
                                track_id,
                                slot_idx,
                                param_idx: pi,
                                old_value: old_val,
                                new_value: val,
                            }),
                            &mut state.project,
                        );
                    }
                }

                // Start drag on press
                if hover && input.mouse_pressed && !input.consumed {
                    input.drag_widget = knob_id;
                    input.consume();
                }

                // Drag to change value (vertical drag like knobs)
                if dragging && input.mouse_down {
                    let dy = -input.mouse_dy as f32;
                    let sensitivity = if input.shift() { 0.05 } else { 0.15 };
                    let delta = dy * sensitivity;
                    if delta.abs() > 0.001 {
                        let old_val = state.project.tracks[ti].rack[slot_idx].params[pi].value;
                        let new_val = (old_val + delta).clamp(p_min, p_max);
                        let old_idx = old_val.round() as usize;
                        let new_idx = new_val.round() as usize;
                        state.project.tracks[ti].rack[slot_idx].params[pi].value = new_val;
                        state.dirty = true;

                        if old_idx != new_idx {
                            let track_id = state.project.tracks[ti].id;
                            state.commands.execute(
                                Box::new(crate::app::commands::SetRackParam {
                                    track_id,
                                    slot_idx,
                                    param_idx: pi,
                                    old_value: old_val,
                                    new_value: new_val,
                                }),
                                &mut state.project,
                            );
                        }
                    }
                }

                // Release drag
                if dragging && !input.mouse_down {
                    // Snap to nearest integer on release
                    let v = state.project.tracks[ti].rack[slot_idx].params[pi].value;
                    state.project.tracks[ti].rack[slot_idx].params[pi].value = v.round();
                    input.drag_widget = WidgetId::None;
                }

                // Click (no drag motion) to cycle: left click = forward, right click = backward
                let left_cycle = hover
                    && input.mouse_released
                    && input.drag_widget == WidgetId::None
                    && !input.consumed;
                let right_cycle = hover && input.right_mouse_released && !input.consumed;

                if left_cycle || right_cycle {
                    let old_val = val;
                    let go_back = right_cycle || input.shift();
                    let new_idx = if go_back {
                        if idx == 0 {
                            opts.len() - 1
                        } else {
                            idx - 1
                        }
                    } else {
                        (idx + 1) % opts.len()
                    };
                    val = new_idx as f32;
                    state.project.tracks[ti].rack[slot_idx].params[pi].value = val;
                    state.dirty = true;
                    input.consume();

                    if (old_val - val).abs() > 1e-4 {
                        let track_id = state.project.tracks[ti].id;
                        state.commands.execute(
                            Box::new(crate::app::commands::SetRackParam {
                                track_id,
                                slot_idx,
                                param_idx: pi,
                                old_value: old_val,
                                new_value: val,
                            }),
                            &mut state.project,
                        );
                    }
                }
            } else {
                // ── Render as knob (default for continuous params) ──
                {
                    // Build human-readable hint for time-mapped params (attack/release)
                    let knob_hint = if p_id == "attack" && plugin_name.contains("Compressor") {
                        let ms = (0.0003 * (1000.0_f64).powf(val as f64) * 1000.0) as i32;
                        format!("Attack: {}ms", ms)
                    } else if p_id == "release" && plugin_name.contains("Compressor") {
                        let ms = (0.005 * (400.0_f64).powf(val as f64) * 1000.0) as i32;
                        format!("Release: {}ms", ms)
                    } else {
                        p_name.clone()
                    };
                    let changed = crate::widgets::knob(
                        canvas,
                        input,
                        &state.theme,
                        &crate::widgets::KnobParams {
                            id: knob_id,
                            x: kx,
                            y: ky,
                            radius: 16,
                            min: p_min,
                            max: p_max,
                            sensitivity: 0.004,
                            label: Some(p_name.clone()),
                            bipolar: is_bipolar,
                            default_value: Some(p_default),
                            hint: Some(knob_hint),
                            snap_points: vec![],
                        },
                        &mut val,
                    );

                    if changed {
                        state.project.tracks[ti].rack[slot_idx].params[pi].value = val;
                        state.dirty = true;
                    }
                    // Commit rack param change on mouse release
                    if input.mouse_released && input.drag_widget == knob_id {
                        let old_val = input.drag_start_value as f32;
                        if (old_val - val).abs() > 1e-4 {
                            let track_id = state.project.tracks[ti].id;
                            state.commands.execute(
                                Box::new(crate::app::commands::SetRackParam {
                                    track_id,
                                    slot_idx,
                                    param_idx: pi,
                                    old_value: old_val,
                                    new_value: val,
                                }),
                                &mut state.project,
                            );
                        }
                    }
                }
            }

            // Ctrl+click on knob area → create automation lane for this parameter
            let knob_hover = input.mouse_in_rect(kx - 16, ky - 16, 32, 50);
            if knob_hover && input.mouse_pressed && input.ctrl() {
                let track_id = state.project.tracks[ti].id;
                let slot_id = state.project.tracks[ti].rack[slot_idx].slot_id;
                let param_id = state.project.tracks[ti].rack[slot_idx].params[pi]
                    .id
                    .clone();
                let param_name_str = state.project.tracks[ti].rack[slot_idx].params[pi]
                    .name
                    .clone();
                let target_key = format!("{}:{}:{}", track_id, slot_id, param_id);

                // Check if an automation track for this target already exists
                let already_exists = state.project.tracks.iter().any(|t| {
                    t.track_type == crate::app::models::TrackType::Automation
                        && t.clips.iter().any(|c| {
                            if let crate::app::models::Clip::Automation(ac) = c {
                                ac.target_param == target_key
                            } else {
                                false
                            }
                        })
                });

                if !already_exists {
                    let new_id = state.project.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                    let track_name = state.project.tracks[ti].name.clone();
                    let auto_name = format!("{} - {}", track_name, param_name_str);
                    let mut auto_track = crate::app::models::Track::new(
                        new_id,
                        &auto_name,
                        crate::app::models::TrackType::Automation,
                    );
                    // Normalize current value to 0–1 range for automation
                    let p_min = state.project.tracks[ti].rack[slot_idx].params[pi].min;
                    let p_max = state.project.tracks[ti].rack[slot_idx].params[pi].max;
                    let norm_val = if (p_max - p_min).abs() > 1e-9 {
                        ((val - p_min) / (p_max - p_min)).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    // Add a default automation clip spanning 16 beats
                    auto_track.clips.push(crate::app::models::Clip::Automation(
                        crate::app::models::AutomationClip {
                            points: vec![
                                crate::app::models::AutomationPoint {
                                    time: 0.0,
                                    value: norm_val,
                                },
                                crate::app::models::AutomationPoint {
                                    time: 16.0,
                                    value: norm_val,
                                },
                            ],
                            start_time: 0.0,
                            length: 16.0,
                            target_param: target_key.clone(),
                            name: auto_name.clone(),
                            color: [220, 180, 80, 200],
                        },
                    ));
                    state.commands.execute(
                        Box::new(crate::app::commands::AddTrack { track: auto_track }),
                        &mut state.project,
                    );
                    state.selected_track = Some(new_id);
                    state.selected_tracks.clear();
                    state.selected_tracks.insert(new_id);
                    state.dirty = true;
                    state.push_status(format!("Created automation: {}", auto_name));
                } else {
                    state.push_status(format!("Automation for {} already exists", param_name_str));
                }
            }
        }

        // ── Sampler waveform display ──
        if is_sampler {
            let wf_x = sx + 10;
            let wf_y = sy + slot_h - 64;
            let wf_w = (slot_w - 20) as u32;
            let wf_h = 50u32;
            // Background
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 25, 255));
            let _ = canvas.fill_rect(Rect::new(wf_x, wf_y, wf_w, wf_h));
            canvas.set_draw_color(Theme::c(state.theme.panel_border));
            let _ = canvas.draw_rect(Rect::new(wf_x, wf_y, wf_w, wf_h));

            // Show sample file name or "Drop sample here"
            let has_file = state.project.tracks[ti].sampler_file.is_some();
            if has_file {
                let fname = state.project.tracks[ti].sampler_file.as_ref().unwrap();
                let display = fname.rsplit('/').next().unwrap_or(fname);
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    display,
                    wf_x + 4,
                    wf_y + 2,
                    wf_w as i32 - 8,
                    Theme::c(state.theme.text_dim),
                );
                // Draw actual waveform from cache
                let mid_y = wf_y + 12 + (wf_h as i32 - 14) / 2;
                let draw_h = (wf_h as i32 - 14) / 2;
                if let Some((peaks, _dur)) = state.waveform_cache.get(fname) {
                    let n = peaks.len();
                    let draw_w = wf_w as i32 - 4;
                    if n > 0 && draw_w > 0 {
                        canvas.set_draw_color(Theme::c(state.theme.accent));
                        for px in 0..draw_w {
                            let idx = (px as usize * n) / draw_w as usize;
                            let idx = idx.min(n - 1);
                            let amp = peaks[idx].clamp(0.0, 1.0);
                            let h = (amp * draw_h as f32) as i32;
                            if h > 0 {
                                let _ = canvas.draw_line(
                                    sdl2::rect::Point::new(wf_x + 2 + px, mid_y - h),
                                    sdl2::rect::Point::new(wf_x + 2 + px, mid_y + h),
                                );
                            }
                        }
                    }
                } else {
                    // Fallback: center line while loading
                    canvas.set_draw_color(Theme::c(state.theme.accent));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(wf_x + 2, mid_y),
                        sdl2::rect::Point::new(wf_x + wf_w as i32 - 2, mid_y),
                    );
                }
            } else {
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    "Drop sample here",
                    wf_x + 4,
                    wf_y + (wf_h as i32 - 10) / 2,
                    wf_w as i32 - 8,
                    Theme::c(state.theme.text_dim),
                );
            }

            // Handle file drop onto waveform box
            let wf_hover = input.mouse_in_rect(wf_x, wf_y, wf_w as i32, wf_h as i32);
            if wf_hover {
                // OS-level file drop
                if let Some(ref dropped) = input.dropped_file {
                    let path = dropped.clone();
                    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
                    if ext == "wav" || ext == "flac" || ext == "ogg" || ext == "mp3" {
                        let track_id = state.project.tracks[ti].id;
                        let old_value = state.project.tracks[ti].sampler_file.clone();
                        state.commands.execute(
                            Box::new(crate::app::commands::SetSamplerFile {
                                track_id,
                                old_value,
                                new_value: Some(path),
                            }),
                            &mut state.project,
                        );
                        state.dirty = true;
                        state.push_status("Sample loaded".to_string());
                    }
                }
                // In-app file browser drag-drop
                if input.mouse_released {
                    if let Some(ref drag_file) = state.sample_drag_path.clone() {
                        let path = drag_file.to_string_lossy().to_string();
                        let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
                        if ext == "wav" || ext == "flac" || ext == "ogg" || ext == "mp3" {
                            let track_id = state.project.tracks[ti].id;
                            let old_value = state.project.tracks[ti].sampler_file.clone();
                            state.commands.execute(
                                Box::new(crate::app::commands::SetSamplerFile {
                                    track_id,
                                    old_value,
                                    new_value: Some(path),
                                }),
                                &mut state.project,
                            );
                            state.dirty = true;
                            state.push_status("Sample loaded from browser".to_string());
                            state.sample_drag_path = None;
                            state.sample_drag_len_beats = None;
                        }
                    }
                }
            }
        }

        // ── Sidechain picker (Compressor only) ──────────────────────────────
        // Show a small track selector row so the user can pick an external key signal.
        let is_compressor = state.project.tracks[ti].rack[slot_idx].plugin_name == "Compressor";
        if is_compressor
            && crate::modules::is_effect(&state.project.tracks[ti].rack[slot_idx].plugin_name)
        {
            // Row sits just above the visual feedback area, below the knobs
            let rows_used_sc = if param_count == 0 {
                0
            } else {
                param_count.div_ceil(cols)
            };
            let sc_y = sy + 24 + 20 + rows_used_sc as i32 * 64;
            if sc_y + 18 < sy + slot_h {
                // Label
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    "SIDECHAIN",
                    sx + 8,
                    sc_y,
                    60,
                    Theme::c(state.theme.text_dim),
                );

                // Current sidechain track id
                let sc_track_id = state.project.tracks[ti].rack[slot_idx].sidechain_track_id;

                // Build display name
                let sc_label: String = if let Some(sc_id) = sc_track_id {
                    state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == sc_id)
                        .map(|t| {
                            if t.name.is_empty() {
                                format!("Track {}", sc_id)
                            } else {
                                t.name.clone()
                            }
                        })
                        .unwrap_or_else(|| "?".to_string())
                } else {
                    "Self".to_string()
                };

                // Dropdown button
                let sc_btn_x = sx + 72;
                let sc_btn_w = (slot_w - 80).max(60);
                let sc_btn_h = 14;
                let sc_hover = input.mouse_in_rect(sc_btn_x, sc_y, sc_btn_w, sc_btn_h);
                let sc_bg = if sc_hover {
                    sdl2::pixels::Color::RGBA(60, 65, 90, 255)
                } else {
                    sdl2::pixels::Color::RGBA(35, 38, 50, 255)
                };
                canvas.set_draw_color(sc_bg);
                let _ =
                    canvas.fill_rect(Rect::new(sc_btn_x, sc_y, sc_btn_w as u32, sc_btn_h as u32));
                canvas.set_draw_color(Theme::c(state.theme.panel_border));
                let _ =
                    canvas.draw_rect(Rect::new(sc_btn_x, sc_y, sc_btn_w as u32, sc_btn_h as u32));
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &sc_label,
                    sc_btn_x + 4,
                    sc_y + 2,
                    sc_btn_w - 8,
                    if sc_track_id.is_some() {
                        Theme::c(state.theme.accent)
                    } else {
                        Theme::c(state.theme.text_primary)
                    },
                );

                // Click or drag: cycle through tracks (None → track0 → track1 → … → None)
                let sc_widget_id = input.next_id();
                let sc_dragging = input.drag_widget == sc_widget_id;

                // Start drag
                if sc_hover && input.mouse_pressed && !input.consumed {
                    input.drag_widget = sc_widget_id;
                    input.consume();
                }

                // Helper closure to build choices list
                let build_sc_choices = |tracks: &Vec<crate::app::models::Track>,
                                        self_id: u32|
                 -> Vec<Option<u32>> {
                    std::iter::once(None)
                        .chain(
                            tracks
                                .iter()
                                .filter(|t| {
                                    t.id != self_id
                                        && t.track_type != crate::app::models::TrackType::Automation
                                })
                                .map(|t| Some(t.id)),
                        )
                        .collect()
                };

                // Drag: move up/down to change selection
                if sc_dragging && input.mouse_down {
                    let dy = { -input.mouse_dy };
                    if dy.abs() >= 8 {
                        // enough drag motion
                        let track_id_cur = state.project.tracks[ti].id;
                        let choices = build_sc_choices(&state.project.tracks, track_id_cur);
                        let cur_sc = state.project.tracks[ti].rack[slot_idx].sidechain_track_id;
                        let current_idx = choices.iter().position(|c| *c == cur_sc).unwrap_or(0);
                        let new_idx = if dy > 0 {
                            if current_idx == 0 {
                                choices.len() - 1
                            } else {
                                current_idx - 1
                            }
                        } else {
                            (current_idx + 1) % choices.len()
                        };
                        let new_sc = choices[new_idx];
                        if new_sc != cur_sc {
                            state.commands.execute(
                                Box::new(crate::app::commands::SetRackSidechain {
                                    track_id: track_id_cur,
                                    slot_idx,
                                    old_sc: cur_sc,
                                    new_sc,
                                }),
                                &mut state.project,
                            );
                            state.dirty = true;
                        }
                    }
                }
                if sc_dragging && !input.mouse_down {
                    input.drag_widget = WidgetId::None;
                }

                // Click (release with no drag) to cycle forward/backward
                // Left click = forward, right click = backward
                let sc_left_click = sc_hover
                    && input.mouse_released
                    && !sc_dragging
                    && input.drag_widget == WidgetId::None
                    && !input.consumed;
                let sc_right_click = sc_hover && input.right_mouse_released && !input.consumed;

                if sc_left_click || sc_right_click {
                    input.consume();
                    let track_id_cur = state.project.tracks[ti].id;
                    let choices = build_sc_choices(&state.project.tracks, track_id_cur);
                    let current_idx = choices.iter().position(|c| *c == sc_track_id).unwrap_or(0);
                    let go_back = sc_right_click || input.shift();
                    let next_idx = if go_back {
                        if current_idx == 0 {
                            choices.len() - 1
                        } else {
                            current_idx - 1
                        }
                    } else {
                        (current_idx + 1) % choices.len()
                    };
                    let new_sc = choices[next_idx];

                    state.commands.execute(
                        Box::new(crate::app::commands::SetRackSidechain {
                            track_id: track_id_cur,
                            slot_idx,
                            old_sc: sc_track_id,
                            new_sc,
                        }),
                        &mut state.project,
                    );
                    state.dirty = true;
                }

                // Middle-click: open dropdown popup for quick sidechain selection
                if sc_hover && input.middle_mouse_pressed && !input.consumed {
                    input.consume();
                    state.sc_popup_open = true;
                    state.sc_popup_x = sc_btn_x;
                    state.sc_popup_y = sc_y + sc_btn_h;
                    state.sc_popup_track_idx = ti;
                    state.sc_popup_slot_idx = slot_idx;
                }
            }
        }

        // ── Module Visual Feedback Display (right-side panel for effects) ──
        // Drawn to the right of the knob columns for effect modules that have
        // a useful graph. Instrument slots and Gain/Utility/Reverb/Chorus have no visual.
        if has_vis_panel && !is_sampler {
            let vis_x = sx + knob_cols_w + 4;
            let vis_y = sy + 22; // just below the header
            let vis_w = (vis_col_w - 8).max(20) as u32;
            let vis_h = (slot_h - 26).max(20);

            // Background
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 18, 22, 255));
            let _ = canvas.fill_rect(Rect::new(vis_x, vis_y, vis_w, vis_h as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 50, 200));
            let _ = canvas.draw_rect(Rect::new(vis_x, vis_y, vis_w, vis_h as u32));

            let plugin_ref = state.project.tracks[ti].rack[slot_idx].plugin_name.as_str();
            let params_snap: Vec<(String, f32)> = state.project.tracks[ti].rack[slot_idx]
                .params
                .iter()
                .map(|p| (p.id.clone(), p.value))
                .collect();

            // ── NOTE: Track rack vis must stay in sync with master rack vis ──
            // When updating any effect visualization here, the same change
            // MUST be applied in draw_master_rack for the master rack.
            match plugin_ref {
                "LP Filter" | "HP Filter" => {
                    let is_lp = plugin_ref == "LP Filter";
                    let cutoff = params_snap
                        .iter()
                        .find(|(k, _)| k == "cutoff")
                        .map(|(_, v)| *v)
                        .unwrap_or(if is_lp { 1.0 } else { 0.0 });
                    let reso = params_snap
                        .iter()
                        .find(|(k, _)| k == "resonance")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);

                    let w_f = vis_w as f32;
                    let h_f = vis_h as f32;
                    let accent = Theme::c(state.theme.accent);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));

                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        let t = px as f32 / w_f;
                        let freq = 20.0_f32 * (20000.0_f32 / 20.0).powf(t);
                        let cutoff_hz = 20.0_f32 * (20000.0_f32 / 20.0_f32).powf(cutoff);
                        let ratio = freq / cutoff_hz;
                        let q = 0.5 + reso * 10.0;
                        let r2 = ratio * ratio;
                        let denom = ((1.0 - r2) * (1.0 - r2) + r2 / (q * q)).sqrt();
                        let mag = if is_lp {
                            (1.0 / denom).min(4.0)
                        } else {
                            (r2 / denom).min(4.0)
                        };
                        let db = 20.0 * mag.max(0.001).log10();
                        let y_norm = 0.5 - db / 40.0;
                        let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }
                    // 0dB reference line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 120));
                    let ref_y = vis_y + vis_h / 2;
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, ref_y),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, ref_y),
                    );
                }

                "Compressor" => {
                    // Static compression curve on top half
                    let threshold = params_snap
                        .iter()
                        .find(|(k, _)| k == "threshold")
                        .map(|(_, v)| *v)
                        .unwrap_or(-18.0);
                    let ratio = params_snap
                        .iter()
                        .find(|(k, _)| k == "ratio")
                        .map(|(_, v)| *v)
                        .unwrap_or(4.0);
                    let knee_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "knee")
                        .map(|(_, v)| *v)
                        .unwrap_or(6.0);
                    let accent = Theme::c(state.theme.accent);

                    // Curve shows -60..0 dBFS on both axes
                    let curve_h = (vis_h * 2 / 3).max(20);
                    let w_f = vis_w as f32;
                    let h_f = curve_h as f32;
                    let thresh_db = threshold; // already in dBFS
                    let comp_ratio = ratio.max(1.0);

                    // Draw unity diagonal (faint)
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 100));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + curve_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
                    );

                    // Draw compression curve
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));
                    let slope = 1.0 - 1.0 / comp_ratio;
                    let half_knee = knee_db * 0.5;
                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        // Map pixel to dB input (-60..0)
                        let in_db = -60.0 + (px as f32 / w_f) * 60.0;
                        let over = in_db - thresh_db;
                        let gr = if over <= -half_knee {
                            0.0_f32
                        } else if over >= half_knee {
                            -slope * over
                        } else {
                            let x = over + half_knee;
                            let t = x / knee_db.max(0.01);
                            -slope * knee_db * t * t * 0.5
                        };
                        let out_db = in_db + gr;
                        let y_norm = 1.0 - (out_db + 60.0) / 60.0;
                        let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }
                    // Threshold line
                    let thresh_x_norm = (thresh_db + 60.0) / 60.0;
                    let thresh_x = vis_x + (thresh_x_norm * w_f) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 80, 150));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(thresh_x, vis_y),
                        sdl2::rect::Point::new(thresh_x, vis_y + curve_h),
                    );

                    // ── Input + GR meters on bottom third ──
                    let meter_y = vis_y + curve_h + 2;
                    let meter_h = (vis_h - curve_h - 4).max(4);
                    let track_rms_pre: f32 = state
                        .meters
                        .track_rms_pre_effect
                        .get(ti)
                        .copied()
                        .unwrap_or(0.0);

                    let in_db_rms = if track_rms_pre > 1e-6 {
                        20.0 * track_rms_pre.log10()
                    } else {
                        -60.0_f32
                    };
                    // Use actual gain reduction from audio engine
                    let gr_db_live = state
                        .meters
                        .track_effect_gr
                        .get(ti)
                        .and_then(|v| v.get(slot_idx))
                        .copied()
                        .unwrap_or(0.0)
                        .min(0.0);

                    // ── Real-time dot on the compression curve ──
                    // Shows current audio level riding along the curve.
                    {
                        let dot_in_db = in_db_rms;
                        let dot_over = dot_in_db - thresh_db;
                        let dot_gr = if dot_over <= -half_knee {
                            0.0_f32
                        } else if dot_over >= half_knee {
                            -slope * dot_over
                        } else {
                            let x = dot_over + half_knee;
                            let t = x / knee_db.max(0.01);
                            -slope * knee_db * t * t * 0.5
                        };
                        let dot_out_db = dot_in_db + dot_gr;
                        let dot_x_norm = (dot_in_db + 60.0) / 60.0;
                        let dot_y_norm = 1.0 - (dot_out_db + 60.0) / 60.0;
                        let dot_px = vis_x + (dot_x_norm * w_f).clamp(0.0, w_f - 1.0) as i32;
                        let dot_py = vis_y + (dot_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        // Draw a filled circle (radius 3) as the dot
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 100, 240));
                        for dy in -3i32..=3 {
                            for dx in -3i32..=3 {
                                if dx * dx + dy * dy <= 9 {
                                    let _ = canvas.draw_point(sdl2::rect::Point::new(
                                        dot_px + dx,
                                        dot_py + dy,
                                    ));
                                }
                            }
                        }
                    }

                    // Draw input level bar
                    let in_meter_w = (vis_w as i32 - 4) / 2 - 1;
                    let in_frac = ((in_db_rms + 60.0) / 60.0).clamp(0.0, 1.0);
                    let in_fill_w = (in_frac * in_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        vis_x + 2,
                        meter_y,
                        in_meter_w as u32,
                        meter_h as u32,
                    ));
                    if in_fill_w > 0 {
                        let col = if in_frac > 0.85 {
                            sdl2::pixels::Color::RGBA(220, 60, 60, 230)
                        } else if in_frac > 0.6 {
                            sdl2::pixels::Color::RGBA(200, 180, 50, 230)
                        } else {
                            sdl2::pixels::Color::RGBA(60, 180, 80, 230)
                        };
                        canvas.set_draw_color(col);
                        let _ = canvas.fill_rect(Rect::new(
                            vis_x + 2,
                            meter_y,
                            in_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "IN",
                        vis_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(140, 200, 140, 160),
                    );

                    // Draw GR bar (fills from right, as GR increases)
                    let gr_meter_x = vis_x + 2 + in_meter_w + 2;
                    let gr_meter_w = vis_w as i32 - 4 - in_meter_w - 2;
                    let gr_frac = ((-gr_db_live) / 24.0).clamp(0.0, 1.0);
                    let gr_fill_w = (gr_frac * gr_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        gr_meter_x,
                        meter_y,
                        gr_meter_w as u32,
                        meter_h as u32,
                    ));
                    if gr_fill_w > 0 {
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 60, 220));
                        let _ = canvas.fill_rect(Rect::new(
                            gr_meter_x,
                            meter_y,
                            gr_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "GR",
                        gr_meter_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(180, 100, 100, 160),
                    );

                    // Sidechain indicator
                    let sc_id = state.project.tracks[ti].rack[slot_idx].sidechain_track_id;
                    if sc_id.is_some() {
                        draw_pixel_label(
                            canvas,
                            &state.theme,
                            "SC",
                            vis_x + vis_w as i32 - 18,
                            meter_y + 1,
                            14,
                            sdl2::pixels::Color::RGBA(120, 220, 255, 220),
                        );
                    }
                }

                "EQ" => {
                    let lo_gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "lo_gain")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(0.0);
                    let mid_gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "mid_gain")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(0.0);
                    let hi_gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "hi_gain")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(0.0);
                    let lo_freq = params_snap
                        .iter()
                        .find(|(k, _)| k == "lo_freq")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(200.0);
                    let mid_freq = params_snap
                        .iter()
                        .find(|(k, _)| k == "mid_freq")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(1000.0);
                    let hi_freq = params_snap
                        .iter()
                        .find(|(k, _)| k == "hi_freq")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(4000.0);
                    let sr = 44100.0_f64;
                    let lo_c = crate::modules::BiquadCoeffs::low_shelf(lo_freq, lo_gain_db, sr);
                    let mid_c =
                        crate::modules::BiquadCoeffs::peaking(mid_freq, mid_gain_db, 0.7, sr);
                    let hi_c = crate::modules::BiquadCoeffs::high_shelf(hi_freq, hi_gain_db, sr);

                    let db_range = 18.0_f64;
                    let log_min = (20.0_f64).ln();
                    let log_max = (20000.0_f64).ln();
                    let w_i = vis_w as i32;
                    let h_f = vis_h as f64;
                    let mid_line = vis_y + vis_h / 2;

                    // Build pixel-per-column Y values
                    let mut curve_py: Vec<i32> = Vec::with_capacity(w_i as usize);
                    for col in 0..w_i {
                        let t = col as f64 / (w_i - 1).max(1) as f64;
                        let freq = (log_min + t * (log_max - log_min)).exp();
                        let omega = 2.0 * std::f64::consts::PI * freq / sr;
                        let mag = lo_c.magnitude_at(omega)
                            * mid_c.magnitude_at(omega)
                            * hi_c.magnitude_at(omega);
                        let db = if mag > 1e-10 {
                            20.0 * mag.log10()
                        } else {
                            -db_range
                        };
                        let db_clamped = db.clamp(-db_range, db_range);
                        let py = mid_line - (db_clamped / db_range * (h_f / 2.0)) as i32;
                        curve_py.push(py.clamp(vis_y, vis_y + vis_h - 1));
                    }

                    // 0 dB reference line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 120));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, mid_line),
                        sdl2::rect::Point::new(vis_x + w_i, mid_line),
                    );

                    // Filled area between curve and centre
                    let accent = Theme::c(state.theme.accent);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 30,
                    ));
                    for col in 0..w_i {
                        let px = vis_x + col;
                        let py = curve_py[col as usize];
                        let (ya, yb) = if py <= mid_line {
                            (py, mid_line)
                        } else {
                            (mid_line, py)
                        };
                        let fill_h = (yb - ya + 1).max(1) as u32;
                        let _ = canvas.fill_rect(Rect::new(px, ya, 1, fill_h));
                    }

                    // Curve line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));
                    for col in 1..w_i {
                        let _ = canvas.draw_line(
                            sdl2::rect::Point::new(vis_x + col - 1, curve_py[(col - 1) as usize]),
                            sdl2::rect::Point::new(vis_x + col, curve_py[col as usize]),
                        );
                    }
                }

                "Distortion" => {
                    let drive = params_snap
                        .iter()
                        .find(|(k, _)| k == "drive")
                        .map(|(_, v)| *v)
                        .unwrap_or(1.0);
                    let accent = Theme::c(state.theme.accent);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));
                    let w_f = vis_w as f32;
                    let h_f = vis_h as f32;
                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        let inp = (px as f32 / w_f) * 2.0 - 1.0;
                        let driven = inp * (1.0 + drive * 9.0);
                        let output = driven.tanh();
                        let y_norm = 0.5 - output * 0.45;
                        let y_px = vis_y + (y_norm * h_f) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 100));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + vis_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
                    );
                }

                "Delay" => {
                    let div_l = params_snap
                        .iter()
                        .find(|(k, _)| k == "time_l")
                        .map(|(_, v)| v.round() as usize)
                        .unwrap_or(5);
                    let div_r = params_snap
                        .iter()
                        .find(|(k, _)| k == "time_r")
                        .map(|(_, v)| v.round() as usize)
                        .unwrap_or(3);
                    let feedback = params_snap
                        .iter()
                        .find(|(k, _)| k == "feedback")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.3);
                    let mix = params_snap
                        .iter()
                        .find(|(k, _)| k == "mix")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.5);
                    let w_f = vis_w as f32;
                    let h_f = vis_h as f32;
                    let half_h = (h_f * 0.5) as i32;
                    let accent = Theme::c(state.theme.accent);
                    // Beat values for each division index (same order as DELAY_DIVISIONS)
                    let beat_vals: [f32; 10] = [
                        4.0,
                        2.0,
                        4.0 / 3.0,
                        1.0,
                        2.0 / 3.0,
                        0.5,
                        1.0 / 3.0,
                        0.25,
                        0.5 / 3.0,
                        0.125,
                    ];
                    // Draw L taps on top half
                    {
                        let beats = beat_vals.get(div_l).copied().unwrap_or(0.5);
                        let tap_spacing =
                            ((beats / 4.0) * w_f * 0.9).max(4.0).min(w_f * 0.45) as i32;
                        let tap_w = (tap_spacing / 3).clamp(2, 12);
                        let mut tap_x = vis_x + 2;
                        let mut level = mix;
                        for _ in 0..12 {
                            if level < 0.02 || tap_x + tap_w > vis_x + vis_w as i32 - 1 {
                                break;
                            }
                            let bar_h = (level * half_h as f32 * 0.85) as i32;
                            let bar_y = vis_y + half_h - bar_h;
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                                accent.r,
                                accent.g,
                                accent.b,
                                (level * 220.0) as u8,
                            ));
                            let _ = canvas.fill_rect(Rect::new(
                                tap_x,
                                bar_y,
                                tap_w as u32,
                                bar_h as u32,
                            ));
                            tap_x += tap_spacing;
                            level *= feedback;
                        }
                    }
                    // Draw R taps on bottom half
                    {
                        let beats = beat_vals.get(div_r).copied().unwrap_or(1.0);
                        let tap_spacing =
                            ((beats / 4.0) * w_f * 0.9).max(4.0).min(w_f * 0.45) as i32;
                        let tap_w = (tap_spacing / 3).clamp(2, 12);
                        let mut tap_x = vis_x + 2;
                        let mut level = mix;
                        for _ in 0..12 {
                            if level < 0.02 || tap_x + tap_w > vis_x + vis_w as i32 - 1 {
                                break;
                            }
                            let bar_h = (level * half_h as f32 * 0.85) as i32;
                            let bar_y = vis_y + half_h;
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                                100,
                                accent.g,
                                accent.b,
                                (level * 200.0) as u8,
                            ));
                            let _ = canvas.fill_rect(Rect::new(
                                tap_x,
                                bar_y,
                                tap_w as u32,
                                bar_h as u32,
                            ));
                            tap_x += tap_spacing;
                            level *= feedback;
                        }
                    }
                    // Centre divider
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 80));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + half_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y + half_h),
                    );
                }

                "Limiter" => {
                    let gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "gain_db")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    let ceiling_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "ceiling_db")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    let accent = Theme::c(state.theme.accent);

                    // ── Transfer curve (upper 2/3) ──
                    // X axis = input dB (-60..0), Y axis = output dB (-60..ceiling)
                    let curve_h = (vis_h as f32 * 0.65) as i32;
                    let w_f = vis_w as f32;
                    let h_f = curve_h as f32;

                    // Reference line (unity, no processing)
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 50, 60, 120));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + curve_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
                    );

                    // Draw the limiter transfer curve
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 220,
                    ));
                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        // Map pixel to input dB: 0..w → -60..0
                        let in_db_c = -60.0 + (px as f32 / w_f) * 60.0;
                        // Apply input gain
                        let after_gain = in_db_c + gain_db;
                        // Limiter: hard ceiling
                        let out_db_c = after_gain.min(ceiling_db);
                        // Map output dB to Y: -60dB = bottom, 0dB = top
                        let y_norm = 1.0 - (out_db_c + 60.0) / 60.0;
                        let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }

                    // Ceiling horizontal reference line
                    let ceil_y_norm = 1.0 - (ceiling_db + 60.0) / 60.0;
                    let ceil_y = vis_y + (ceil_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 90, 70, 160));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, ceil_y),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, ceil_y),
                    );

                    // ── Real-time dot on the limiter transfer curve ──
                    {
                        let input_rms_dot: f32 = state
                            .meters
                            .track_rms_pre_effect
                            .get(ti)
                            .copied()
                            .unwrap_or(0.0);
                        let dot_in_db = if input_rms_dot > 1e-6 {
                            20.0 * input_rms_dot.log10()
                        } else {
                            -60.0_f32
                        };
                        let dot_after_gain = dot_in_db + gain_db;
                        let dot_out_db = dot_after_gain.min(ceiling_db);
                        let dot_x_norm = (dot_in_db + 60.0) / 60.0;
                        let dot_y_norm = 1.0 - (dot_out_db + 60.0) / 60.0;
                        let dot_px = vis_x + (dot_x_norm * w_f).clamp(0.0, w_f - 1.0) as i32;
                        let dot_py = vis_y + (dot_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 100, 240));
                        for dy in -3i32..=3 {
                            for dx in -3i32..=3 {
                                if dx * dx + dy * dy <= 9 {
                                    let _ = canvas.draw_point(sdl2::rect::Point::new(
                                        dot_px + dx,
                                        dot_py + dy,
                                    ));
                                }
                            }
                        }
                    }

                    // ── Meters on bottom third (IN, GR) ──
                    let meter_y = vis_y + curve_h + 3;
                    let meter_h = (vis_h - curve_h - 6).max(4);

                    let input_rms: f32 = state
                        .meters
                        .track_rms_pre_effect
                        .get(ti)
                        .copied()
                        .unwrap_or(0.0);
                    let in_db_live = if input_rms > 1e-6 {
                        20.0 * input_rms.log10()
                    } else {
                        -60.0_f32
                    };
                    // Use actual gain reduction from audio engine
                    let gr_db = state
                        .meters
                        .track_effect_gr
                        .get(ti)
                        .and_then(|v| v.get(slot_idx))
                        .copied()
                        .unwrap_or(0.0)
                        .min(0.0);

                    // Input level bar (horizontal)
                    let in_meter_w = (vis_w as i32 - 4) / 2 - 1;
                    let in_frac = ((in_db_live + 60.0) / 60.0).clamp(0.0, 1.0);
                    let in_fill_w = (in_frac * in_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        vis_x + 2,
                        meter_y,
                        in_meter_w as u32,
                        meter_h as u32,
                    ));
                    if in_fill_w > 0 {
                        let col = if in_frac > 0.85 {
                            sdl2::pixels::Color::RGBA(220, 60, 60, 230)
                        } else if in_frac > 0.6 {
                            sdl2::pixels::Color::RGBA(200, 180, 50, 230)
                        } else {
                            sdl2::pixels::Color::RGBA(60, 180, 80, 230)
                        };
                        canvas.set_draw_color(col);
                        let _ = canvas.fill_rect(Rect::new(
                            vis_x + 2,
                            meter_y,
                            in_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "IN",
                        vis_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(140, 200, 140, 160),
                    );

                    // GR bar (fills from right as GR increases)
                    let gr_meter_x = vis_x + 2 + in_meter_w + 2;
                    let gr_meter_w = vis_w as i32 - 4 - in_meter_w - 2;
                    let gr_frac = ((-gr_db) / 24.0).clamp(0.0, 1.0);
                    let gr_fill_w = (gr_frac * gr_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        gr_meter_x,
                        meter_y,
                        gr_meter_w as u32,
                        meter_h as u32,
                    ));
                    if gr_fill_w > 0 {
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 60, 220));
                        let _ = canvas.fill_rect(Rect::new(
                            gr_meter_x,
                            meter_y,
                            gr_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "GR",
                        gr_meter_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(180, 100, 100, 160),
                    );

                    // GR dB readout next to GR label
                    if gr_db < -0.1 {
                        let gr_text = format!("{:.1}dB", gr_db);
                        draw_pixel_label(
                            canvas,
                            &state.theme,
                            &gr_text,
                            gr_meter_x + 18,
                            meter_y,
                            gr_meter_w - 20,
                            sdl2::pixels::Color::RGBA(200, 100, 80, 200),
                        );
                    }

                    // Ceiling label on curve
                    let ceil_label = format!("C:{:.1}", ceiling_db);
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &ceil_label,
                        vis_x + vis_w as i32 - 36,
                        ceil_y - 10,
                        34,
                        sdl2::pixels::Color::RGBA(255, 100, 80, 180),
                    );
                }

                _ => {}
            }
        }

        sx += slot_w + slot_gap;
    }

    // "Add Slot" button at the end of the chain
    let add_btn_x = sx;
    let add_btn_y = top + 24 + slot_h / 2 - 12;
    let __auto_id_27 = input.next_id();
    let add_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_27,
            x: add_btn_x,
            y: add_btn_y,
            width: 24,
            height: 24,
            label: "+".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Open modules panel to drag modules here".into()),
            ..Default::default()
        },
    );
    if add_clicked {
        // Open the left panel if not open, switch to modules tab
        state.sample_browser_open = true;
        state.left_panel_tab = LeftPanelTab::Instruments;
    }

    // ── Drop zone for modules being dragged from the left panel ──
    let drop_zone_x = add_btn_x;
    let drop_zone_y = top + 24;
    let drop_zone_w = (w - drop_zone_x).max(80); // extend to fill remaining width
    let drop_zone_h = slot_h;
    let drop_hover = input.mouse_in_rect(drop_zone_x, drop_zone_y, drop_zone_w, drop_zone_h);

    // Show drop hint when dragging a module
    if state.module_drag.is_some() && drop_hover {
        // Hovering over empty drop zone — clear stale replace/insert indices
        // so the module is appended at the end, not swapped with an existing slot.
        state.module_drag_replace_idx = None;
        state.module_drag_insert_idx = None;

        canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 100, 80));
        let _ = canvas.fill_rect(Rect::new(
            drop_zone_x,
            drop_zone_y,
            drop_zone_w as u32,
            drop_zone_h as u32,
        ));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 100, 200));
        let _ = canvas.draw_rect(Rect::new(
            drop_zone_x,
            drop_zone_y,
            drop_zone_w as u32,
            drop_zone_h as u32,
        ));
    }

    // Handle module drop (on add-button or between slots)
    let drop_anywhere_in_rack = input.mouse_in_rect(0, top + 24, sx.max(add_btn_x + 80), slot_h);
    if input.mouse_released
        && (drop_hover || (state.module_drag.is_some() && drop_anywhere_in_rack))
    {
        if let Some(module_name) = state.module_drag.take() {
            // ── Module slot type validation ──
            let track_type = &state.project.tracks[ti].track_type;
            let is_midi_effect = crate::modules::is_midi_effect(&module_name);
            let is_instrument = crate::modules::is_instrument(&module_name);
            let _is_audio_effect = crate::modules::is_effect(&module_name);

            let (valid, reason) = match track_type {
                crate::app::models::TrackType::Midi => {
                    // MIDI tracks: signal flow is MIDI Effects → Instruments → Audio FX
                    // Determine where this module should go based on what's already in the rack
                    let rack = &state.project.tracks[ti].rack;
                    let insert_pos = state
                        .module_drag_replace_idx
                        .or(state.module_drag_insert_idx)
                        .unwrap_or(rack.len());

                    // Find the last MIDI effect index, last instrument index, first FX index
                    let mut last_midi_fx_idx: Option<usize> = None;
                    let mut first_instrument_idx: Option<usize> = None;
                    let mut last_instrument_idx: Option<usize> = None;
                    let mut first_fx_idx: Option<usize> = None;

                    for (ri, slot) in rack.iter().enumerate() {
                        if crate::modules::is_midi_effect(&slot.plugin_name) {
                            last_midi_fx_idx = Some(ri);
                        }
                        if crate::modules::is_instrument(&slot.plugin_name) {
                            if first_instrument_idx.is_none() {
                                first_instrument_idx = Some(ri);
                            }
                            last_instrument_idx = Some(ri);
                        }
                        if crate::modules::is_effect(&slot.plugin_name) && first_fx_idx.is_none() {
                            first_fx_idx = Some(ri);
                        }
                    }

                    if is_midi_effect {
                        // MIDI effects must come before instruments and audio FX
                        if let Some(fi) = first_instrument_idx {
                            if insert_pos > fi {
                                (
                                    false,
                                    format!(
                                        "{} is a MIDI effect — must be placed before instruments",
                                        module_name
                                    ),
                                )
                            } else {
                                (true, String::new())
                            }
                        } else if let Some(fi) = first_fx_idx {
                            if insert_pos > fi {
                                (
                                    false,
                                    format!(
                                        "{} is a MIDI effect — must be placed before audio effects",
                                        module_name
                                    ),
                                )
                            } else {
                                (true, String::new())
                            }
                        } else {
                            (true, String::new())
                        }
                    } else if is_instrument {
                        // Only one generator allowed per MIDI track.
                        // Allow replacing the existing generator (drop on top of it).
                        let already_has_generator = first_instrument_idx.is_some();
                        let replacing_generator = state
                            .module_drag_replace_idx
                            .map(|i| {
                                rack.get(i)
                                    .map(|s| crate::modules::is_instrument(&s.plugin_name))
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);
                        if already_has_generator && !replacing_generator {
                            (
                                false,
                                "A generator already exists — drop on top of it to replace"
                                    .to_string(),
                            )
                        } else
                        // Instruments must come after MIDI effects, before audio FX
                        if let Some(lm) = last_midi_fx_idx {
                            if insert_pos <= lm {
                                (
                                    false,
                                    format!(
                                        "{} is a generator — must be placed after MIDI effects",
                                        module_name
                                    ),
                                )
                            } else if let Some(fi) = first_fx_idx {
                                if insert_pos > fi {
                                    (false, format!("{} is a generator — must be placed before audio effects", module_name))
                                } else {
                                    (true, String::new())
                                }
                            } else {
                                (true, String::new())
                            }
                        } else if let Some(fi) = first_fx_idx {
                            if insert_pos > fi {
                                (
                                    false,
                                    format!(
                                        "{} is a generator — must be placed before audio effects",
                                        module_name
                                    ),
                                )
                            } else {
                                (true, String::new())
                            }
                        } else {
                            (true, String::new())
                        }
                    } else {
                        // Audio FX must come after instruments and MIDI effects
                        // Also requires at least one generator on a MIDI track
                        if first_instrument_idx.is_none() {
                            (
                                false,
                                format!(
                                    "{} is an audio effect — add a generator first",
                                    module_name
                                ),
                            )
                        } else if let Some(li) = last_instrument_idx {
                            if insert_pos <= li {
                                (
                                    false,
                                    format!(
                                        "{} is an audio effect — must be placed after generators",
                                        module_name
                                    ),
                                )
                            } else {
                                (true, String::new())
                            }
                        } else if let Some(lm) = last_midi_fx_idx {
                            if insert_pos <= lm {
                                (
                                    false,
                                    format!(
                                        "{} is an audio effect — must be placed after MIDI effects",
                                        module_name
                                    ),
                                )
                            } else {
                                (true, String::new())
                            }
                        } else {
                            (true, String::new())
                        }
                    }
                }
                crate::app::models::TrackType::Audio => {
                    if is_midi_effect {
                        (
                            false,
                            format!(
                                "{} is a MIDI effect — only works on MIDI tracks",
                                module_name
                            ),
                        )
                    } else if is_instrument {
                        (
                            false,
                            format!(
                                "{} is an instrument — only works on MIDI tracks",
                                module_name
                            ),
                        )
                    } else {
                        (true, String::new()) // audio effects are fine on audio tracks
                    }
                }
                crate::app::models::TrackType::Automation => {
                    (false, "Cannot add modules to automation tracks".to_string())
                }
            };

            if valid {
                let track_id = state.project.tracks[ti].id;
                let next_id = state.project.tracks[ti]
                    .rack
                    .iter()
                    .map(|s| s.slot_id)
                    .max()
                    .unwrap_or(0)
                    + 1;

                // Check for replacement (same-category drop onto existing slot)
                if let Some(replace_idx) = state.module_drag_replace_idx.take() {
                    if replace_idx < state.project.tracks[ti].rack.len() {
                        // Remove old slot, then add new one at same position
                        state.commands.execute(
                            Box::new(crate::app::commands::RackSlotRemove {
                                track_id,
                                slot_idx: replace_idx,
                                removed_slot: None,
                            }),
                            &mut state.project,
                        );
                        let slot = create_rack_slot_for_module(&module_name, next_id);
                        state.commands.execute(
                            Box::new(crate::app::commands::RackSlotAdd {
                                track_id,
                                slot,
                                insert_at: Some(replace_idx),
                            }),
                            &mut state.project,
                        );
                        state.dirty = true;
                        state.push_status(format!(
                            "Replaced slot {} with {}",
                            replace_idx + 1,
                            module_name
                        ));
                    }
                } else {
                    let insert_idx = state.module_drag_insert_idx.take();
                    let slot = create_rack_slot_for_module(&module_name, next_id);
                    state.commands.execute(
                        Box::new(crate::app::commands::RackSlotAdd {
                            track_id,
                            slot,
                            insert_at: insert_idx,
                        }),
                        &mut state.project,
                    );
                    state.dirty = true;
                    let pos_msg = if let Some(idx) = insert_idx {
                        format!("Added {} to rack at position {}", module_name, idx + 1)
                    } else {
                        format!("Added {} to rack", module_name)
                    };
                    state.push_status(pos_msg);
                }
            } else {
                state.module_drag_insert_idx = None;
                state.module_drag_replace_idx = None;
                state.push_status(reason);
            }
        }
        state.module_drag_insert_idx = None;
        state.module_drag_replace_idx = None;
    }

    // Decrement highlight timer
    if state.rack_highlight_timer > 0 {
        state.rack_highlight_timer -= 1;
        if state.rack_highlight_timer == 0 {
            state.rack_highlight_param = None;
        }
    }

    canvas.set_clip_rect(None);

    // ── Module reorder: handle drag-and-drop between slots ──
    if input.mouse_released {
        if let Some(src) = state.rack_reorder_drag.take() {
            if let Some(dst) = state.rack_reorder_target.take() {
                if src != dst && src < state.project.tracks[ti].rack.len() {
                    // Validate signal flow order after reorder
                    let module_name = state.project.tracks[ti].rack[src].plugin_name.clone();
                    let is_midi_track =
                        state.project.tracks[ti].track_type == crate::app::models::TrackType::Midi;
                    let insert_at = if dst > src {
                        (dst - 1).min(state.project.tracks[ti].rack.len())
                    } else {
                        dst.min(state.project.tracks[ti].rack.len())
                    };

                    if is_midi_track {
                        // Simulate the move: check if the resulting order is valid
                        let mut simulated = state.project.tracks[ti]
                            .rack
                            .iter()
                            .map(|s| s.plugin_name.clone())
                            .collect::<Vec<_>>();
                        let removed = simulated.remove(src);
                        let sim_insert = if dst > src {
                            (dst - 1).min(simulated.len())
                        } else {
                            dst.min(simulated.len())
                        };
                        simulated.insert(sim_insert, removed);

                        // Check order: MIDI effects, then instruments, then audio FX
                        let mut phase = 0u8; // 0=midi_fx, 1=instrument, 2=audio_fx
                        let mut valid = true;
                        let mut reason = String::new();
                        for name in &simulated {
                            let cat = if crate::modules::is_midi_effect(name) {
                                0u8
                            } else if crate::modules::is_instrument(name) {
                                1u8
                            } else {
                                2u8
                            };
                            if cat < phase {
                                valid = false;
                                reason = format!("Cannot move {} — would break signal flow (MIDI FX → Generators → Audio FX)", module_name);
                                break;
                            }
                            phase = cat;
                        }

                        if valid {
                            let slot = state.project.tracks[ti].rack.remove(src);
                            state.project.tracks[ti].rack.insert(sim_insert, slot);
                            state.dirty = true;
                            state.push_status("Module reordered".to_string());
                        } else {
                            state.push_status(reason);
                        }
                    } else {
                        // Non-MIDI tracks: just reorder
                        let slot = state.project.tracks[ti].rack.remove(src);
                        state.project.tracks[ti].rack.insert(insert_at, slot);
                        state.dirty = true;
                        state.push_status("Module reordered".to_string());
                    }
                }
            }
        }
        state.rack_reorder_drag = None;
        state.rack_reorder_target = None;
    }

    // ── Chunky horizontal scrollbar at bottom of rack ──
    // Add extra padding so the "+" button at the end of the last slot is always scrollable-to
    let total_content_w = sx + scroll_offset + 80; // +80px so the "+" button is reachable when rack is full
    let sb_y = top + h - scrollbar_h;
    if total_content_w > w {
        let visible_ratio = (w as f32 / total_content_w as f32).clamp(0.02, 1.0);
        let max_scroll = (total_content_w - w) as f32;
        let cur = (state.rack_scroll_x / max_scroll).clamp(0.0, 1.0);
        let new_s = scrollbar(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(84000),
            0,
            sb_y,
            w,
            scrollbar_h,
            ScrollbarDir::Horizontal,
            cur,
            visible_ratio,
        );
        state.rack_scroll_x = new_s * max_scroll;
    } else {
        state.rack_scroll_x = 0.0;
        // Draw empty scrollbar track
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 24, 200));
        let _ = canvas.fill_rect(Rect::new(0, sb_y, w as u32, scrollbar_h as u32));
    }

    // Scroll wheel in rack area
    if input.mouse_in_rect(0, top, w, h - scrollbar_h)
        && input.scroll_y != 0
        && !input.scroll_consumed
    {
        let delta = input.scroll_y as f32 * 40.0;
        let max_scroll = ((total_content_w - w) as f32).max(0.0);
        state.rack_scroll_x = (state.rack_scroll_x - delta).clamp(0.0, max_scroll);
    }

    // Middle-click drag: pan the instrument rack horizontally (same gesture as arranger)
    let rack_instr_drag_id = WidgetId::Auto(87001);
    if input.middle_mouse_down
        && input.mouse_in_rect(0, top, w, h - scrollbar_h)
        && input.middle_drag_widget == WidgetId::None
    {
        input.middle_drag_widget = rack_instr_drag_id;
    }
    if input.middle_mouse_down && input.middle_drag_widget == rack_instr_drag_id {
        let max_scroll = ((total_content_w - w) as f32).max(0.0);
        state.rack_scroll_x = (state.rack_scroll_x - input.mouse_dx as f32).clamp(0.0, max_scroll);
    }
    if state.sc_popup_open {
        let popup_ti = state.sc_popup_track_idx;
        let popup_si = state.sc_popup_slot_idx;
        let popup_x = state.sc_popup_x;
        let popup_y = state.sc_popup_y;

        // Build choices: None (Self) + all non-automation tracks except self
        let self_id = if popup_ti < state.project.tracks.len() {
            state.project.tracks[popup_ti].id
        } else {
            0
        };
        let cur_sc = if popup_ti < state.project.tracks.len()
            && popup_si < state.project.tracks[popup_ti].rack.len()
        {
            state.project.tracks[popup_ti].rack[popup_si].sidechain_track_id
        } else {
            None
        };

        let mut choices: Vec<(Option<u32>, String)> = vec![(None, "Self".to_string())];
        for t in &state.project.tracks {
            if t.id != self_id && t.track_type != crate::app::models::TrackType::Automation {
                let label = if t.name.is_empty() {
                    format!("Track {}", t.id)
                } else {
                    t.name.clone()
                };
                choices.push((Some(t.id), label));
            }
        }

        let item_h = 18i32;
        let popup_w = 140i32;
        let popup_h = choices.len() as i32 * item_h;

        // Shadow
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 120));
        let _ = canvas.fill_rect(Rect::new(
            popup_x + 2,
            popup_y + 2,
            popup_w as u32,
            popup_h as u32,
        ));
        // Background
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 32, 42, 250));
        let _ = canvas.fill_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));
        canvas.set_draw_color(Theme::c(state.theme.accent));
        let _ = canvas.draw_rect(Rect::new(popup_x, popup_y, popup_w as u32, popup_h as u32));

        let mut clicked_choice: Option<Option<u32>> = None;
        for (i, (sc_val, label)) in choices.iter().enumerate() {
            let iy = popup_y + i as i32 * item_h;
            let item_hover = input.mouse_in_rect(popup_x, iy, popup_w, item_h);
            let is_selected = *sc_val == cur_sc;

            if item_hover {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 75, 255));
                let _ = canvas.fill_rect(Rect::new(popup_x, iy, popup_w as u32, item_h as u32));
            }
            if is_selected {
                canvas.set_draw_color(Theme::c(state.theme.accent));
                let _ = canvas.fill_rect(Rect::new(popup_x, iy, 3, item_h as u32));
            }

            let text_col = if is_selected {
                Theme::c(state.theme.accent)
            } else {
                sdl2::pixels::Color::RGBA(210, 210, 210, 255)
            };
            draw_pixel_label(
                canvas,
                &state.theme,
                label,
                popup_x + 6,
                iy + 4,
                popup_w - 10,
                text_col,
            );

            if item_hover && input.mouse_pressed {
                clicked_choice = Some(*sc_val);
                input.consume();
            }
        }

        // Apply selection
        if let Some(new_sc) = clicked_choice {
            if new_sc != cur_sc
                && popup_ti < state.project.tracks.len()
                && popup_si < state.project.tracks[popup_ti].rack.len()
            {
                let track_id = state.project.tracks[popup_ti].id;
                state.commands.execute(
                    Box::new(crate::app::commands::SetRackSidechain {
                        track_id,
                        slot_idx: popup_si,
                        old_sc: cur_sc,
                        new_sc,
                    }),
                    &mut state.project,
                );
                state.dirty = true;
            }
            state.sc_popup_open = false;
        }

        // Click outside or Escape to close
        let over_popup = input.mouse_in_rect(popup_x, popup_y, popup_w, popup_h);
        if (input.mouse_pressed && !over_popup)
            || input
                .keys_pressed
                .contains(&sdl2::keyboard::Keycode::Escape)
        {
            state.sc_popup_open = false;
        }

        // Block mouse input from passing through the popup
        if over_popup {
            input.consume();
        }
    }
}

/// Draw the master output effects rack — uses the same visual style as the track rack.
pub(super) fn draw_master_rack(
    canvas: &mut Canvas<Window>,
    input: &mut crate::app::input::InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    use sdl2::rect::Rect;

    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // Reserve right side for the full master meter strip
    let rack_master_w = 220i32;
    let rack_w = w - rack_master_w - 4;

    draw_pixel_label(
        canvas,
        &state.theme,
        "MASTER RACK [Audio FX -> Out]",
        8,
        top + 6,
        rack_w - 16,
        Theme::c(state.theme.text_secondary),
    );

    let slot_count = state.project.master_rack.len();

    let scrollbar_h = 18i32;
    let natural_h = (h - 40 - scrollbar_h).max(60);
    let slot_h = natural_h.max(300); // tall enough for 4 rows of knobs (need ~266px)
    let slot_gap = 8i32;
    let scroll_offset = state.rack_scroll_x as i32;
    let mut sx = 10i32 - scroll_offset;
    let knob_cell_w = 80i32;

    canvas.set_clip_rect(Rect::new(0, top + 20, rack_w as u32, (slot_h + 24) as u32));

    for slot_idx in 0..slot_count {
        // Signal flow arrow
        if slot_idx > 0 {
            let arrow_x = sx - slot_gap / 2;
            let arrow_y = top + 24 + slot_h / 2;
            canvas.set_draw_color(Theme::c(state.theme.accent));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(arrow_x - 4, arrow_y),
                sdl2::rect::Point::new(arrow_x + 4, arrow_y),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(arrow_x + 2, arrow_y - 3),
                sdl2::rect::Point::new(arrow_x + 4, arrow_y),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(arrow_x + 2, arrow_y + 3),
                sdl2::rect::Point::new(arrow_x + 4, arrow_y),
            );
        }

        let sy = top + 24;
        let plugin_name = state.project.master_rack[slot_idx].plugin_name.clone();
        let param_count = state.project.master_rack[slot_idx].params.len();
        let max_rows = 4usize;
        let cols = if param_count == 0 {
            2
        } else {
            param_count.div_ceil(max_rows)
        }
        .max(2);
        let knob_cols_w = cols as i32 * knob_cell_w + 20;

        // Effect vis panel (same as track rack)
        let has_vis_panel = crate::modules::has_vis_panel(plugin_name.as_str());
        let vis_col_w = if has_vis_panel { 120i32 } else { 0i32 };
        let slot_w = (knob_cols_w + vis_col_w).max(160);

        let slot_enabled = state.project.master_rack[slot_idx].enabled;
        let bg = if slot_enabled {
            Theme::c(state.theme.panel_bg)
        } else {
            sdl2::pixels::Color::RGBA(
                state.theme.panel_bg[0].saturating_sub(20),
                state.theme.panel_bg[1].saturating_sub(20),
                state.theme.panel_bg[2].saturating_sub(20),
                200,
            )
        };
        canvas.set_draw_color(bg);
        let _ = canvas.fill_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));

        // Slot header
        draw_pixel_label(
            canvas,
            &state.theme,
            &plugin_name,
            sx + 8,
            sy + 6,
            slot_w - 40,
            if slot_enabled {
                Theme::c(state.theme.text_primary)
            } else {
                Theme::c(state.theme.text_dim)
            },
        );

        // Enable/disable toggle
        let toggle_id = input.next_id();
        let toggle_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: toggle_id,
                x: sx + slot_w - 58,
                y: sy + 3,
                width: 28,
                height: 14,
                label: if slot_enabled { "ON" } else { "OFF" }.into(),
                toggled: slot_enabled,
                icon: ButtonIcon::None,
                hint: Some("Toggle effect on/off".into()),

                ..Default::default()
            },
        );
        if toggle_clicked {
            let snapshot = state.project.clone();
            state.project.master_rack[slot_idx].enabled = !slot_enabled;
            state
                .commands
                .push_undo_snapshot(snapshot, "Toggle Master Effect");
            state.dirty = true;
        }

        // Delete button
        let del_id = input.next_id();
        let del_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: del_id,
                x: sx + slot_w - 26,
                y: sy + 3,
                width: 20,
                height: 14,
                label: "X".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Remove module".into()),
                ..Default::default()
            },
        );
        if del_clicked {
            let snapshot = state.project.clone();
            state.project.master_rack.remove(slot_idx);
            state
                .commands
                .push_undo_snapshot(snapshot, "Remove Master Effect");
            state.dirty = true;
            state.push_status("Master effect removed".to_string());
            break;
        }

        // ── Module panel drag: show drop zone indicators between modules ──
        if let Some(ref drag_name) = state.module_drag.clone() {
            let slot_center = sx + slot_w / 2;
            let in_slot = input.mouse_in_rect(sx - slot_gap / 2, sy, slot_w + slot_gap, slot_h);
            if in_slot {
                let drag_is_fx = crate::modules::is_effect(drag_name);
                let slot_is_fx =
                    crate::modules::is_effect(&state.project.master_rack[slot_idx].plugin_name);
                let same_category = drag_is_fx && slot_is_fx;
                let edge_frac = slot_w / 4;
                let mouse_in_center =
                    input.mouse_x >= sx + edge_frac && input.mouse_x <= sx + slot_w - edge_frac;

                if same_category && mouse_in_center {
                    state.module_drag_replace_idx = Some(slot_idx);
                    state.module_drag_insert_idx = None;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 180, 60, 80));
                    let _ = canvas.fill_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 180, 60, 220));
                    let _ = canvas.draw_rect(Rect::new(sx, sy, slot_w as u32, slot_h as u32));
                } else {
                    state.module_drag_replace_idx = None;
                    let insert_before = input.mouse_x < slot_center;
                    let target = if insert_before {
                        slot_idx
                    } else {
                        slot_idx + 1
                    };
                    state.module_drag_insert_idx = Some(target);
                    let ind_x = if insert_before {
                        sx - 2
                    } else {
                        sx + slot_w + 2
                    };
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 100, 220));
                    let _ = canvas.fill_rect(Rect::new(ind_x, sy, 3, slot_h as u32));
                }
            }
        }

        // Draw parameter knobs
        let knob_y_start = sy + 24;
        for pi in 0..param_count {
            let col = pi % cols;
            let row = pi / cols;
            let kx = sx + (knob_cell_w / 2) + col as i32 * knob_cell_w;
            let ky = knob_y_start + 20 + row as i32 * 64;
            if ky + 30 > sy + slot_h {
                break;
            }

            let param = &state.project.master_rack[slot_idx].params[pi];
            let knob_id = input.next_id();
            let mut val = param.value;
            let p_min = param.min;
            let p_max = param.max;
            let p_default = param.default;
            let p_name = param.name.clone();
            let p_id = param.id.clone();
            let is_bipolar = p_min < 0.0;

            let static_descs = crate::modules::get_param_descs(&plugin_name);
            let has_options = static_descs
                .iter()
                .find(|d| d.id == p_id)
                .and_then(|d| d.options);

            if let Some(opts) = has_options {
                // Render as draggable selector
                let idx = val.round() as usize;
                let label_text = if idx < opts.len() { opts[idx] } else { "?" };
                let sel_w = (knob_cell_w - 8).max(36);
                let sel_x = kx - sel_w / 2;
                let sel_y = ky - 8;
                let sel_h = 18;
                let hover = input.mouse_in_rect(sel_x, sel_y, sel_w, sel_h);
                let dragging = input.drag_widget == knob_id;
                canvas.set_draw_color(if dragging {
                    sdl2::pixels::Color::RGBA(70, 80, 110, 255)
                } else if hover {
                    sdl2::pixels::Color::RGBA(60, 65, 80, 255)
                } else {
                    sdl2::pixels::Color::RGBA(40, 44, 56, 255)
                });
                let _ = canvas.fill_rect(Rect::new(sel_x, sel_y, sel_w as u32, sel_h as u32));
                canvas.set_draw_color(Theme::c(state.theme.panel_border));
                let _ = canvas.draw_rect(Rect::new(sel_x, sel_y, sel_w as u32, sel_h as u32));
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &p_name,
                    sel_x,
                    sel_y - 12,
                    sel_w,
                    Theme::c(state.theme.text_dim),
                );
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    label_text,
                    sel_x + 4,
                    sel_y + 4,
                    sel_w - 8,
                    Theme::c(state.theme.text_primary),
                );
                if hover && input.mouse_pressed && !input.consumed {
                    input.drag_widget = knob_id;
                    input.consume();
                }
                if dragging && input.mouse_down {
                    let dy = -input.mouse_dy as f32;
                    let sensitivity = if input.shift() { 0.05 } else { 0.15 };
                    let delta = dy * sensitivity;
                    if delta.abs() > 0.001 {
                        let old_val = state.project.master_rack[slot_idx].params[pi].value;
                        let new_val = (old_val + delta).clamp(p_min, p_max);
                        let old_idx = old_val.round() as usize;
                        let new_idx = new_val.round() as usize;
                        state.project.master_rack[slot_idx].params[pi].value = new_val;
                        state.dirty = true;
                        if old_idx != new_idx {
                            state.commands.execute(
                                Box::new(crate::app::commands::SetMasterRackParam {
                                    slot_idx,
                                    param_idx: pi,
                                    old_value: old_val,
                                    new_value: new_val,
                                }),
                                &mut state.project,
                            );
                        }
                    }
                }
                if dragging && !input.mouse_down {
                    let v = state.project.master_rack[slot_idx].params[pi].value;
                    state.project.master_rack[slot_idx].params[pi].value = v.round();
                    input.drag_widget = WidgetId::None;
                }
                if hover
                    && input.mouse_released
                    && input.drag_widget == WidgetId::None
                    && !input.consumed
                {
                    let opts_len = opts.len();
                    let old_val = val;
                    let new_idx = if input.shift() {
                        if idx == 0 {
                            opts_len - 1
                        } else {
                            idx - 1
                        }
                    } else {
                        (idx + 1) % opts_len
                    };
                    val = new_idx as f32;
                    state.project.master_rack[slot_idx].params[pi].value = val;
                    state.dirty = true;
                    input.consume();
                    if (old_val - val).abs() > 1e-4 {
                        state.commands.execute(
                            Box::new(crate::app::commands::SetMasterRackParam {
                                slot_idx,
                                param_idx: pi,
                                old_value: old_val,
                                new_value: val,
                            }),
                            &mut state.project,
                        );
                    }
                }
            } else {
                let changed = crate::widgets::knob(
                    canvas,
                    input,
                    &state.theme,
                    &crate::widgets::KnobParams {
                        id: knob_id,
                        x: kx,
                        y: ky,
                        radius: 16,
                        min: p_min,
                        max: p_max,
                        sensitivity: 0.004,
                        label: Some(p_name.clone()),
                        bipolar: is_bipolar,
                        default_value: Some(p_default),
                        hint: Some(p_name.clone()),
                        snap_points: vec![],
                    },
                    &mut val,
                );
                if changed {
                    state.project.master_rack[slot_idx].params[pi].value = val;
                    state.dirty = true;
                }
                // Commit master rack param change on mouse release
                if input.mouse_released && input.drag_widget == knob_id {
                    let old_val = input.drag_start_value as f32;
                    if (old_val - val).abs() > 1e-4 {
                        state.commands.execute(
                            Box::new(crate::app::commands::SetMasterRackParam {
                                slot_idx,
                                param_idx: pi,
                                old_value: old_val,
                                new_value: val,
                            }),
                            &mut state.project,
                        );
                    }
                }
            }
        }

        // Effect visualisation panel (right side of slot)
        if has_vis_panel && slot_enabled {
            let vis_x = sx + knob_cols_w;
            let vis_y = sy + 24;
            let vis_w = (vis_col_w - 8) as u32;
            let vis_h = (slot_h - 32).max(20);
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 26, 220));
            let _ = canvas.fill_rect(Rect::new(vis_x, vis_y, vis_w, vis_h as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 50, 60, 150));
            let _ = canvas.draw_rect(Rect::new(vis_x, vis_y, vis_w, vis_h as u32));

            let params_snap: Vec<(String, f32)> = state.project.master_rack[slot_idx]
                .params
                .iter()
                .map(|p| (p.id.clone(), p.value))
                .collect();

            // ── NOTE: Master rack vis must stay in sync with track rack vis ──
            // When updating any effect visualization in draw_instrument_rack,
            // the same change MUST be applied here for the master rack.
            match plugin_name.as_str() {
                "LP Filter" | "HP Filter" => {
                    // SVF-model frequency response curve (synced with track rack vis)
                    let is_lp = plugin_name == "LP Filter";
                    let cutoff = params_snap
                        .iter()
                        .find(|(k, _)| k == "cutoff")
                        .map(|(_, v)| *v)
                        .unwrap_or(if is_lp { 1.0 } else { 0.0 });
                    let reso = params_snap
                        .iter()
                        .find(|(k, _)| k == "resonance")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);

                    let w_f = vis_w as f32;
                    let h_f = vis_h as f32;
                    let accent = Theme::c(state.theme.accent);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));

                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        let t = px as f32 / w_f;
                        let freq = 20.0_f32 * (20000.0_f32 / 20.0).powf(t);
                        let cutoff_hz = 20.0_f32 * (20000.0_f32 / 20.0_f32).powf(cutoff);
                        let ratio = freq / cutoff_hz;
                        let q = 0.5 + reso * 10.0;
                        let r2 = ratio * ratio;
                        let denom = ((1.0 - r2) * (1.0 - r2) + r2 / (q * q)).sqrt();
                        let mag = if is_lp {
                            (1.0 / denom).min(4.0)
                        } else {
                            (r2 / denom).min(4.0)
                        };
                        let db = 20.0 * mag.max(0.001).log10();
                        let y_norm = 0.5 - db / 40.0;
                        let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }
                    // 0dB reference line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 120));
                    let ref_y = vis_y + vis_h / 2;
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, ref_y),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, ref_y),
                    );
                }

                "Compressor" => {
                    // Compression curve + GR/IN meters (synced with track rack vis)
                    let threshold = params_snap
                        .iter()
                        .find(|(k, _)| k == "threshold")
                        .map(|(_, v)| *v)
                        .unwrap_or(-18.0);
                    let ratio = params_snap
                        .iter()
                        .find(|(k, _)| k == "ratio")
                        .map(|(_, v)| *v)
                        .unwrap_or(4.0);
                    let knee_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "knee")
                        .map(|(_, v)| *v)
                        .unwrap_or(6.0);
                    let accent = Theme::c(state.theme.accent);

                    // Curve shows -60..0 dBFS on both axes
                    let curve_h = (vis_h * 2 / 3).max(20);
                    let w_f = vis_w as f32;
                    let h_f = curve_h as f32;
                    let thresh_db = threshold;
                    let comp_ratio = ratio.max(1.0);

                    // Draw unity diagonal (faint)
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 100));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + curve_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
                    );

                    // Draw compression curve
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));
                    let slope = 1.0 - 1.0 / comp_ratio;
                    let half_knee = knee_db * 0.5;
                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        let in_db = -60.0 + (px as f32 / w_f) * 60.0;
                        let over = in_db - thresh_db;
                        let gr = if over <= -half_knee {
                            0.0_f32
                        } else if over >= half_knee {
                            -slope * over
                        } else {
                            let x = over + half_knee;
                            let t = x / knee_db.max(0.01);
                            -slope * knee_db * t * t * 0.5
                        };
                        let out_db = in_db + gr;
                        let y_norm = 1.0 - (out_db + 60.0) / 60.0;
                        let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }
                    // Threshold line
                    let thresh_x_norm = (thresh_db + 60.0) / 60.0;
                    let thresh_x = vis_x + (thresh_x_norm * w_f) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 80, 150));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(thresh_x, vis_y),
                        sdl2::rect::Point::new(thresh_x, vis_y + curve_h),
                    );

                    // ── Input + GR meters on bottom third (uses master bus meters) ──
                    let meter_y = vis_y + curve_h + 2;
                    let meter_h = (vis_h - curve_h - 4).max(4);
                    let track_rms_pre: f32 = state.meters.master_rms_pre;

                    let in_db_rms = if track_rms_pre > 1e-6 {
                        20.0 * track_rms_pre.log10()
                    } else {
                        -60.0_f32
                    };
                    // Use actual gain reduction from audio engine
                    let gr_db_live = state
                        .meters
                        .master_effect_gr
                        .get(slot_idx)
                        .copied()
                        .unwrap_or(0.0)
                        .min(0.0);

                    // ── Real-time dot on the compression curve ──
                    {
                        let dot_in_db = in_db_rms;
                        let dot_over = dot_in_db - thresh_db;
                        let dot_gr = if dot_over <= -half_knee {
                            0.0_f32
                        } else if dot_over >= half_knee {
                            -slope * dot_over
                        } else {
                            let x = dot_over + half_knee;
                            let t = x / knee_db.max(0.01);
                            -slope * knee_db * t * t * 0.5
                        };
                        let dot_out_db = dot_in_db + dot_gr;
                        let dot_x_norm = (dot_in_db + 60.0) / 60.0;
                        let dot_y_norm = 1.0 - (dot_out_db + 60.0) / 60.0;
                        let dot_px = vis_x + (dot_x_norm * w_f).clamp(0.0, w_f - 1.0) as i32;
                        let dot_py = vis_y + (dot_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 100, 240));
                        for dy in -3i32..=3 {
                            for dx in -3i32..=3 {
                                if dx * dx + dy * dy <= 9 {
                                    let _ = canvas.draw_point(sdl2::rect::Point::new(
                                        dot_px + dx,
                                        dot_py + dy,
                                    ));
                                }
                            }
                        }
                    }

                    // Draw input level bar
                    let in_meter_w = (vis_w as i32 - 4) / 2 - 1;
                    let in_frac = ((in_db_rms + 60.0) / 60.0).clamp(0.0, 1.0);
                    let in_fill_w = (in_frac * in_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        vis_x + 2,
                        meter_y,
                        in_meter_w as u32,
                        meter_h as u32,
                    ));
                    if in_fill_w > 0 {
                        let col = if in_frac > 0.85 {
                            sdl2::pixels::Color::RGBA(220, 60, 60, 230)
                        } else if in_frac > 0.6 {
                            sdl2::pixels::Color::RGBA(200, 180, 50, 230)
                        } else {
                            sdl2::pixels::Color::RGBA(60, 180, 80, 230)
                        };
                        canvas.set_draw_color(col);
                        let _ = canvas.fill_rect(Rect::new(
                            vis_x + 2,
                            meter_y,
                            in_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "IN",
                        vis_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(140, 200, 140, 160),
                    );

                    // Draw GR bar
                    let gr_meter_x = vis_x + 2 + in_meter_w + 2;
                    let gr_meter_w = vis_w as i32 - 4 - in_meter_w - 2;
                    let gr_frac = ((-gr_db_live) / 24.0).clamp(0.0, 1.0);
                    let gr_fill_w = (gr_frac * gr_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        gr_meter_x,
                        meter_y,
                        gr_meter_w as u32,
                        meter_h as u32,
                    ));
                    if gr_fill_w > 0 {
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 60, 220));
                        let _ = canvas.fill_rect(Rect::new(
                            gr_meter_x,
                            meter_y,
                            gr_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "GR",
                        gr_meter_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(180, 100, 100, 160),
                    );
                }
                "EQ" => {
                    let lo_gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "lo_gain")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(0.0);
                    let mid_gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "mid_gain")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(0.0);
                    let hi_gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "hi_gain")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(0.0);
                    let lo_freq = params_snap
                        .iter()
                        .find(|(k, _)| k == "lo_freq")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(200.0);
                    let mid_freq = params_snap
                        .iter()
                        .find(|(k, _)| k == "mid_freq")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(1000.0);
                    let hi_freq = params_snap
                        .iter()
                        .find(|(k, _)| k == "hi_freq")
                        .map(|(_, v)| *v as f64)
                        .unwrap_or(4000.0);
                    let sr = 44100.0_f64;
                    let lo_c = crate::modules::BiquadCoeffs::low_shelf(lo_freq, lo_gain_db, sr);
                    let mid_c =
                        crate::modules::BiquadCoeffs::peaking(mid_freq, mid_gain_db, 0.7, sr);
                    let hi_c = crate::modules::BiquadCoeffs::high_shelf(hi_freq, hi_gain_db, sr);

                    let db_range = 18.0_f64;
                    let log_min = (20.0_f64).ln();
                    let log_max = (20000.0_f64).ln();
                    let w_i = vis_w as i32;
                    let h_f = vis_h as f64;
                    let mid_line = vis_y + vis_h / 2;

                    let mut curve_py: Vec<i32> = Vec::with_capacity(w_i as usize);
                    for col in 0..w_i {
                        let t = col as f64 / (w_i - 1).max(1) as f64;
                        let freq = (log_min + t * (log_max - log_min)).exp();
                        let omega = 2.0 * std::f64::consts::PI * freq / sr;
                        let mag = lo_c.magnitude_at(omega)
                            * mid_c.magnitude_at(omega)
                            * hi_c.magnitude_at(omega);
                        let db = if mag > 1e-10 {
                            20.0 * mag.log10()
                        } else {
                            -db_range
                        };
                        let db_clamped = db.clamp(-db_range, db_range);
                        let py = mid_line - (db_clamped / db_range * (h_f / 2.0)) as i32;
                        curve_py.push(py.clamp(vis_y, vis_y + vis_h - 1));
                    }

                    // 0 dB reference line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 120));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, mid_line),
                        sdl2::rect::Point::new(vis_x + w_i, mid_line),
                    );

                    // Filled area
                    let accent = Theme::c(state.theme.accent);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 30,
                    ));
                    for col in 0..w_i {
                        let px = vis_x + col;
                        let py = curve_py[col as usize];
                        let (ya, yb) = if py <= mid_line {
                            (py, mid_line)
                        } else {
                            (mid_line, py)
                        };
                        let fill_h = (yb - ya + 1).max(1) as u32;
                        let _ = canvas.fill_rect(Rect::new(px, ya, 1, fill_h));
                    }

                    // Curve line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));
                    for col in 1..w_i {
                        let _ = canvas.draw_line(
                            sdl2::rect::Point::new(vis_x + col - 1, curve_py[(col - 1) as usize]),
                            sdl2::rect::Point::new(vis_x + col, curve_py[col as usize]),
                        );
                    }
                }
                "Distortion" => {
                    // Transfer curve (synced with track rack vis)
                    let drive = params_snap
                        .iter()
                        .find(|(k, _)| k == "drive")
                        .map(|(_, v)| *v)
                        .unwrap_or(1.0);
                    let accent = Theme::c(state.theme.accent);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 200,
                    ));
                    let w_f = vis_w as f32;
                    let h_f = vis_h as f32;
                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        let inp = (px as f32 / w_f) * 2.0 - 1.0;
                        let driven = inp * (1.0 + drive * 9.0);
                        let output = driven.tanh();
                        let y_norm = 0.5 - output * 0.45;
                        let y_px = vis_y + (y_norm * h_f) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 100));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + vis_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
                    );
                }

                "Delay" => {
                    // Stereo delay tap bars (synced with track rack vis)
                    let div_l = params_snap
                        .iter()
                        .find(|(k, _)| k == "time_l")
                        .map(|(_, v)| v.round() as usize)
                        .unwrap_or(5);
                    let div_r = params_snap
                        .iter()
                        .find(|(k, _)| k == "time_r")
                        .map(|(_, v)| v.round() as usize)
                        .unwrap_or(3);
                    let feedback = params_snap
                        .iter()
                        .find(|(k, _)| k == "feedback")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.3);
                    let mix = params_snap
                        .iter()
                        .find(|(k, _)| k == "mix")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.5);
                    let w_f = vis_w as f32;
                    let h_f = vis_h as f32;
                    let half_h = (h_f * 0.5) as i32;
                    let accent = Theme::c(state.theme.accent);
                    let beat_vals: [f32; 10] = [
                        4.0,
                        2.0,
                        4.0 / 3.0,
                        1.0,
                        2.0 / 3.0,
                        0.5,
                        1.0 / 3.0,
                        0.25,
                        0.5 / 3.0,
                        0.125,
                    ];
                    // L taps (top half)
                    {
                        let beats = beat_vals.get(div_l).copied().unwrap_or(0.5);
                        let tap_spacing =
                            ((beats / 4.0) * w_f * 0.9).max(4.0).min(w_f * 0.45) as i32;
                        let tap_w = (tap_spacing / 3).clamp(2, 12);
                        let mut tap_x = vis_x + 2;
                        let mut level = mix;
                        for _ in 0..12 {
                            if level < 0.02 || tap_x + tap_w > vis_x + vis_w as i32 - 1 {
                                break;
                            }
                            let bar_h = (level * half_h as f32 * 0.85) as i32;
                            let bar_y = vis_y + half_h - bar_h;
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                                accent.r,
                                accent.g,
                                accent.b,
                                (level * 220.0) as u8,
                            ));
                            let _ = canvas.fill_rect(Rect::new(
                                tap_x,
                                bar_y,
                                tap_w as u32,
                                bar_h as u32,
                            ));
                            tap_x += tap_spacing;
                            level *= feedback;
                        }
                    }
                    // R taps (bottom half)
                    {
                        let beats = beat_vals.get(div_r).copied().unwrap_or(1.0);
                        let tap_spacing =
                            ((beats / 4.0) * w_f * 0.9).max(4.0).min(w_f * 0.45) as i32;
                        let tap_w = (tap_spacing / 3).clamp(2, 12);
                        let mut tap_x = vis_x + 2;
                        let mut level = mix;
                        for _ in 0..12 {
                            if level < 0.02 || tap_x + tap_w > vis_x + vis_w as i32 - 1 {
                                break;
                            }
                            let bar_h = (level * half_h as f32 * 0.85) as i32;
                            let bar_y = vis_y + half_h;
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                                100,
                                accent.g,
                                accent.b,
                                (level * 200.0) as u8,
                            ));
                            let _ = canvas.fill_rect(Rect::new(
                                tap_x,
                                bar_y,
                                tap_w as u32,
                                bar_h as u32,
                            ));
                            tap_x += tap_spacing;
                            level *= feedback;
                        }
                    }
                    // Centre divider
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 80));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + half_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y + half_h),
                    );
                }

                "Limiter" => {
                    let gain_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "gain_db")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    let ceiling_db = params_snap
                        .iter()
                        .find(|(k, _)| k == "ceiling_db")
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    let accent = Theme::c(state.theme.accent);

                    // ── Transfer curve (upper 2/3) ──
                    let curve_h = (vis_h as f32 * 0.65) as i32;
                    let w_f = vis_w as f32;
                    let h_f = curve_h as f32;

                    // Unity reference line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 50, 60, 120));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, vis_y + curve_h),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
                    );

                    // Draw the limiter transfer curve
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r, accent.g, accent.b, 220,
                    ));
                    let mut prev_y_px = 0i32;
                    for px in 0..vis_w as i32 {
                        let in_db_c = -60.0 + (px as f32 / w_f) * 60.0;
                        let after_gain = in_db_c + gain_db;
                        let out_db_c = after_gain.min(ceiling_db);
                        let y_norm = 1.0 - (out_db_c + 60.0) / 60.0;
                        let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        if px > 0 {
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                                sdl2::rect::Point::new(vis_x + px, y_px),
                            );
                        }
                        prev_y_px = y_px;
                    }

                    // Ceiling reference line
                    let ceil_y_norm = 1.0 - (ceiling_db + 60.0) / 60.0;
                    let ceil_y = vis_y + (ceil_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 90, 70, 160));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x, ceil_y),
                        sdl2::rect::Point::new(vis_x + vis_w as i32, ceil_y),
                    );

                    // ── Real-time dot on the limiter transfer curve ──
                    {
                        let input_rms_dot = state.meters.master_rms_pre;
                        let dot_in_db = if input_rms_dot > 1e-6 {
                            20.0 * input_rms_dot.log10()
                        } else {
                            -60.0_f32
                        };
                        let dot_after_gain = dot_in_db + gain_db;
                        let dot_out_db = dot_after_gain.min(ceiling_db);
                        let dot_x_norm = (dot_in_db + 60.0) / 60.0;
                        let dot_y_norm = 1.0 - (dot_out_db + 60.0) / 60.0;
                        let dot_px = vis_x + (dot_x_norm * w_f).clamp(0.0, w_f - 1.0) as i32;
                        let dot_py = vis_y + (dot_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 100, 240));
                        for dy in -3i32..=3 {
                            for dx in -3i32..=3 {
                                if dx * dx + dy * dy <= 9 {
                                    let _ = canvas.draw_point(sdl2::rect::Point::new(
                                        dot_px + dx,
                                        dot_py + dy,
                                    ));
                                }
                            }
                        }
                    }

                    // ── Meters on bottom third (IN + GR) ──
                    let meter_y = vis_y + curve_h + 3;
                    let meter_h = (vis_h - curve_h - 6).max(4);

                    let input_rms = state.meters.master_rms_pre;
                    let in_db_live = if input_rms > 1e-6 {
                        20.0 * input_rms.log10()
                    } else {
                        -60.0_f32
                    };
                    let gr_db = state
                        .meters
                        .master_effect_gr
                        .get(slot_idx)
                        .copied()
                        .unwrap_or_else(|| {
                            let after = in_db_live + gain_db;
                            if after > ceiling_db {
                                ceiling_db - after
                            } else {
                                0.0
                            }
                        });

                    // Input level bar (horizontal)
                    let in_meter_w = (vis_w as i32 - 4) / 2 - 1;
                    let in_frac = ((in_db_live + 60.0) / 60.0).clamp(0.0, 1.0);
                    let in_fill_w = (in_frac * in_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        vis_x + 2,
                        meter_y,
                        in_meter_w as u32,
                        meter_h as u32,
                    ));
                    if in_fill_w > 0 {
                        let col = if in_frac > 0.85 {
                            sdl2::pixels::Color::RGBA(220, 60, 60, 230)
                        } else if in_frac > 0.6 {
                            sdl2::pixels::Color::RGBA(200, 180, 50, 230)
                        } else {
                            sdl2::pixels::Color::RGBA(60, 180, 80, 230)
                        };
                        canvas.set_draw_color(col);
                        let _ = canvas.fill_rect(Rect::new(
                            vis_x + 2,
                            meter_y,
                            in_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "IN",
                        vis_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(140, 200, 140, 160),
                    );

                    // GR bar (fills from left as GR increases)
                    let gr_meter_x = vis_x + 2 + in_meter_w + 2;
                    let gr_meter_w = vis_w as i32 - 4 - in_meter_w - 2;
                    let gr_frac = ((-gr_db) / 24.0).clamp(0.0, 1.0);
                    let gr_fill_w = (gr_frac * gr_meter_w as f32) as i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(
                        gr_meter_x,
                        meter_y,
                        gr_meter_w as u32,
                        meter_h as u32,
                    ));
                    if gr_fill_w > 0 {
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 60, 220));
                        let _ = canvas.fill_rect(Rect::new(
                            gr_meter_x,
                            meter_y,
                            gr_fill_w as u32,
                            meter_h as u32,
                        ));
                    }
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "GR",
                        gr_meter_x + 2,
                        meter_y,
                        20,
                        sdl2::pixels::Color::RGBA(180, 100, 100, 160),
                    );

                    if gr_db < -0.1 {
                        let gr_text = format!("{:.1}dB", gr_db);
                        draw_pixel_label(
                            canvas,
                            &state.theme,
                            &gr_text,
                            gr_meter_x + 18,
                            meter_y,
                            gr_meter_w - 20,
                            sdl2::pixels::Color::RGBA(200, 100, 80, 200),
                        );
                    }
                }
                _ => {}
            }
        }

        sx += slot_w + slot_gap;
    }

    canvas.set_clip_rect(None);

    // ── Drop zone for adding new effects ──
    let add_btn_x = sx.max(10);
    let drop_zone_x = add_btn_x;
    let drop_zone_y = top + 24;
    let drop_zone_w = (w - drop_zone_x).max(80);
    let drop_zone_h = slot_h;
    let drop_hover = input.mouse_in_rect(drop_zone_x, drop_zone_y, drop_zone_w, drop_zone_h);

    // Show drop hint when dragging a module
    if state.module_drag.is_some() && drop_hover {
        // Hovering over empty drop zone — clear stale replace/insert indices
        // so the module is appended at the end, not swapped with an existing slot.
        state.module_drag_replace_idx = None;
        state.module_drag_insert_idx = None;

        canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 100, 80));
        let _ = canvas.fill_rect(Rect::new(
            drop_zone_x,
            drop_zone_y,
            drop_zone_w as u32,
            drop_zone_h as u32,
        ));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 100, 200));
        let _ = canvas.draw_rect(Rect::new(
            drop_zone_x,
            drop_zone_y,
            drop_zone_w as u32,
            drop_zone_h as u32,
        ));
    }

    // ── Add button ──
    let add_btn_id = input.next_id();
    let add_btn_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: add_btn_id,
            x: add_btn_x,
            y: top + 24 + slot_h / 2 - 12,
            width: 24,
            height: 24,
            label: "+".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Add effect to master output chain".into()),
            ..Default::default()
        },
    );
    if add_btn_clicked {
        state.left_panel_tab = crate::app::state::LeftPanelTab::Instruments;
        if state.sample_browser_width < 10 {
            state.sample_browser_width = 220;
        }
        state.push_status("Drag an Audio FX from the MOD panel onto the master rack".to_string());
    }

    // Handle module drop onto master rack
    let drop_anywhere_in_rack = input.mouse_in_rect(0, top + 24, sx.max(add_btn_x + 80), slot_h);
    if input.mouse_released
        && (drop_hover || (state.module_drag.is_some() && drop_anywhere_in_rack))
    {
        if let Some(module_name) = state.module_drag.take() {
            if crate::modules::is_effect(&module_name) {
                let new_slot_id = state
                    .project
                    .master_rack
                    .iter()
                    .map(|s| s.slot_id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                let new_slot = create_rack_slot_for_module(&module_name, new_slot_id);
                let snapshot = state.project.clone();

                if let Some(replace_idx) = state.module_drag_replace_idx.take() {
                    if replace_idx < state.project.master_rack.len() {
                        state.project.master_rack.remove(replace_idx);
                        state.project.master_rack.insert(replace_idx, new_slot);
                        state
                            .commands
                            .push_undo_snapshot(snapshot, "Replace Master Effect");
                        state.push_status(format!(
                            "Replaced slot {} with {}",
                            replace_idx + 1,
                            module_name
                        ));
                    }
                } else {
                    let insert_idx = state.module_drag_insert_idx.take();
                    if let Some(idx) = insert_idx {
                        let idx = idx.min(state.project.master_rack.len());
                        state.project.master_rack.insert(idx, new_slot);
                        state
                            .commands
                            .push_undo_snapshot(snapshot, "Add Master Effect");
                        state.push_status(format!(
                            "Added {} to master rack at position {}",
                            module_name,
                            idx + 1
                        ));
                    } else {
                        state.project.master_rack.push(new_slot);
                        state
                            .commands
                            .push_undo_snapshot(snapshot, "Add Master Effect");
                        state.push_status(format!("Added {} to master rack", module_name));
                    }
                }
                state.dirty = true;
            } else {
                state.push_status(format!(
                    "{} — only audio effects can be added to the master rack",
                    module_name
                ));
            }
            state.module_drag_insert_idx = None;
            state.module_drag_replace_idx = None;
        }
    }
    state.module_drag_insert_idx = None;
    state.module_drag_replace_idx = None;

    // ── Scrollbar ──
    let total_content_w = sx + scroll_offset;
    let sb_y = top + h - scrollbar_h;
    if total_content_w > rack_w {
        let visible_ratio = (rack_w as f32 / total_content_w as f32).clamp(0.02, 1.0);
        let max_scroll = (total_content_w - rack_w) as f32;
        let cur = (state.rack_scroll_x / max_scroll).clamp(0.0, 1.0);
        let new_s = scrollbar(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(85100),
            0,
            sb_y,
            rack_w,
            scrollbar_h,
            ScrollbarDir::Horizontal,
            cur,
            visible_ratio,
        );
        state.rack_scroll_x = new_s * max_scroll;
    } else {
        state.rack_scroll_x = 0.0;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 24, 200));
        let _ = canvas.fill_rect(Rect::new(0, sb_y, rack_w as u32, scrollbar_h as u32));
    }
    if input.mouse_in_rect(0, top, rack_w, h - scrollbar_h)
        && input.scroll_y != 0
        && !input.scroll_consumed
    {
        let delta = input.scroll_y as f32 * 40.0;
        let max_scroll = ((total_content_w - rack_w) as f32).max(0.0);
        state.rack_scroll_x = (state.rack_scroll_x - delta).clamp(0.0, max_scroll);
    }

    // Middle-click drag: pan the master rack horizontally
    let rack_master_drag_id = WidgetId::Auto(87002);
    if input.middle_mouse_down
        && input.mouse_in_rect(0, top, rack_w, h - scrollbar_h)
        && input.middle_drag_widget == WidgetId::None
    {
        input.middle_drag_widget = rack_master_drag_id;
    }
    if input.middle_mouse_down && input.middle_drag_widget == rack_master_drag_id {
        let max_scroll = ((total_content_w - rack_w) as f32).max(0.0);
        state.rack_scroll_x = (state.rack_scroll_x - input.mouse_dx as f32).clamp(0.0, max_scroll);
    }

    // ══════════════════════════════════════════════════════════════════
    // ── MASTER METER STRIP (right side of rack view) ──
    // Full metering matching the mixer master strip: VU, fader, Pre/Out
    // meters, peak holds, clip LEDs, dB labels, info readout.
    // ══════════════════════════════════════════════════════════════════
    {
        let rmx = rack_w + 4;
        let rmy = top + 4;
        let rmh = (h - 8).max(4);

        // Background
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(24, 24, 32, 255));
        let _ = canvas.fill_rect(Rect::new(rmx, rmy, rack_master_w as u32, rmh as u32));

        // Accent cap
        canvas.set_draw_color(Theme::c(state.theme.accent));
        let _ = canvas.fill_rect(Rect::new(rmx, rmy, rack_master_w as u32, 4));

        // Label
        draw_pixel_label(
            canvas,
            &state.theme,
            "MASTER",
            rmx + 8,
            rmy + 7,
            rack_master_w - 16,
            sdl2::pixels::Color::RGBA(180, 200, 255, 240),
        );

        // ── VU meter (below label, full width) ──
        let rm_vu_h = 76i32;
        let rm_vu_y = rmy + 18;
        vu_meter(
            canvas,
            &state.theme,
            rmx + 6,
            rm_vu_y,
            rack_master_w - 12,
            rm_vu_h,
            state.meters.master_vu_needle,
            state.meters.master_vu_peak_needle,
        );

        // ── Master volume fader ──
        let rm_fader_top = rm_vu_y + rm_vu_h + 12;
        let rm_bottom_bar = 14i32;
        let rm_fader_h = (rmh - (rm_fader_top - rmy) - rm_bottom_bar - 4).max(20);
        let rm_fader_x = rmx + 10;
        let rm_fader_w = 20i32;

        // Fader groove
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(12, 12, 16, 255));
        let _ = canvas.fill_rect(Rect::new(
            rm_fader_x + 7,
            rm_fader_top,
            6,
            rm_fader_h as u32,
        ));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 50, 200));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(rm_fader_x + 13, rm_fader_top),
            sdl2::rect::Point::new(rm_fader_x + 13, rm_fader_top + rm_fader_h),
        );
        // dB tick marks
        for &db_mark in &[-48.0_f32, -36.0, -24.0, -12.0, -6.0, 0.0, 6.0] {
            let gain = if db_mark <= -60.0 {
                0.0
            } else {
                10.0_f32.powf(db_mark / 20.0)
            };
            let pos = vol_gain_to_pos(gain);
            let tick_y = rm_fader_top + rm_fader_h - (pos * rm_fader_h as f32) as i32;
            let tc = if db_mark >= 0.0 {
                sdl2::pixels::Color::RGBA(200, 80, 60, 140)
            } else {
                sdl2::pixels::Color::RGBA(70, 75, 85, 120)
            };
            canvas.set_draw_color(tc);
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(rm_fader_x, tick_y),
                sdl2::rect::Point::new(rm_fader_x + 4, tick_y),
            );
        }

        {
            let mut mvol_pos = vol_gain_to_pos(state.master_volume_ui);
            let mvol_id = WidgetId::Auto(85200);
            let mvol_changed = slider(
                canvas,
                input,
                &state.theme,
                &SliderParams {
                    id: mvol_id,
                    x: rm_fader_x,
                    y: rm_fader_top,
                    width: rm_fader_w,
                    height: rm_fader_h,
                    min: 0.0,
                    max: 1.0,
                    orientation: SliderOrientation::Vertical,
                    label: None,
                    default_value: Some(vol_gain_to_pos(1.0)),
                },
                &mut mvol_pos,
            );
            if mvol_changed {
                state.master_volume_ui = vol_pos_to_gain(mvol_pos);
            }
        }

        // ── Stereo meters: Pre (pre-effect) + Out (post-effect) pairs ──
        let rm_meter_x = rmx + 38;
        let rm_bar_w = 7u32;
        let rm_bar_gap = 2i32;
        let rm_pair_gap = 4i32;

        // Pre-effects pair
        let rm_rms_l = state.meters.master_rms_l;
        let rm_rms_r = state.meters.master_rms_r;
        draw_meter_bar(
            canvas,
            rm_rms_l,
            rm_meter_x,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );
        draw_meter_bar(
            canvas,
            rm_rms_r,
            rm_meter_x + rm_bar_w as i32 + rm_bar_gap,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );

        // Out (post-effects) pair — uses smoothed peak so the bar matches the limiter ceiling
        let rm_out_x = rm_meter_x + (rm_bar_w as i32) * 2 + rm_bar_gap + rm_pair_gap;
        let rm_out_l = state.meters.master_peak_smooth_post_l;
        let rm_out_r = state.meters.master_peak_smooth_post_r;
        draw_meter_bar(
            canvas,
            rm_out_l,
            rm_out_x,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );
        draw_meter_bar(
            canvas,
            rm_out_r,
            rm_out_x + rm_bar_w as i32 + rm_bar_gap,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );

        // Peak hold (pre pair)
        draw_peak_hold(
            canvas,
            state.meters.master_peak_hold_l,
            rm_meter_x,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );
        draw_peak_hold(
            canvas,
            state.meters.master_peak_hold_r,
            rm_meter_x + rm_bar_w as i32 + rm_bar_gap,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );

        // Peak hold (out pair)
        draw_peak_hold(
            canvas,
            state.meters.master_peak_hold_post_l,
            rm_out_x,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );
        draw_peak_hold(
            canvas,
            state.meters.master_peak_hold_post_r,
            rm_out_x + rm_bar_w as i32 + rm_bar_gap,
            rm_fader_top,
            rm_bar_w,
            rm_fader_h,
        );

        // dB labels to the right of out meters
        {
            let rm_labels_x = rm_out_x + rm_bar_w as i32 * 2 + rm_bar_gap + 2;
            let rm_label_max_w = rmx + rack_master_w - rm_labels_x - 4;
            if rm_label_max_w > 8 {
                draw_meter_db_labels(
                    canvas,
                    &state.theme,
                    rm_labels_x,
                    rm_fader_top,
                    rm_fader_h,
                    rm_label_max_w,
                );
            }
        }

        // Clip LEDs (on out pair)
        for (ch, (flag, mx_off)) in [
            (state.meters.master_clipping_l, (rm_out_x - rm_meter_x)),
            (
                state.meters.master_clipping_r,
                (rm_out_x - rm_meter_x) + rm_bar_w as i32 + rm_bar_gap,
            ),
        ]
        .iter()
        .enumerate()
        {
            let led_x = rm_meter_x + mx_off;
            let led_y = rm_fader_top - 7;
            if *flag {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 20, 20, 255));
                let _ = canvas.fill_rect(Rect::new(led_x, led_y, rm_bar_w, 4));
                if input.mouse_in_rect(led_x, led_y, rm_bar_w as i32, 4) && input.mouse_pressed {
                    if ch == 0 {
                        state.meters.master_clipping_l = false;
                    } else {
                        state.meters.master_clipping_r = false;
                    }
                }
            } else {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 50, 30, 160));
                let _ = canvas.fill_rect(Rect::new(led_x, led_y, rm_bar_w, 4));
            }
        }

        // Pre / Out labels below meter pairs
        draw_pixel_label(
            canvas,
            &state.theme,
            "Pre",
            rm_meter_x,
            rm_fader_top + rm_fader_h + 1,
            rm_bar_w as i32 * 2 + rm_bar_gap,
            sdl2::pixels::Color::RGBA(100, 110, 130, 150),
        );
        draw_pixel_label(
            canvas,
            &state.theme,
            "Out",
            rm_out_x,
            rm_fader_top + rm_fader_h + 1,
            rm_bar_w as i32 * 2 + rm_bar_gap,
            sdl2::pixels::Color::RGBA(140, 180, 120, 180),
        );

        // ── Info readout (right of meters) ──
        // Full mastering info matching the mixer master strip: Peak, RMS, Balance,
        // Crest, Oscilloscope, LUFS, Stereo correlation, DR bar, True-peak.
        let info_x = rm_out_x + (rm_bar_w as i32) * 2 + rm_bar_gap + 10 + 10;
        let info_w = rack_master_w - (info_x - rmx) - 8;
        if info_w > 20 {
            let peak = state.meters.master_peak_l.max(state.meters.master_peak_r);
            let rms = state.meters.master_rms;

            let peak_db_str = if peak > 1e-6 {
                format!("Peak {:.1}dB", 20.0 * peak.log10())
            } else {
                "Peak -∞".to_string()
            };
            draw_pixel_label(
                canvas,
                &state.theme,
                &peak_db_str,
                info_x,
                rm_fader_top + 2,
                info_w,
                sdl2::pixels::Color::RGBA(180, 190, 200, 220),
            );

            let rms_db_str = if rms > 1e-6 {
                format!("RMS {:.1}dB", 20.0 * rms.log10())
            } else {
                "RMS -∞".to_string()
            };
            draw_pixel_label(
                canvas,
                &state.theme,
                &rms_db_str,
                info_x,
                rm_fader_top + 16,
                info_w,
                sdl2::pixels::Color::RGBA(160, 170, 180, 200),
            );

            // Balance
            let bal = if (rm_rms_l + rm_rms_r) > 1e-6 {
                (rm_rms_r - rm_rms_l) / (rm_rms_l + rm_rms_r)
            } else {
                0.0
            };
            let bal_str = if bal.abs() < 0.05 {
                "Bal: C".to_string()
            } else if bal > 0.0 {
                format!("Bal: R{:.0}%", bal * 100.0)
            } else {
                format!("Bal: L{:.0}%", -bal * 100.0)
            };
            draw_pixel_label(
                canvas,
                &state.theme,
                &bal_str,
                info_x,
                rm_fader_top + 30,
                info_w,
                sdl2::pixels::Color::RGBA(140, 150, 160, 170),
            );

            // Crest
            if peak > 1e-6 && rms > 1e-6 {
                let crest = 20.0 * (peak / rms).log10();
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &format!("Crest {:.1}dB", crest),
                    info_x,
                    rm_fader_top + 44,
                    info_w,
                    sdl2::pixels::Color::RGBA(140, 150, 160, 170),
                );
            }

            // ── Mini oscilloscope ──
            let osc_y = rm_fader_top + 62;
            let osc_h = (rm_fader_h - 70).clamp(0, 60);
            let osc_w = info_w;
            if osc_h > 16 && osc_w > 20 {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(14, 16, 22, 240));
                let _ = canvas.fill_rect(Rect::new(info_x, osc_y, osc_w as u32, osc_h as u32));
                let osc_mid = osc_y + osc_h / 2;
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 100));
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(info_x, osc_mid),
                    sdl2::rect::Point::new(info_x + osc_w, osc_mid),
                );
                let osc_data = &state.meters.oscilloscope;
                if !osc_data.is_empty() {
                    let step = (osc_data.len() as f32 / osc_w as f32).max(1.0);
                    let mut prev_y = osc_mid;
                    for px in 0..osc_w {
                        let idx = (px as f32 * step) as usize;
                        let s = osc_data.get(idx).copied().unwrap_or(0.0);
                        let sy_sample = (osc_mid - (s * osc_h as f32 * 0.45) as i32)
                            .clamp(osc_y, osc_y + osc_h - 1);
                        if px > 0 {
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 220, 160, 200));
                            let _ = canvas.draw_line(
                                sdl2::rect::Point::new(info_x + px - 1, prev_y),
                                sdl2::rect::Point::new(info_x + px, sy_sample),
                            );
                        }
                        prev_y = sy_sample;
                    }
                }
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
                let _ = canvas.draw_rect(Rect::new(info_x, osc_y, osc_w as u32, osc_h as u32));
            }

            // ── Mastering section (below oscilloscope) ──
            let mas_y = osc_y + osc_h + 6;
            let mas_bot = rmy + rmh - 16;
            let mas_avail = (mas_bot - mas_y).max(0);
            let mas_x = info_x;
            let mas_w = info_w;
            if mas_avail >= 30 && mas_w > 20 {
                let mut cy = mas_y;

                // ── LUFS meters ──
                let lufs_m = state.meters.master_lufs_momentary;
                let lufs_st = state.meters.master_lufs_short;
                let lufs_m_str = if lufs_m < -60.0 || lufs_m == 0.0 {
                    "M: -∞ LUFS".to_string()
                } else {
                    format!("M: {:.1} LUFS", lufs_m)
                };
                let lufs_col = if lufs_m > -6.0 {
                    sdl2::pixels::Color::RGBA(230, 80, 60, 220)
                } else if lufs_m > -14.0 {
                    sdl2::pixels::Color::RGBA(220, 190, 60, 200)
                } else {
                    sdl2::pixels::Color::RGBA(120, 200, 150, 190)
                };
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &lufs_m_str,
                    mas_x,
                    cy,
                    mas_w,
                    lufs_col,
                );
                cy += 11;
                let lufs_st_str = if lufs_st < -60.0 || lufs_st == 0.0 {
                    "S: -∞ LUFS".to_string()
                } else {
                    format!("S: {:.1} LUFS", lufs_st)
                };
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &lufs_st_str,
                    mas_x,
                    cy,
                    mas_w,
                    sdl2::pixels::Color::RGBA(100, 170, 130, 170),
                );
                cy += 13;

                // ── Stereo correlation bar ──
                let corr = state.meters.master_correlation.clamp(-1.0, 1.0);
                if cy + 13 < mas_bot {
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "Phase",
                        mas_x,
                        cy,
                        mas_w,
                        sdl2::pixels::Color::RGBA(110, 120, 140, 160),
                    );
                    cy += 9;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(22, 24, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                    let mid_x = mas_x + mas_w / 2;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(70, 75, 85, 180));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(mid_x, cy),
                        sdl2::rect::Point::new(mid_x, cy + 5),
                    );
                    let fill_frac = (corr + 1.0) / 2.0;
                    let fill_w = (fill_frac * mas_w as f32) as i32;
                    let bar_col = if corr < 0.0 {
                        sdl2::pixels::Color::RGBA(200, 70, 50, 200)
                    } else if corr < 0.5 {
                        sdl2::pixels::Color::RGBA(200, 180, 50, 200)
                    } else {
                        sdl2::pixels::Color::RGBA(60, 190, 110, 200)
                    };
                    canvas.set_draw_color(bar_col);
                    let _ = canvas.fill_rect(Rect::new(mas_x, cy, fill_w.max(1) as u32, 6));
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
                    let _ = canvas.draw_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                    let corr_str = format!("{:+.2}", corr);
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &corr_str,
                        mas_x + mas_w - 26,
                        cy - 1,
                        26,
                        sdl2::pixels::Color::RGBA(130, 140, 155, 160),
                    );
                    cy += 10;
                }

                // ── Dynamic range (crest factor) bar ──
                if cy + 16 < mas_bot {
                    let peak2 = state.meters.master_peak_l.max(state.meters.master_peak_r);
                    let rms2 = state.meters.master_rms;
                    let dr_db = if peak2 > 1e-6 && rms2 > 1e-6 {
                        (20.0 * (peak2 / rms2).log10()).clamp(0.0, 30.0)
                    } else {
                        0.0_f32
                    };
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        "DR",
                        mas_x,
                        cy,
                        mas_w,
                        sdl2::pixels::Color::RGBA(110, 120, 140, 160),
                    );
                    cy += 9;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(22, 24, 30, 220));
                    let _ = canvas.fill_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                    let dr_frac = (dr_db / 30.0).clamp(0.0, 1.0);
                    let dr_fill = (dr_frac * mas_w as f32) as i32;
                    let dr_col = if dr_db < 6.0 {
                        sdl2::pixels::Color::RGBA(200, 60, 50, 200)
                    } else if dr_db < 12.0 {
                        sdl2::pixels::Color::RGBA(200, 175, 50, 200)
                    } else {
                        sdl2::pixels::Color::RGBA(60, 180, 100, 200)
                    };
                    canvas.set_draw_color(dr_col);
                    let _ = canvas.fill_rect(Rect::new(mas_x, cy, dr_fill.max(1) as u32, 6));
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
                    let _ = canvas.draw_rect(Rect::new(mas_x, cy, mas_w as u32, 6));
                    let dr_str = format!("{:.0}dB", dr_db);
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &dr_str,
                        mas_x + mas_w - 22,
                        cy - 1,
                        22,
                        sdl2::pixels::Color::RGBA(130, 140, 155, 160),
                    );
                    cy += 10;
                }

                // ── True peak indicator ──
                if cy + 10 < mas_bot {
                    let tp = state.meters.master_peak_l.max(state.meters.master_peak_r);
                    let tp_db = if tp > 1e-6 {
                        20.0 * tp.log10()
                    } else {
                        -60.0_f32
                    };
                    let tp_col = if tp_db > -0.1 {
                        sdl2::pixels::Color::RGBA(235, 55, 40, 240)
                    } else if tp_db > -3.0 {
                        sdl2::pixels::Color::RGBA(220, 185, 50, 210)
                    } else {
                        sdl2::pixels::Color::RGBA(100, 170, 130, 180)
                    };
                    let tp_str = if tp > 1e-6 {
                        format!("TP: {:.1}dBFS", tp_db)
                    } else {
                        "TP: -∞".to_string()
                    };
                    draw_pixel_label(canvas, &state.theme, &tp_str, mas_x, cy, mas_w, tp_col);
                }
            }
        }

        // Border
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(70, 80, 120, 200));
        let _ = canvas.draw_rect(Rect::new(rmx, rmy, rack_master_w as u32, rmh as u32));
    }
}
// ── Sample Browser (left panel) ─────────────────────────────────────
