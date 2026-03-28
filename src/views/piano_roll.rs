// Eden DAW — Views: piano_roll

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::state::*;
use crate::theme::Theme;
use crate::widgets::*;

pub(super) fn draw_piano_roll(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    draw_piano_roll_at(canvas, input, state, 0, top, w, h);
}

pub(super) fn draw_piano_roll_at(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    left: i32,
    top: i32,
    w: i32,
    h: i32,
) {
    // Offset mouse coords so the piano roll's internal layout (which starts at x=0)
    // sees coordinates relative to its own left edge.
    let orig_mouse_x = input.mouse_x;
    let orig_drag_start_x = input.drag_start_x;
    input.mouse_x -= left;
    input.drag_start_x -= left;

    let orig_viewport = canvas.viewport();
    canvas.set_viewport(Rect::new(left, 0, w as u32, (top + h) as u32));

    draw_piano_roll_impl(canvas, input, state, top, w, h);

    canvas.set_viewport(orig_viewport);
    input.mouse_x = orig_mouse_x;
    input.drag_start_x = orig_drag_start_x;
}

pub(super) fn draw_piano_roll_impl(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    // ── Layout ────────────────────────────────────────────────────────
    const KEY_W: i32 = 64; // piano keyboard column width
    const TOOLBAR_H: i32 = 22; // piano roll toolbar
    const RULER_H: i32 = 20; // beat/bar ruler at top
    const NOTE_H: i32 = 12; // height of each semitone row
    const SCROLL_T: i32 = 14; // scrollbar thickness
    const TOTAL: i32 = 128; // MIDI pitches
    let vel_h: i32 = if state.velocity_editor_visible { 68 } else { 0 };

    // Loop length is always clip length (Loop: Clip bar removed)

    // Derived regions
    let toolbar_top = top;
    let ruler_top = toolbar_top + TOOLBAR_H; // bar/beat ruler
    let grid_top = ruler_top + RULER_H; // note grid top
    let hscroll_y = top + h - SCROLL_T - vel_h - SCROLL_T; // horizontal scrollbar
    let vel_top = hscroll_y + SCROLL_T; // velocity lane
    let vscroll_x = w - SCROLL_T; // vertical scrollbar column
    let grid_h = hscroll_y - grid_top; // height of note grid area
    let grid_w = vscroll_x - KEY_W; // width of note grid area

    // ── Collect clip info (immutable borrow done up-front) ────────────
    let clip_info: Option<(u32, usize, f64, f64)> = state.selected_clip.and_then(|(tid, ci)| {
        state
            .project
            .tracks
            .iter()
            .find(|t| t.id == tid)
            .and_then(|t| {
                if let Some(crate::models::Clip::Midi(m)) = t.clips.get(ci) {
                    Some((tid, ci, m.start_time, m.length))
                } else {
                    None
                }
            })
    });

    // ── Background ────────────────────────────────────────────────────
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 18, 22, 255));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // ── Toolbar ───────────────────────────────────────────────────────
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(32, 34, 40, 255));
    let _ = canvas.fill_rect(Rect::new(0, toolbar_top, w as u32, TOOLBAR_H as u32));
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, toolbar_top + TOOLBAR_H - 1),
        sdl2::rect::Point::new(w, toolbar_top + TOOLBAR_H - 1),
    );

    // ── Toolbar buttons (left side) ──
    let tb_y = toolbar_top + 2;
    let tb_h = TOOLBAR_H - 4;
    let mut tb_x = 4i32;

    // Clip/Track info label
    if let Some((tid, ci, _, clip_len_info)) = clip_info {
        let track_name = state
            .project
            .tracks
            .iter()
            .find(|t| t.id == tid)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let clip_name = state
            .project
            .tracks
            .iter()
            .find(|t| t.id == tid)
            .and_then(|t| t.clips.get(ci))
            .map(|c| c.name().to_string())
            .unwrap_or_default();
        let info = format!(
            "{} / {} ({:.0}b)",
            track_name,
            clip_name,
            clip_len_info / 4.0
        );
        let info_w = info.len() as i32 * 8 + 8;
        draw_pixel_label(
            canvas,
            &state.theme,
            &info,
            tb_x,
            tb_y + 4,
            info_w,
            Theme::c(state.theme.text_dim),
        );
        tb_x += info_w + 8;
    }

    // Separator
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(tb_x, toolbar_top + 3),
        sdl2::rect::Point::new(tb_x, toolbar_top + TOOLBAR_H - 4),
    );
    tb_x += 6;

    // Mode indicator: Ctrl = SELECT, default = DRAW (only when piano roll is focused)
    {
        let is_select =
            input.ctrl() && state.focused_panel == crate::state::FocusedPanel::PianoRoll;
        let mode_label = if is_select { "SELECT" } else { "DRAW" };
        draw_pixel_label(
            canvas,
            &state.theme,
            mode_label,
            tb_x,
            tb_y + 4,
            48,
            if is_select {
                sdl2::pixels::Color::RGBA(120, 180, 255, 220)
            } else {
                sdl2::pixels::Color::RGBA(255, 180, 80, 220)
            },
        );
        tb_x += 52;
    }

    // Separator
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(tb_x, toolbar_top + 3),
        sdl2::rect::Point::new(tb_x, toolbar_top + TOOLBAR_H - 4),
    );
    tb_x += 6;

    // Snap grid dropdown button
    {
        // Snap on/off toggle for piano roll
        let snap_toggle_id = input.next_id();
        let snap_tog = toggle_button(
            canvas,
            input,
            &state.theme,
            tb_x,
            tb_y,
            tb_h,
            state.theme.accent,
            state.piano_roll_snap_enabled,
            snap_toggle_id,
            "S",
            Some("Toggle piano roll snap"),
        );
        if snap_tog {
            state.piano_roll_snap_enabled = !state.piano_roll_snap_enabled;
        }
        tb_x += tb_h + 4;

        let pr_snap_label = SNAP_RESOLUTIONS[state.piano_roll_snap_idx].0;
        let snap_btn = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(80022),
                x: tb_x,
                y: tb_y,
                width: 58,
                height: tb_h,
                label: format!("G:{}", pr_snap_label),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Cycle snap grid resolution".into()),

                ..Default::default()
            },
        );
        if snap_btn {
            state.piano_roll_snap_idx = (state.piano_roll_snap_idx + 1) % SNAP_RESOLUTIONS.len();
        }
        tb_x += 62;
    }

    // VEL toggle button
    {
        let vel_btn = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(80023),
                x: tb_x,
                y: tb_y,
                width: 32,
                height: tb_h,
                label: "VEL".to_string(),
                toggled: state.velocity_editor_visible,
                icon: ButtonIcon::None,
                hint: Some("Toggle velocity editor".into()),
                ..Default::default()
            },
        );
        if vel_btn {
            state.velocity_editor_visible = !state.velocity_editor_visible;
        }
        tb_x += 36;
    }

    // Select All button
    {
        let sel_all_btn = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(80024),
                x: tb_x,
                y: tb_y,
                width: 32,
                height: tb_h,
                label: "ALL".to_string(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Select all notes (Ctrl+A)".into()),
                ..Default::default()
            },
        );
        if sel_all_btn {
            // Select all notes in clip
            if let Some((tid, ci, _, _)) = clip_info {
                if let Some(track) = state.project.tracks.iter().find(|t| t.id == tid) {
                    if let Some(crate::models::Clip::Midi(m)) = track.clips.get(ci) {
                        state.piano_roll_selected_notes = (0..m.notes.len()).collect();
                    }
                }
            }
        }
        tb_x += 36;
    }

    // Deselect button
    {
        let desel_btn = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(80025),
                x: tb_x,
                y: tb_y,
                width: 18,
                height: tb_h,
                label: "0".to_string(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Deselect all notes".into()),
                ..Default::default()
            },
        );
        if desel_btn {
            state.piano_roll_selected_notes.clear();
        }
    }

    // MIDI Export button
    {
        tb_x += 22;
        // Separator
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(tb_x, toolbar_top + 3),
            sdl2::rect::Point::new(tb_x, toolbar_top + TOOLBAR_H - 4),
        );
        tb_x += 6;

        let midi_exp_btn = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: WidgetId::Auto(80026),
                x: tb_x,
                y: tb_y,
                width: 36,
                height: tb_h,
                label: "MID".to_string(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Export clip as MIDI file (.mid)".into()),
                ..Default::default()
            },
        );
        if midi_exp_btn {
            if let Some((tid, ci, _, _)) = clip_info {
                if let Some(track) = state.project.tracks.iter().find(|t| t.id == tid) {
                    if let Some(crate::models::Clip::Midi(m)) = track.clips.get(ci) {
                        let clip_name = if m.name.is_empty() {
                            format!("clip_{}", ci)
                        } else {
                            m.name.clone()
                        };
                        state.midi_export_name = format!("{}.mid", clip_name);
                        // Default export directory: project file dir or home
                        state.midi_export_dir = if let Some(ref p) = state.last_save_path {
                            std::path::Path::new(p)
                                .parent()
                                .map(|d| d.to_string_lossy().to_string())
                                .unwrap_or_else(|| ".".to_string())
                        } else {
                            std::env::current_dir()
                                .map(|d| d.to_string_lossy().to_string())
                                .unwrap_or_else(|_| ".".to_string())
                        };
                        state.midi_export_popup_open = true;
                    }
                }
            }
        }
        tb_x += 40;
        let _ = tb_x; // suppress unused-assignment warning
    }

    // ── No clip selected message ──────────────────────────────────────
    if clip_info.is_none() {
        draw_pixel_label(
            canvas,
            &state.theme,
            "Double-click a MIDI clip to edit it here",
            KEY_W + 20,
            top + h / 2 - 4,
            w - KEY_W - 40,
            sdl2::pixels::Color::RGBA(90, 90, 110, 200),
        );
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(KEY_W - 1, top),
            sdl2::rect::Point::new(KEY_W - 1, top + h),
        );
        return;
    }
    let (track_id, clip_idx, clip_start, clip_len) = clip_info.unwrap();

    // ── Read piano roll view state ────────────────────────────────────
    let zoom = state.piano_roll_zoom_x;
    let scroll_x = state.piano_roll_scroll_x; // in beats
    let scroll_y = state.piano_roll_scroll_y; // top visible semitone (descending: 127 = top)
    let snap_beats = SNAP_RESOLUTIONS[state.piano_roll_snap_idx].1;

    // Snap helper (piano roll has its own grid)
    let pr_snap = |beat: f64| -> f64 {
        if state.piano_roll_snap_enabled {
            (beat / snap_beats).round() * snap_beats
        } else {
            beat
        }
    };
    let pr_snap_prox = |beat: f64| -> f64 {
        if !state.piano_roll_snap_enabled {
            return beat;
        }
        let nearest = (beat / snap_beats).round() * snap_beats;
        if (beat - nearest).abs() <= snap_beats * 0.35 {
            nearest
        } else {
            beat
        }
    };

    // Beat → screen X
    let beat_to_x = |beat: f64| -> i32 { KEY_W + ((beat - scroll_x) * zoom) as i32 };
    // Screen X → beat
    let x_to_beat = |x: i32| -> f64 { scroll_x + (x - KEY_W) as f64 / zoom };
    // Pitch → screen Y (descending: higher pitch = lower Y)
    let pitch_to_y = |pitch: i32| -> i32 { grid_top + ((TOTAL - 1 - pitch) - scroll_y) * NOTE_H };
    // Screen Y → pitch
    let y_to_pitch =
        |y: i32| -> i32 { ((TOTAL - 1) - scroll_y - (y - grid_top) / NOTE_H).clamp(0, TOTAL - 1) };

    // ── Clip "measure" indicator — where the clip length boundary is ──
    let clip_end_x = beat_to_x(clip_len);
    let clip_start_x = beat_to_x(0.0);

    // ── Clip shading: content region ─────────────────────────────────
    canvas.set_clip_rect(Rect::new(KEY_W, grid_top, grid_w as u32, grid_h as u32));
    {
        let sx = clip_start_x.max(KEY_W);
        let ex = clip_end_x.min(vscroll_x);
        if ex > sx {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 6));
            let _ = canvas.fill_rect(Rect::new(sx, grid_top, (ex - sx) as u32, grid_h as u32));
        }
        // End boundary line
        if clip_end_x >= KEY_W && clip_end_x <= vscroll_x {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, 150));
            let _ = canvas.fill_rect(Rect::new(clip_end_x - 1, grid_top, 2, grid_h as u32));
        }
        // Darken area beyond clip end to show it's outside the clip region
        if clip_end_x < vscroll_x {
            let dark_x = clip_end_x.max(KEY_W);
            let dark_w = (vscroll_x - dark_x).max(0);
            if dark_w > 0 {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 80));
                let _ = canvas.fill_rect(Rect::new(dark_x, grid_top, dark_w as u32, grid_h as u32));
            }
        }
    }

    // ── Row backgrounds + piano keys ─────────────────────────────────
    let visible_rows = grid_h / NOTE_H + 2;
    for i in 0..visible_rows {
        let pitch = (TOTAL - 1) - scroll_y - i;
        if !(0..TOTAL).contains(&pitch) {
            continue;
        }
        let ny = pitch_to_y(pitch);
        let is_black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
        let is_kbd_held =
            state.piano_keyboard_mode && state.piano_keyboard_held.contains(&(pitch as u8));

        // Row background
        let row_col = if is_kbd_held {
            sdl2::pixels::Color::RGBA(60, 120, 180, 255) // keyboard-played note highlight
        } else if is_black {
            sdl2::pixels::Color::RGBA(24, 24, 30, 255)
        } else if pitch % 12 == 0 {
            sdl2::pixels::Color::RGBA(32, 38, 42, 255) // C notes slightly highlighted
        } else {
            sdl2::pixels::Color::RGBA(30, 30, 36, 255)
        };
        canvas.set_draw_color(row_col);
        let _ = canvas.fill_rect(Rect::new(KEY_W, ny, grid_w as u32, NOTE_H as u32));

        // Row separator
        canvas.set_draw_color(if pitch % 12 == 0 {
            sdl2::pixels::Color::RGBA(60, 70, 80, 180)
        } else {
            sdl2::pixels::Color::RGBA(40, 40, 48, 140)
        });
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(KEY_W, ny + NOTE_H - 1),
            sdl2::rect::Point::new(vscroll_x, ny + NOTE_H - 1),
        );
    }
    canvas.set_clip_rect(None);

    // Piano keys (drawn separately, left column, no clip rect needed)
    {
        canvas.set_clip_rect(Rect::new(0, grid_top, KEY_W as u32, grid_h as u32));
        // Dark key column bg
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(22, 22, 26, 255));
        let _ = canvas.fill_rect(Rect::new(0, grid_top, KEY_W as u32, grid_h as u32));

        for i in 0..visible_rows {
            let pitch = (TOTAL - 1) - scroll_y - i;
            if !(0..TOTAL).contains(&pitch) {
                continue;
            }
            let ny = pitch_to_y(pitch);
            let is_black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
            let is_kbd_held =
                state.piano_keyboard_mode && state.piano_keyboard_held.contains(&(pitch as u8));

            // Note name lookup
            let note_names = [
                "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
            ];
            let note_idx = (pitch % 12) as usize;
            let octave = pitch / 12 - 1;
            let note_label = format!("{}{}", note_names[note_idx], octave);

            if is_black {
                let key_col = if is_kbd_held {
                    sdl2::pixels::Color::RGBA(60, 140, 220, 255)
                } else {
                    sdl2::pixels::Color::RGBA(30, 30, 35, 255)
                };
                canvas.set_draw_color(key_col);
                let _ = canvas.fill_rect(Rect::new(
                    2,
                    ny + 1,
                    (KEY_W * 2 / 3 - 4) as u32,
                    (NOTE_H - 2) as u32,
                ));
                // Note name on black key
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &note_label,
                    4,
                    ny + 2,
                    KEY_W * 2 / 3 - 8,
                    sdl2::pixels::Color::RGBA(140, 140, 155, 180),
                );
            } else {
                let key_col = if is_kbd_held {
                    sdl2::pixels::Color::RGBA(80, 160, 240, 255)
                } else {
                    sdl2::pixels::Color::RGBA(200, 200, 210, 230)
                };
                canvas.set_draw_color(key_col);
                let _ = canvas.fill_rect(Rect::new(
                    2,
                    ny + 1,
                    (KEY_W - 6) as u32,
                    (NOTE_H - 2) as u32,
                ));
                // Note name on white key (C notes highlighted)
                let text_col = if is_kbd_held {
                    sdl2::pixels::Color::RGBA(255, 255, 255, 230)
                } else if pitch % 12 == 0 {
                    sdl2::pixels::Color::RGBA(40, 60, 80, 220)
                } else {
                    sdl2::pixels::Color::RGBA(60, 60, 75, 190)
                };
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &note_label,
                    4,
                    ny + 2,
                    KEY_W - 8,
                    text_col,
                );
                // Right edge line between white keys
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(120, 120, 130, 180));
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(2, ny + NOTE_H - 1),
                    sdl2::rect::Point::new(KEY_W - 4, ny + NOTE_H - 1),
                );
            }
        }
        canvas.set_clip_rect(None);
    }

    // ── Beat/bar ruler ────────────────────────────────────────────────
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(28, 32, 38, 255));
    let _ = canvas.fill_rect(Rect::new(
        KEY_W,
        ruler_top,
        (vscroll_x - KEY_W) as u32,
        RULER_H as u32,
    ));
    {
        let start_beat = scroll_x.floor() as i32 - 1;
        let end_beat = start_beat + (grid_w as f64 / zoom) as i32 + 4;
        for beat in start_beat..=end_beat {
            let bx = beat_to_x(beat as f64);
            if bx < KEY_W || bx > vscroll_x {
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
                    let lbl = format!("{}", bar_num);
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &lbl,
                        bx + 3,
                        ruler_top + 3,
                        24,
                        sdl2::pixels::Color::RGBA(180, 190, 210, 220),
                    );
                } else if zoom > 40.0 {
                    let sub = beat % 4 + 1;
                    let lbl = format!(".{}", sub);
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &lbl,
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
        sdl2::rect::Point::new(KEY_W, ruler_top + RULER_H - 1),
        sdl2::rect::Point::new(vscroll_x, ruler_top + RULER_H - 1),
    );

    // ── Vertical beat grid lines ──────────────────────────────────────
    canvas.set_clip_rect(Rect::new(KEY_W, grid_top, grid_w as u32, grid_h as u32));
    {
        let start_beat = scroll_x.floor() as i32 - 1;
        let end_beat = start_beat + (grid_w as f64 / zoom) as i32 + 4;

        // Sub-division lines at snap resolution
        if zoom > 6.0 {
            let mut sub = (scroll_x / snap_beats).floor() * snap_beats;
            while sub < scroll_x + grid_w as f64 / zoom + 2.0 {
                let bx = beat_to_x(sub);
                let on_beat = (sub.fract().abs() < 1e-6) || ((sub.fract() - 1.0).abs() < 1e-6);
                if !on_beat && bx > KEY_W && bx < vscroll_x {
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 40, 50, 80));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(bx, grid_top),
                        sdl2::rect::Point::new(bx, grid_top + grid_h),
                    );
                }
                sub += snap_beats;
            }
        }

        for beat in start_beat..=end_beat {
            let bx = beat_to_x(beat as f64);
            if bx <= KEY_W || bx > vscroll_x {
                continue;
            }
            let is_bar = beat % 4 == 0;
            canvas.set_draw_color(if is_bar {
                sdl2::pixels::Color::RGBA(65, 70, 90, 160)
            } else {
                sdl2::pixels::Color::RGBA(45, 45, 58, 100)
            });
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(bx, grid_top),
                sdl2::rect::Point::new(bx, grid_top + grid_h),
            );
        }
    }
    canvas.set_clip_rect(None);

    // ── Draw notes ────────────────────────────────────────────────────
    canvas.set_clip_rect(Rect::new(KEY_W, grid_top, grid_w as u32, grid_h as u32));

    let note_data: Vec<(usize, i32, u8, f64, f64)> = {
        state
            .project
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| {
                if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                    Some(
                        m.notes
                            .iter()
                            .enumerate()
                            .map(|(i, n)| (i, n.pitch as i32, n.velocity, n.start, n.length))
                            .collect(),
                    )
                } else {
                    None
                }
            })
            .unwrap_or_default()
    };

    let mut hovered_note: Option<usize> = None;
    let mut hovered_note_edge: i32 = 0; // -1 = left, 1 = right, 0 = none
    let resize_zone = 6i32;

    for &(ni, pitch, velocity, note_start, note_len) in &note_data {
        let nx = beat_to_x(note_start);
        let nw = (note_len * zoom).max(4.0) as i32;
        let ny = pitch_to_y(pitch);

        if nx + nw < KEY_W || nx > vscroll_x {
            continue;
        }
        if ny + NOTE_H < grid_top || ny > grid_top + grid_h {
            continue;
        }

        let selected = state.piano_roll_selected_notes.contains(&ni);
        let body_hover = input.mouse_in_rect(nx, ny + 1, nw.max(4), NOTE_H - 2)
            && input.mouse_y >= grid_top
            && input.mouse_y < grid_top + grid_h;

        if body_hover {
            let lx = input.mouse_x - nx;
            if nw > resize_zone * 2 && lx >= nw - resize_zone {
                hovered_note_edge = 1;
            } else if nw > resize_zone * 2 && lx <= resize_zone {
                hovered_note_edge = -1;
            } else {
                hovered_note_edge = 0;
            }
            hovered_note = Some(ni);
        }

        // ── Color scheme ──
        let nc = state.theme.note_on;
        let (r, g, b) = if selected {
            (
                nc[0].saturating_add(60),
                nc[1].saturating_add(50),
                nc[2].saturating_add(80),
            )
        } else if body_hover {
            (
                nc[0].saturating_add(30),
                nc[1].saturating_add(25),
                nc[2].saturating_add(40),
            )
        } else {
            (nc[0], nc[1], nc[2])
        };

        // Main body
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(r, g, b, 230));
        let _ = canvas.fill_rect(Rect::new(nx, ny + 1, nw as u32, (NOTE_H - 2) as u32));

        // Velocity brightness strip (top 2px)
        let vel_t = velocity as f32 / 127.0;
        let strip_h = ((vel_t * (NOTE_H - 2) as f32) as i32).clamp(1, NOTE_H - 2);
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 50));
        let _ = canvas.fill_rect(Rect::new(nx, ny + 1, nw as u32, strip_h as u32));

        // Resize handles visual (darker strip)
        if nw > resize_zone * 2 {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 60));
            let _ = canvas.fill_rect(Rect::new(
                nx + nw - resize_zone,
                ny + 1,
                resize_zone as u32,
                (NOTE_H - 2) as u32,
            )); // Right
            let _ = canvas.fill_rect(Rect::new(
                nx,
                ny + 1,
                resize_zone as u32,
                (NOTE_H - 2) as u32,
            )); // Left
        }

        // Border
        let border_col = if selected {
            sdl2::pixels::Color::RGBA(200, 200, 255, 255)
        } else {
            sdl2::pixels::Color::RGBA(0, 0, 0, 100)
        };
        canvas.set_draw_color(border_col);
        let _ = canvas.draw_rect(Rect::new(nx, ny + 1, nw as u32, (NOTE_H - 2) as u32));
    }
    canvas.set_clip_rect(None);

    // ── Playhead (clip-relative, loops within clip length) ──
    {
        let loop_len = clip_len;

        // Compute clip-relative playhead position
        let raw_pos = state.project.transport.position - clip_start;
        let ph_beat = if loop_len > 0.0 && raw_pos >= 0.0 {
            raw_pos % loop_len
        } else {
            raw_pos
        };
        state.piano_roll_playhead = ph_beat;

        let ph_x = beat_to_x(ph_beat);
        if ph_x >= KEY_W && ph_x <= vscroll_x {
            canvas.set_draw_color(Theme::c(state.theme.playhead));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(ph_x, ruler_top),
                sdl2::rect::Point::new(ph_x, vel_top),
            );
            // Triangle marker in ruler
            for d in 0..4i32 {
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(ph_x - d, ruler_top + d),
                    sdl2::rect::Point::new(ph_x + d, ruler_top + d),
                );
            }
        }
    }

    // ── Rubberband ────────────────────────────────────────────────────
    if let Some((rx1, ry1, rx2, ry2)) = state.piano_roll_rubberband {
        let ac = state.theme.accent;
        let sx = rx1.min(rx2);
        let sy = ry1.min(ry2);
        let sw = (rx1 - rx2).unsigned_abs();
        let sh = (ry1 - ry2).unsigned_abs();
        // Outline only — thin lines, no fill
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(ac[0], ac[1], ac[2], 200));
        let _ = canvas.draw_rect(Rect::new(sx, sy, sw.max(1), sh.max(1)));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(ac[0], ac[1], ac[2], 100));
        let _ = canvas.draw_rect(Rect::new(
            sx.saturating_sub(1),
            sy.saturating_sub(1),
            sw + 2,
            sh + 2,
        ));
    }

    // ── Input handling ────────────────────────────────────────────────
    let in_grid = input.mouse_in_rect(KEY_W, grid_top, grid_w, grid_h);
    let in_ruler = input.mouse_in_rect(KEY_W, ruler_top, grid_w, RULER_H);
    let in_keys = input.mouse_in_rect(0, grid_top, KEY_W, grid_h);

    // ── Piano key click → preview note sound ─────────────────────────
    if in_keys && input.mouse_pressed {
        let pitch = y_to_pitch(input.mouse_y);
        if (0..128).contains(&pitch) {
            // Find the track index for the selected clip's track
            if let Some(ti) = state.project.tracks.iter().position(|t| t.id == track_id) {
                state.preview_notes.push((ti, pitch as u8, 100));
                // Remember which pitch we started so note-off is correct
                // even if the mouse moves vertically before release.
                state.piano_roll_preview_pitch = Some(pitch as u8);
            }
        }
    }
    // Release key → stop preview note (works even if released outside keys area)
    if input.mouse_released {
        if let Some(pressed_pitch) = state.piano_roll_preview_pitch.take() {
            state.preview_notes.retain(|&(_, p, _)| p != pressed_pitch);
            state.piano_note_off_queue.push(pressed_pitch);
        }
    }
    // If mouse leaves the keys area while held, also stop preview
    if !in_keys && input.mouse_down && !state.preview_notes.is_empty() {
        // Only remove if we're not drawing (draw drag handles its own cleanup)
        if state.piano_roll_draw_drag.is_none() {
            for &(_, p, _) in &state.preview_notes {
                state.piano_note_off_queue.push(p);
            }
            state.preview_notes.clear();
            state.piano_roll_preview_pitch = None;
        }
    }

    // Click ruler → set playhead (convert clip-relative beat back to arrangement position)
    if in_ruler && input.mouse_pressed && input.drag_widget == WidgetId::None {
        input.drag_widget = WidgetId::Auto(80100);
        input.active_widget = WidgetId::Auto(80100);
        let beat = x_to_beat(input.mouse_x).max(0.0);
        state.project.transport.position = (beat + clip_start).max(clip_start);
        state.seek_pending = true;
    }
    // Continue tracking playhead while dragging even if mouse leaves ruler area
    if input.drag_widget == WidgetId::Auto(80100) && input.mouse_down {
        let beat = x_to_beat(input.mouse_x).max(0.0);
        state.project.transport.position = (beat + clip_start).max(clip_start);
        state.seek_pending = true;
    }

    // ── Note resize drag ───────────────────────────
    let is_resizing_right = matches!(input.drag_widget, WidgetId::Auto(80001));
    let is_resizing_left = matches!(input.drag_widget, WidgetId::Auto(80002));
    if is_resizing_right || is_resizing_left {
        if input.mouse_down {
            let dx_beats = (input.mouse_x - input.drag_start_x) as f64 / zoom;
            let origins = state.piano_roll_resize_origins.clone();

            if is_resizing_right {
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(clip_idx) {
                        for (&sni, &(_orig_s, orig_l)) in &origins {
                            let raw_len = (orig_l + dx_beats).max(snap_beats.min(0.125));
                            let new_len = pr_snap_prox(raw_len).max(snap_beats.min(0.125));
                            if let Some(note) = m.notes.get_mut(sni) {
                                note.length = new_len;
                            }
                        }
                        state.dirty = true;
                    }
                }
            } else if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(clip_idx) {
                    for (&sni, &(orig_s, orig_l)) in &origins {
                        let raw_start = orig_s + dx_beats;
                        let new_start = pr_snap_prox(raw_start)
                            .max(0.0)
                            .min(orig_s + orig_l - snap_beats.min(0.125));
                        let new_len = orig_s + orig_l - new_start;
                        if let Some(note) = m.notes.get_mut(sni) {
                            note.start = new_start;
                            note.length = new_len;
                        }
                    }
                    state.dirty = true;
                }
            }
        }
        if input.mouse_released {
            // Build a composite undo command for all resized notes
            let origins = state.piano_roll_resize_origins.clone();
            let mut cmds: Vec<Box<dyn crate::commands::Command>> = Vec::new();
            if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                if let Some(crate::models::Clip::Midi(m)) = track.clips.get(clip_idx) {
                    for (&sni, &(_orig_s, orig_l)) in &origins {
                        if let Some(note) = m.notes.get(sni) {
                            let new_len = note.length;
                            if (new_len - orig_l).abs() > 1e-9 {
                                cmds.push(Box::new(crate::commands::ResizeMidiNote {
                                    track_id,
                                    clip_idx,
                                    note_idx: sni,
                                    old_len: orig_l,
                                    new_len,
                                }));
                            }
                        }
                    }
                }
            }
            if !cmds.is_empty() {
                state.commands.execute(
                    Box::new(crate::commands::CompositeCommand {
                        desc: "Resize MIDI Notes".to_string(),
                        cmds,
                    }),
                    &mut state.project,
                );
            }
            state.piano_roll_resize_origins.clear();
        }
    }

    // ── Note move drag ────────────────────────────────────────────────
    let is_moving = state.piano_roll_moving;
    if is_moving {
        if input.mouse_down && input.dragging {
            let dx_beats = (input.mouse_x - input.drag_start_x) as f64 / zoom;
            let dy_semi = -(input.mouse_y - input.drag_start_y) / NOTE_H;
            let snap_dx = pr_snap_prox(dx_beats);

            // Apply delta to all selected notes live
            // First, compute the max allowed leftward shift so no note goes below 0.
            let origins = state.piano_roll_move_origins.clone();
            let mut clamped_dx = snap_dx;
            for &(orig_start, _) in origins.values() {
                // If this note would go negative, limit the delta
                let min_dx = -orig_start; // most negative delta this note allows
                if clamped_dx < min_dx {
                    clamped_dx = min_dx;
                }
            }
            for (&ni, &(orig_start, orig_pitch)) in &origins {
                let new_start = orig_start + clamped_dx;
                let new_pitch = ((orig_pitch as i32 + dy_semi).clamp(0, 127)) as u8;
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(clip_idx) {
                        if let Some(note) = m.notes.get_mut(ni) {
                            note.start = new_start;
                            note.pitch = new_pitch;
                            state.dirty = true;
                        }
                    }
                }
            }
        }
        if input.mouse_released {
            if state.piano_roll_clone_drag {
                // Clone drag release: the cloned notes are already in the clip at their
                // current positions. Collect them, remove them, then use AddMidiNote
                // commands so undo works correctly.
                let mut cloned_notes: Vec<crate::models::MidiNote> = Vec::new();
                let mut indices_to_remove: Vec<usize> =
                    state.piano_roll_selected_notes.iter().copied().collect();
                indices_to_remove.sort_unstable_by(|a, b| b.cmp(a)); // descending for safe removal

                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(clip_idx) {
                        for &idx in &indices_to_remove {
                            if idx < m.notes.len() {
                                cloned_notes.push(m.notes.remove(idx));
                            }
                        }
                    }
                }
                cloned_notes.reverse(); // restore original order
                                        // Now add them back via commands for proper undo
                if !cloned_notes.is_empty() {
                    let mut sub_cmds: Vec<Box<dyn crate::commands::Command>> = Vec::new();
                    for note in cloned_notes {
                        sub_cmds.push(Box::new(crate::commands::AddMidiNote {
                            track_id,
                            clip_idx,
                            note,
                        }));
                    }
                    state.commands.execute(
                        Box::new(crate::commands::CompositeCommand {
                            cmds: sub_cmds,
                            desc: "Clone MIDI Notes".into(),
                        }),
                        &mut state.project,
                    );
                }
                state.piano_roll_clone_drag = false;
            } else {
                // Normal move: commit undo command
                let origins = state.piano_roll_move_origins.clone();
                let mut moves = Vec::new();
                for (&ni, &(orig_start, orig_pitch)) in &origins {
                    let (new_start, new_pitch) = state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == track_id)
                        .and_then(|t| {
                            if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                                m.notes.get(ni).map(|n| (n.start, n.pitch))
                            } else {
                                None
                            }
                        })
                        .unwrap_or((orig_start, orig_pitch));
                    moves.push((ni, orig_start, orig_pitch, new_start, new_pitch));
                }
                if !moves.is_empty() {
                    state.commands.execute(
                        Box::new(crate::commands::MoveMidiNotes {
                            track_id,
                            clip_idx,
                            moves,
                        }),
                        &mut state.project,
                    );
                }
            }
            state.piano_roll_moving = false;
            state.piano_roll_move_origins.clear();
        }
    }

    // ── Draw-drag: drawing new note by click+drag ─────────────────────
    if let Some((note_beat, note_pitch, drag_sx, drag_sbeat)) = state.piano_roll_draw_drag {
        // When dragging left past the click point the note should start at the
        // cursor and grow rightward toward where the mouse was clicked — mirroring
        // the behaviour of every other DAW note-draw tool.
        let raw_cur_beat = x_to_beat(input.mouse_x).max(0.0);
        let raw_len = (raw_cur_beat - drag_sbeat).abs().max(snap_beats.min(0.125));
        let note_len = pr_snap(raw_len).max(snap_beats.min(0.125));
        // Actual start: left edge of the note regardless of drag direction
        let actual_start = if raw_cur_beat < drag_sbeat {
            pr_snap(raw_cur_beat).max(0.0)
        } else {
            note_beat
        };
        let nx = beat_to_x(actual_start);
        let nw = (note_len * zoom).max(4.0) as i32;
        let ny = pitch_to_y(note_pitch as i32);
        let nc = state.theme.note_on;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(nc[0], nc[1], nc[2], 180));
        let _ = canvas.fill_rect(Rect::new(nx, ny + 1, nw as u32, (NOTE_H - 2) as u32));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 180));
        let _ = canvas.draw_rect(Rect::new(nx, ny + 1, nw as u32, (NOTE_H - 2) as u32));

        if input.mouse_released {
            // Commit note at the correct start position
            let final_len = pr_snap(note_len).max(snap_beats.min(0.125));
            let new_note = crate::models::MidiNote {
                pitch: note_pitch,
                velocity: 100,
                start: actual_start,
                length: final_len,
            };
            state.commands.execute(
                Box::new(crate::commands::AddMidiNote {
                    track_id,
                    clip_idx,
                    note: new_note,
                }),
                &mut state.project,
            );
            state.piano_roll_draw_drag = None;
            let _ = drag_sx;
        }
    }

    // ── Rubberband select update ──────────────────────────────────────
    if let Some(ref mut rb) = state.piano_roll_rubberband {
        // Clamp to the visible grid region so the rect doesn't escape the piano roll panel
        rb.2 = input.mouse_x.clamp(KEY_W, vscroll_x);
        rb.3 = input.mouse_y.clamp(grid_top, grid_top + grid_h);
        // Live-select overlapping notes
        let (rx1, ry1, rx2, ry2) = *rb;
        let sel_x1 = rx1.min(rx2);
        let sel_x2 = rx1.max(rx2);
        let sel_y1 = ry1.min(ry2);
        let sel_y2 = ry1.max(ry2);
        state.piano_roll_selected_notes.clear();
        for &(ni, pitch, _, note_start, note_len) in &note_data {
            let nx = beat_to_x(note_start);
            let nw = (note_len * zoom).max(4.0) as i32;
            let ny = pitch_to_y(pitch);
            if nx < sel_x2 && nx + nw > sel_x1 && ny < sel_y2 && ny + NOTE_H > sel_y1 {
                state.piano_roll_selected_notes.insert(ni);
            }
        }
        if input.mouse_released {
            state.piano_roll_rubberband = None;
        }
    }

    // ── Focus: clicking anywhere in piano roll area claims keyboard focus ──
    let in_piano_roll_area = input.mouse_in_rect(0, top, w, h);
    if in_piano_roll_area && input.mouse_pressed {
        state.focused_panel = crate::state::FocusedPanel::PianoRoll;
    }

    // ── Mouse press: decide action ────────────────────────────────────
    // Ctrl+drag = select/rubberband; default = draw/move
    let use_select_mode = input.ctrl();
    if in_grid
        && input.mouse_pressed
        && !(is_resizing_right || is_resizing_left)
        && !is_moving
        && state.piano_roll_draw_drag.is_none()
        && state.piano_roll_rubberband.is_none()
    {
        let cur_beat = x_to_beat(input.mouse_x);
        let cur_pitch = y_to_pitch(input.mouse_y) as u8;

        if use_select_mode {
            // CLONE MODE (Ctrl held) — clicking a note clones all selected notes
            if let Some(ni) = hovered_note {
                // Ensure the hovered note is selected
                if !state.piano_roll_selected_notes.contains(&ni) {
                    state.piano_roll_selected_notes.clear();
                    state.piano_roll_selected_notes.insert(ni);
                }
                // Clone all selected notes
                let notes_to_clone: Vec<crate::models::MidiNote> = state
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .and_then(|t| {
                        if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                            Some(
                                state
                                    .piano_roll_selected_notes
                                    .iter()
                                    .filter_map(|&i| m.notes.get(i).cloned())
                                    .collect(),
                            )
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                if !notes_to_clone.is_empty() {
                    // Add cloned notes to the clip
                    let base_idx = state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == track_id)
                        .and_then(|t| {
                            if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                                Some(m.notes.len())
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id)
                    {
                        if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(clip_idx) {
                            for note in &notes_to_clone {
                                m.notes.push(note.clone());
                            }
                        }
                    }
                    // Select the new clones and start moving them
                    state.piano_roll_selected_notes.clear();
                    state.piano_roll_move_origins.clear();
                    for (i, note) in notes_to_clone.iter().enumerate() {
                        let new_idx = base_idx + i;
                        state.piano_roll_selected_notes.insert(new_idx);
                        state
                            .piano_roll_move_origins
                            .insert(new_idx, (note.start, note.pitch));
                    }
                    state.piano_roll_moving = true;
                    state.piano_roll_clone_drag = true;
                    input.drag_start_x = input.mouse_x;
                    input.drag_start_y = input.mouse_y;
                }
            } else {
                // Ctrl+click empty space: start rubberband
                if !input.shift() {
                    state.piano_roll_selected_notes.clear();
                }
                state.piano_roll_rubberband =
                    Some((input.mouse_x, input.mouse_y, input.mouse_x, input.mouse_y));
            }
        } else {
            // DRAW MODE (default)
            if input.right_mouse_pressed {
                // Right-click: handled below
            } else if let Some(ni) = hovered_note {
                if hovered_note_edge != 0 {
                    // Start resize — ensure the dragged note is selected
                    if !state.piano_roll_selected_notes.contains(&ni) {
                        state.piano_roll_selected_notes.clear();
                        state.piano_roll_selected_notes.insert(ni);
                    }
                    let (orig_start, orig_len) = state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == track_id)
                        .and_then(|t| {
                            if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                                m.notes.get(ni).map(|n| (n.start, n.length))
                            } else {
                                None
                            }
                        })
                        .unwrap_or((0.0, 0.5));
                    // Store original (start, length) for all selected notes
                    state.piano_roll_resize_origins.clear();
                    if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                        if let Some(crate::models::Clip::Midi(m)) = track.clips.get(clip_idx) {
                            for &sni in &state.piano_roll_selected_notes {
                                if let Some(n) = m.notes.get(sni) {
                                    state
                                        .piano_roll_resize_origins
                                        .insert(sni, (n.start, n.length));
                                }
                            }
                        }
                    }
                    let widget_id = if hovered_note_edge == 1 { 80001 } else { 80002 }; // 80001=Right, 80002=Left
                    input.drag_widget = WidgetId::Auto(widget_id);
                    input.active_widget = WidgetId::Auto(widget_id);
                    input.drag_start_x = input.mouse_x;
                    input.drag_start_value = orig_len;
                    input.drag_start_value2 = ni as f64;
                    state
                        .drag_original_positions
                        .insert((track_id, clip_idx), orig_start);
                } else {
                    // Click on existing note body: start move
                    if !state.piano_roll_selected_notes.contains(&ni) {
                        state.piano_roll_selected_notes.clear();
                        state.piano_roll_selected_notes.insert(ni);
                    }
                    state.piano_roll_moving = true;
                    state.piano_roll_move_origins.clear();
                    let origins: Vec<(usize, f64, u8)> = state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == track_id)
                        .and_then(|t| {
                            if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                                Some(
                                    state
                                        .piano_roll_selected_notes
                                        .iter()
                                        .filter_map(|&i| {
                                            m.notes.get(i).map(|n| (i, n.start, n.pitch))
                                        })
                                        .collect(),
                                )
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    for (i, s, p) in origins {
                        state.piano_roll_move_origins.insert(i, (s, p));
                    }
                    input.drag_start_x = input.mouse_x;
                    input.drag_start_y = input.mouse_y;
                }
            } else {
                // Click empty space: if notes are selected, deselect first; only draw if nothing was selected
                if !state.piano_roll_selected_notes.is_empty() {
                    state.piano_roll_selected_notes.clear();
                } else {
                    let beat_in_clip = pr_snap(cur_beat).max(0.0);
                    state.piano_roll_draw_drag =
                        Some((beat_in_clip, cur_pitch, input.mouse_x, cur_beat));
                }
            }
        }
    }

    // Right-click in draw mode: delete hovered note (press or drag-erase)
    if in_grid && (input.right_mouse_pressed || input.right_mouse_down) && !use_select_mode {
        if let Some(ni) = hovered_note {
            let orig = state
                .project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| {
                    if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                        m.notes.get(ni).cloned().map(|n| (ni, n))
                    } else {
                        None
                    }
                });
            if let Some((ni, n)) = orig {
                state.commands.execute(
                    Box::new(crate::commands::DeleteMidiNotes {
                        track_id,
                        clip_idx,
                        notes: vec![(ni, n)],
                    }),
                    &mut state.project,
                );
                state.piano_roll_selected_notes.remove(&ni);
                state.dirty = true;
            }
        } else if input.right_mouse_pressed {
            state.piano_roll_selected_notes.clear();
        }
    }

    // Delete key: delete selected notes (only when piano roll has focus)
    if state.focused_panel == crate::state::FocusedPanel::PianoRoll
        && !state.piano_roll_selected_notes.is_empty()
        && (input.key_available(sdl2::keyboard::Keycode::Delete)
            || input.key_available(sdl2::keyboard::Keycode::Backspace))
    {
        let to_delete: Vec<(usize, crate::models::MidiNote)> = state
            .project
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| {
                if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                    Some(
                        state
                            .piano_roll_selected_notes
                            .iter()
                            .filter_map(|&i| m.notes.get(i).cloned().map(|n| (i, n)))
                            .collect(),
                    )
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !to_delete.is_empty() {
            state.commands.execute(
                Box::new(crate::commands::DeleteMidiNotes {
                    track_id,
                    clip_idx,
                    notes: to_delete,
                }),
                &mut state.project,
            );
            state.piano_roll_selected_notes.clear();
            state.dirty = true;
        }
        input.consume_key(sdl2::keyboard::Keycode::Delete);
        input.consume_key(sdl2::keyboard::Keycode::Backspace);
    }

    // ── Ctrl+A: select all (only when piano roll has focus) ──────────
    if state.focused_panel == crate::state::FocusedPanel::PianoRoll
        && input.ctrl()
        && input.key_available(sdl2::keyboard::Keycode::A)
    {
        let n = state
            .project
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .and_then(|t| {
                if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                    Some(m.notes.len())
                } else {
                    None
                }
            })
            .unwrap_or(0);
        state.piano_roll_selected_notes = (0..n).collect();
        input.consume_key(sdl2::keyboard::Keycode::A);
    }

    // ── Up/Down arrows: move selected notes by semitone (Shift = octave) ──
    // Only when piano roll has focus, to prevent cross-bleed with arrangement.
    if state.focused_panel == crate::state::FocusedPanel::PianoRoll {
        let up = input.key_available(sdl2::keyboard::Keycode::Up);
        let down = input.key_available(sdl2::keyboard::Keycode::Down);
        if (up || down) && !state.piano_roll_selected_notes.is_empty() {
            let shift = if input.shift() { 12i8 } else { 1i8 };
            let delta: i8 = if up { shift } else { -shift };
            // First check if the move is safe (no note goes out of 0..127)
            let can_move = state
                .project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| {
                    if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                        Some(state.piano_roll_selected_notes.iter().all(|&ni| {
                            if let Some(note) = m.notes.get(ni) {
                                let new_pitch = note.pitch as i16 + delta as i16;
                                (0..=127).contains(&new_pitch)
                            } else {
                                false
                            }
                        }))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            if can_move {
                // Collect move data: (index, old_start, old_pitch, new_start, new_pitch)
                let moves: Vec<(usize, f64, u8, f64, u8)> = state
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .and_then(|t| {
                        if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                            Some(
                                state
                                    .piano_roll_selected_notes
                                    .iter()
                                    .filter_map(|&ni| {
                                        m.notes.get(ni).map(|n| {
                                            let new_pitch = (n.pitch as i16 + delta as i16) as u8;
                                            (ni, n.start, n.pitch, n.start, new_pitch)
                                        })
                                    })
                                    .collect(),
                            )
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                if !moves.is_empty() {
                    state.commands.execute(
                        Box::new(crate::commands::MoveMidiNotes {
                            track_id,
                            clip_idx,
                            moves,
                        }),
                        &mut state.project,
                    );
                    state.dirty = true;
                }
            }
        }
        // Consume the arrow keys so arrangement doesn't also handle them
        if up {
            input.consume_key(sdl2::keyboard::Keycode::Up);
        }
        if down {
            input.consume_key(sdl2::keyboard::Keycode::Down);
        }
    }

    // ── Mouse-wheel scroll/zoom ───────────────────────────────────────
    let in_roll = input.mouse_in_rect(KEY_W, top, vscroll_x - KEY_W, h - SCROLL_T);
    // Allow scrolling from anywhere in the piano roll panel (including piano keys column)
    let in_roll_any = input.mouse_in_rect(0, top, vscroll_x, h);
    if (in_roll || in_roll_any)
        && state.piano_roll_draw_drag.is_none()
        && !state.piano_roll_moving
        && !input.scroll_consumed
    {
        if input.scroll_y != 0 {
            if input.ctrl() {
                // Zoom toward cursor
                let factor = if input.scroll_y > 0 { 1.15 } else { 0.87 };
                let old_z = state.piano_roll_zoom_x;
                let new_z = (old_z * factor).clamp(8.0, 600.0);
                let cpx = (input.mouse_x - KEY_W) as f64;
                let beat_under = state.piano_roll_scroll_x + cpx / old_z;
                state.piano_roll_scroll_x = (beat_under - cpx / new_z).max(0.0);
                state.piano_roll_zoom_x = new_z;
            } else if input.shift() {
                // Horizontal scroll
                let delta = input.scroll_y as f64 * (snap_beats * 4.0);
                state.piano_roll_scroll_x = (state.piano_roll_scroll_x - delta).max(0.0);
            } else {
                // Vertical pitch scroll
                state.piano_roll_scroll_y =
                    (state.piano_roll_scroll_y - input.scroll_y * 3).clamp(0, TOTAL - 1);
            }
        }
        if input.scroll_x != 0 {
            let delta = input.scroll_x as f64 * (snap_beats * 4.0);
            state.piano_roll_scroll_x = (state.piano_roll_scroll_x - delta).max(0.0);
        }
    }

    // Middle mouse drag to pan (guard with middle_drag_widget to prevent arranger conflict)
    if input.middle_mouse_down && in_roll && input.middle_drag_widget == WidgetId::None {
        input.middle_drag_widget = WidgetId::Auto(86099);
    }
    if input.middle_mouse_down && input.middle_drag_widget == WidgetId::Auto(86099) {
        let dx_beats = input.mouse_dx as f64 / zoom;
        state.piano_roll_scroll_x = (state.piano_roll_scroll_x - dx_beats).max(0.0);
        let dy_semi = input.mouse_dy / NOTE_H;
        state.piano_roll_scroll_y = (state.piano_roll_scroll_y - dy_semi).clamp(0, TOTAL - 1);
    }

    // ── Velocity lane ─────────────────────────────────────────────────
    if state.velocity_editor_visible {
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 26, 255));
        let _ = canvas.fill_rect(Rect::new(KEY_W, vel_top, grid_w as u32, (vel_h - 2) as u32));
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(KEY_W, vel_top),
            sdl2::rect::Point::new(vscroll_x, vel_top),
        );
        draw_pixel_label(
            canvas,
            &state.theme,
            "VEL",
            2,
            vel_top + (vel_h / 2) - 3,
            KEY_W - 4,
            sdl2::pixels::Color::RGBA(100, 100, 120, 200),
        );

        canvas.set_clip_rect(Rect::new(KEY_W, vel_top, grid_w as u32, (vel_h - 2) as u32));

        // Draw clip end line in velocity lane
        if clip_end_x >= KEY_W && clip_end_x <= vscroll_x {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, 100));
            let _ = canvas.fill_rect(Rect::new(clip_end_x - 1, vel_top, 2, (vel_h - 2) as u32));
        }

        let nc = state.theme.note_on;
        let vel_note_data: Vec<(usize, u8, f64)> = {
            state
                .project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| {
                    if let Some(crate::models::Clip::Midi(m)) = t.clips.get(clip_idx) {
                        Some(
                            m.notes
                                .iter()
                                .enumerate()
                                .map(|(i, n)| (i, n.velocity, n.start))
                                .collect(),
                        )
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        };
        for (ni, velocity, note_start) in &vel_note_data {
            let nx = beat_to_x(*note_start);
            if nx < KEY_W || nx > vscroll_x {
                continue;
            }
            let bar_h = (((*velocity as f32) / 127.0) * (vel_h - 6) as f32) as i32;
            let bar_y = vel_top + (vel_h - 4) - bar_h;
            let selected = state.piano_roll_selected_notes.contains(ni);
            let hover_vel = input.mouse_in_rect(nx - 3, vel_top, 7, vel_h);
            let col = if selected {
                sdl2::pixels::Color::RGBA(200, 200, 255, 220)
            } else if hover_vel {
                sdl2::pixels::Color::RGBA(
                    nc[0].saturating_add(60),
                    nc[1],
                    nc[2].saturating_add(60),
                    255,
                )
            } else {
                sdl2::pixels::Color::RGBA(nc[0], nc[1], nc[2], 200)
            };
            canvas.set_draw_color(col);
            let _ = canvas.fill_rect(Rect::new(nx - 1, bar_y, 3, bar_h as u32));
            // Top diamond head
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 180));
            let _ = canvas.fill_rect(Rect::new(nx - 2, bar_y - 2, 5, 3));
            // Shift-click to reset velocity to default (100)
            if hover_vel && input.mouse_pressed && input.shift() {
                let old_vel = *velocity;
                state.commands.execute(
                    Box::new(crate::commands::SetNoteVelocity {
                        track_id,
                        clip_idx,
                        note_idx: *ni,
                        old_velocity: old_vel,
                        new_velocity: 100,
                    }),
                    &mut state.project,
                );
                state.dirty = true;
            }
            // Drag to change velocity (live update, command committed in the block below)
            if hover_vel && input.mouse_down && !input.shift() {
                let new_vel = (((vel_top + vel_h - 4 - input.mouse_y) as f32 / (vel_h - 4) as f32)
                    * 127.0)
                    .clamp(1.0, 127.0) as u8;
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(clip_idx) {
                        if let Some(note) = m.notes.get_mut(*ni) {
                            // Store original velocity on first touch for undo
                            if input.mouse_pressed {
                                state.drag_velocity_note_idx = Some(*ni);
                                state.drag_velocity_original = *velocity;
                            }
                            note.velocity = new_vel;
                            state.dirty = true;
                        }
                    }
                }
            }
        }
        canvas.set_clip_rect(None);
        // Commit velocity change on mouse release
        if input.mouse_released {
            if let Some(ni) = state.drag_velocity_note_idx.take() {
                let old_vel = state.drag_velocity_original;
                if let Some(track) = state.project.tracks.iter().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Midi(m)) = track.clips.get(clip_idx) {
                        if let Some(note) = m.notes.get(ni) {
                            let new_vel = note.velocity;
                            if new_vel != old_vel {
                                state.commands.execute(
                                    Box::new(crate::commands::SetNoteVelocity {
                                        track_id,
                                        clip_idx,
                                        note_idx: ni,
                                        old_velocity: old_vel,
                                        new_velocity: new_vel,
                                    }),
                                    &mut state.project,
                                );
                            }
                        }
                    }
                }
            }
        }
    } // end if velocity_editor_visible

    // ── Piano roll horizontal scroomer (zoom + scroll) ────────────────
    {
        canvas.set_clip_rect(None);
        let sb_x = KEY_W;
        let sb_y = hscroll_y;
        let sb_len = grid_w; // full width from piano keys to vertical scrollbar
        let sb_h = SCROLL_T;

        let clip_len_beats = clip_info.map(|(_, _, _, l)| l).unwrap_or(32.0);
        let total_beats = (clip_len_beats * 1.25).max(32.0);
        let visible_beats = grid_w as f64 / zoom;
        let thumb_ratio = (visible_beats / total_beats).clamp(0.02, 1.0) as f32;
        let max_scroll_beats = (total_beats - visible_beats).max(0.001);
        let scroll_frac = (state.piano_roll_scroll_x / max_scroll_beats).clamp(0.0, 1.0) as f32;

        let (new_frac, new_ratio) = scrollbar_with_squeeze(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(86010),
            WidgetId::Auto(86011),
            WidgetId::Auto(86012),
            sb_x,
            sb_y,
            sb_len,
            sb_h,
            ScrollbarDir::Horizontal,
            scroll_frac,
            thumb_ratio,
        );

        let new_visible_beats = (new_ratio as f64 * total_beats).max(1.0);
        let new_zoom = (grid_w as f64 / new_visible_beats).clamp(4.0, 2000.0);
        let ratio_changed = (new_ratio - thumb_ratio).abs() > 0.001;
        let frac_changed = (new_frac - scroll_frac).abs() > 0.001;
        if ratio_changed {
            state.piano_roll_zoom_x = new_zoom;
        }
        if ratio_changed || frac_changed {
            let cur_zoom = state.piano_roll_zoom_x;
            let new_max_scroll = (total_beats - grid_w as f64 / cur_zoom).max(0.0);
            state.piano_roll_scroll_x = (new_frac as f64 * new_max_scroll).max(0.0);
        }
    }

    // ── Piano roll vertical scrollbar ─────────────────────────────────
    {
        canvas.set_clip_rect(None);
        let sb_x = vscroll_x;
        let sb_y = grid_top;
        let sb_len = grid_h;
        let sb_h = SCROLL_T;
        let total_rows = TOTAL as f32;
        let visible_rows = (grid_h / NOTE_H) as f32;
        let thumb_ratio = (visible_rows / total_rows).clamp(0.02, 1.0);
        let max_scroll = (TOTAL - (grid_h / NOTE_H)).max(0) as f32;
        let scroll_frac = if max_scroll > 0.0 {
            (state.piano_roll_scroll_y as f32 / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let new_frac = scrollbar(
            canvas,
            input,
            &state.theme,
            WidgetId::Auto(86020),
            sb_x,
            sb_y,
            sb_len,
            sb_h,
            ScrollbarDir::Vertical,
            scroll_frac,
            thumb_ratio,
        );
        state.piano_roll_scroll_y = (new_frac * max_scroll) as i32;
    }
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(KEY_W - 1, top),
        sdl2::rect::Point::new(KEY_W - 1, top + h),
    );
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, ruler_top),
        sdl2::rect::Point::new(vscroll_x, ruler_top),
    );
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, top),
        sdl2::rect::Point::new(w, top),
    );
    // Clip-length label
    {
        let total_bars = (clip_len / 4.0).ceil() as i32;
        let lbl = format!("{} bars  {}bts", total_bars, clip_len as i32);
        draw_pixel_label(
            canvas,
            &state.theme,
            &lbl,
            KEY_W + 4,
            top + h - vel_h - SCROLL_T - 10,
            140,
            sdl2::pixels::Color::RGBA(150, 150, 170, 160),
        );
    }
}
