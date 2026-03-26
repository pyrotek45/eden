// Eden DAW — View drawing functions
// Transport bar (with icon buttons), loop ruler, timeline, track headers,
// clip lanes with resize handles, mixer, and context-sensitive Edit page.

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::models::create_rack_slot_for_module;
use crate::state::*;
use crate::theme::Theme;
use crate::widgets::*;

// ── Musical gain ↔ slider-position helpers ──────────────────────────
// The gain multiplier 0.0–2.0 is mapped through a dB-aware curve so
// that the slider feels more natural: the lower half covers silence to
// unity, the upper half covers unity to +6 dB.
//
//  pos 0.0  →  gain 0.0   (−∞ dB)
//  pos 0.75 →  gain 1.0   (  0 dB)
//  pos 1.0  →  gain 2.0   ( +6 dB)
//
// We use a cubic power curve: gain = pos^3 * 2.0 (adjusted so 0.75^3*2 ≈ 0.84
// is close to unity; for exact unity at 0.75 we scale so f(0.75)=1.0).
// f(pos) = pos^3 * (1.0 / 0.75^3) when pos <= 0.75  → maps [0, 0.75] to [0, 1.0]
// f(pos) = 1.0 + (pos - 0.75) / 0.25 * 1.0          → maps [0.75, 1.0] to [1.0, 2.0]

/// Convert a slider position [0,1] to a gain multiplier [0,2].
pub(crate) fn vol_pos_to_gain(pos: f32) -> f32 {
    if pos <= 0.75 {
        // Cubic ramp from 0 to 1.0
        let t = pos / 0.75; // 0..1
        t * t * t // 0..1 gain
    } else {
        // Linear from 1.0 to 2.0
        1.0 + (pos - 0.75) / 0.25
    }
}

/// Convert a gain multiplier [0,2] to a slider position [0,1].
pub(crate) fn vol_gain_to_pos(gain: f32) -> f32 {
    if gain <= 1.0 {
        // Inverse cubic
        0.75 * gain.max(0.0).cbrt()
    } else {
        // Inverse linear
        0.75 + (gain - 1.0) * 0.25
    }
}

/// Format a gain value as a dB string for display.
pub(crate) fn gain_to_db_label(gain: f32) -> String {
    if gain < 1e-6 {
        "-∞ dB".to_string()
    } else {
        let db = 20.0 * gain.log10();
        if db.abs() < 0.05 {
            "0.0 dB".to_string()
        } else {
            format!("{:+.1} dB", db)
        }
    }
}

// ── Transport bar ────────────────────────────────────────────────────

pub fn draw_transport(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let w = state.window_width as i32;
    let h = state.transport_bar_height();

    // ── Intra-transport layer manager ────────────────────────────────────────
    // The snap-grid dropdown is a small inline widget that can open a popup over
    // the transport bar.  It must be processed first (top layer) so its click
    // does not also trigger any button drawn behind it.  ViewLayers::below()
    // returns dead input once input.consumed is set by the top layer.
    let mut layers = ViewLayers::new(input);

    // Background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    // Bottom border
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, h - 1),
        sdl2::rect::Point::new(w, h - 1),
    );

    // ── Mini oscilloscope (between snap controls and mode tabs) ──
    {
        let osc_x = 490i32;
        let osc_w = (w - 540 - osc_x).clamp(60, 400);
        let osc_y = 8i32;
        let osc_h = 32i32;
        // Background
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(10, 10, 14, 200));
        let _ = canvas.fill_rect(Rect::new(osc_x, osc_y, osc_w as u32, osc_h as u32));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 40, 50, 255));
        let _ = canvas.draw_rect(Rect::new(osc_x, osc_y, osc_w as u32, osc_h as u32));
        // Center line
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 60, 50, 100));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(osc_x, osc_y + osc_h / 2),
            sdl2::rect::Point::new(osc_x + osc_w, osc_y + osc_h / 2),
        );
        // Waveform — only show if there's actual signal
        let osc = &state.meters.oscilloscope;
        let has_signal = osc.iter().any(|s| s.abs() > 0.001);
        if has_signal && !osc.is_empty() {
            let step = osc.len() as f32 / osc_w as f32;
            let mid = osc_y + osc_h / 2;
            let amp = osc_h / 2 - 2;
            for px in 0..osc_w - 1 {
                let i0 = ((px as f32 * step) as usize).min(osc.len() - 1);
                let i1 = (((px + 1) as f32 * step) as usize).min(osc.len() - 1);
                let y0 = mid - (osc[i0].clamp(-1.0, 1.0) * amp as f32) as i32;
                let y1 = mid - (osc[i1].clamp(-1.0, 1.0) * amp as f32) as i32;
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 220, 100, 200));
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(osc_x + px, y0),
                    sdl2::rect::Point::new(osc_x + px + 1, y1),
                );
            }
        }
        draw_pixel_label(
            canvas,
            &state.theme,
            "OSC",
            osc_x + 2,
            osc_y + 1,
            18,
            sdl2::pixels::Color::RGBA(80, 130, 80, 180),
        );
    }

    // ── Left-side transport icon buttons ─────────────────────────────────
    // Laid out with RowLayout so gaps are always consistent and overlap-free.
    {
        let inp = layers.below();
        let icon_w = 36i32;
        let btn_y = 8i32;
        let btn_h = 32i32;
        let row = RowLayout {
            x: 10,
            y: btn_y,
            total_width: icon_w * 6 + 4 * 5, // 6 buttons, 4px gaps
            height: btn_h,
            gap: 4,
        };
        let slots = row.layout(&[
            RowItem {
                width: icon_w,
                can_resize: false,
                min_width: icon_w,
            },
            RowItem {
                width: icon_w,
                can_resize: false,
                min_width: icon_w,
            },
            RowItem {
                width: icon_w,
                can_resize: false,
                min_width: icon_w,
            },
            RowItem {
                width: icon_w,
                can_resize: false,
                min_width: icon_w,
            },
            RowItem {
                width: icon_w,
                can_resize: false,
                min_width: icon_w,
            },
            RowItem {
                width: icon_w,
                can_resize: false,
                min_width: icon_w,
            },
        ]);

        // Rewind
        let __auto_id_0 = inp.next_id();
        let rewind_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_0,
                x: slots[0].0,
                y: btn_y,
                width: slots[0].1,
                height: btn_h,
                label: String::new(),
                toggled: false,
                icon: ButtonIcon::Rewind,
                hint: Some("Rewind (Enter)".into()),
                ..Default::default()
            },
        );
        if rewind_clicked {
            state.project.transport.playing = false;
            if state.project.transport.loop_enabled {
                state.project.transport.position = state.project.transport.loop_region.start;
            } else {
                state.project.transport.position = 0.0;
            }
            state.pre_play_position = state.project.transport.position;
            state.seek_pending = true;
            state.sample_preview_path = None;
            state.sample_preview_trigger = false;
        }

        // Play
        let __auto_id_1 = inp.next_id();
        let play_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_1,
                x: slots[1].0,
                y: btn_y,
                width: slots[1].1,
                height: btn_h,
                label: String::new(),
                toggled: state.project.transport.playing,
                icon: ButtonIcon::Play,
                hint: Some("Play/Pause (Space)".into()),
                ..Default::default()
            },
        );
        if play_clicked {
            if !state.project.transport.playing {
                state.pre_play_position = state.project.transport.position;
                state.seek_pending = true;
            } else if state.auto_return {
                state.project.transport.position = state.pre_play_position;
                state.seek_pending = true;
            }
            state.project.transport.playing = !state.project.transport.playing;
        }

        // Stop
        let __auto_id_2 = inp.next_id();
        let stop_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_2,
                x: slots[2].0,
                y: btn_y,
                width: slots[2].1,
                height: btn_h,
                label: String::new(),
                toggled: false,
                icon: ButtonIcon::Stop,
                hint: Some("Stop (Space)".into()),
                ..Default::default()
            },
        );
        if stop_clicked {
            state.project.transport.playing = false;
            if state.auto_return {
                state.project.transport.position = state.pre_play_position;
                state.seek_pending = true;
            }
            state.sample_preview_path = None;
            state.sample_preview_trigger = false;
        }

        // Auto-return
        let __auto_id_3 = inp.next_id();
        let auto_return_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_3,
                x: slots[3].0,
                y: btn_y,
                width: slots[3].1,
                height: btn_h,
                label: String::new(),
                toggled: state.auto_return,
                icon: ButtonIcon::AutoReturn,
                hint: Some("Auto Return".into()),
                ..Default::default()
            },
        );
        if auto_return_clicked {
            state.auto_return = !state.auto_return;
        }

        // Record
        let __auto_id_4 = inp.next_id();
        let _rec_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_4,
                x: slots[4].0,
                y: btn_y,
                width: slots[4].1,
                height: btn_h,
                label: String::new(),
                toggled: state.project.transport.recording,
                icon: ButtonIcon::Record,
                hint: Some("Record".into()),
                ..Default::default()
            },
        );

        // Loop
        let __auto_id_5 = inp.next_id();
        let loop_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_5,
                x: slots[5].0,
                y: btn_y,
                width: slots[5].1,
                height: btn_h,
                label: String::new(),
                toggled: state.project.transport.loop_enabled,
                icon: ButtonIcon::Loop,
                hint: Some("Loop (L)".into()),
                ..Default::default()
            },
        );
        if loop_clicked {
            state.project.transport.loop_enabled = !state.project.transport.loop_enabled;
        }
    }

    // Tempo BPM spinner (scroll wheel or drag to change)
    // Double-click to enter a typed value (validated via parse)
    let bpm_tf_id: u32 = 80000;
    let bpm_text_active = state.text_field_active_id == bpm_tf_id;
    let mut bpm_val = state.project.tempo_map.bpm_at(0.0);
    let bpm_spinner_id = layers.below().next_id();

    if bpm_text_active {
        // ── Text-entry mode: show a text field over the spinner ──
        let mut buf = state.text_field_buffer.clone();
        let mut cursor = state.text_field_cursor;
        let mut active_id = state.text_field_active_id;
        let (committed, new_val) = text_field(
            canvas,
            layers.below(),
            &state.theme,
            &TextFieldParams {
                id: bpm_tf_id,
                x: 262,
                y: 10,
                width: 72,
                height: 28,
                hint: Some("BPM".into()),
            },
            &format!("{:.1}", bpm_val),
            &mut active_id,
            &mut buf,
            &mut cursor,
        );
        state.text_field_active_id = active_id;
        state.text_field_buffer = buf.clone();
        state.text_field_cursor = cursor;
        if committed {
            if let Some(text) = new_val {
                if let Ok(parsed) = text.trim().parse::<f64>() {
                    let new_bpm = parsed.clamp(20.0, 400.0);
                    let old_bpm = state.project.tempo_map.bpm_at(0.0);
                    if (new_bpm - old_bpm).abs() > 1e-9 {
                        let snapshot = state.project.clone();
                        if let Some(first) = state.project.tempo_map.changes.first_mut() {
                            first.bpm = new_bpm;
                        }
                        crate::commands::rescale_audio_clips_pub(
                            &mut state.project,
                            old_bpm,
                            new_bpm,
                        );
                        state.commands.push_undo_snapshot(snapshot, "Set Tempo");
                        state.dirty = true;
                    }
                }
                // If parse fails, just ignore (revert to old value)
            }
        }
    } else {
        // ── Normal spinner mode ──
        // Detect double-click to enter text mode
        let inp = layers.below();
        if inp.mouse_in_rect(262, 10, 72, 28)
            && inp.mouse_pressed
            && inp.click_type == Some(crate::input::ClickType::Double)
        {
            state.text_field_active_id = bpm_tf_id;
            state.text_field_buffer = format!("{:.1}", bpm_val);
            state.text_field_cursor = state.text_field_buffer.len();
            inp.consume();
        } else {
            let bpm_changed = number_spinner(
                canvas,
                inp,
                &state.theme,
                bpm_spinner_id,
                262,
                10,
                72,
                28,
                20.0,
                400.0,
                0.1,
                1,
                &mut bpm_val,
            );
            if bpm_changed {
                if state.bpm_drag_orig.is_none() {
                    state.bpm_drag_orig = Some(state.project.tempo_map.bpm_at(0.0));
                    state.bpm_drag_snapshot = Some(state.project.clone());
                }
                let old_bpm = state.project.tempo_map.bpm_at(0.0);
                if let Some(first) = state.project.tempo_map.changes.first_mut() {
                    first.bpm = bpm_val;
                }
                crate::commands::rescale_audio_clips_pub(&mut state.project, old_bpm, bpm_val);
            }
            // Commit BPM change on mouse release
            if inp.mouse_released {
                if let Some(_old_bpm) = state.bpm_drag_orig.take() {
                    let new_bpm = state.project.tempo_map.bpm_at(0.0);
                    if let Some(snapshot) = state.bpm_drag_snapshot.take() {
                        let orig_bpm = snapshot.tempo_map.bpm_at(0.0);
                        if (new_bpm - orig_bpm).abs() > 1e-9 {
                            state.commands.push_undo_snapshot(snapshot, "Set Tempo");
                            state.dirty = true;
                        }
                    }
                }
            }
        }
    }
    // "BPM" label above
    draw_pixel_label(
        canvas,
        &state.theme,
        "BPM",
        268,
        4,
        40,
        sdl2::pixels::Color::RGBA(140, 140, 140, 200),
    );

    // Position display: bar:beat
    let pos = state.project.transport.position;
    let bar = (pos / 4.0).floor() as i32 + 1;
    let beat = (pos % 4.0).floor() as i32 + 1;
    let pos_str = format!("{}.{}", bar, beat);
    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(324, 10, 52, 28));
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_rect(Rect::new(324, 10, 52, 28));
    draw_pixel_label(
        canvas,
        &state.theme,
        &pos_str,
        328,
        21,
        44,
        sdl2::pixels::Color::RGBA(200, 220, 255, 255),
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        "POS",
        330,
        4,
        40,
        sdl2::pixels::Color::RGBA(140, 140, 140, 200),
    );

    // ── Snap controls ───────────────────────────────────────
    // Snap toggle button — below layer (dropdown takes top)
    let __auto_id_6 = layers.below().next_id();
    let snap_clicked = button(
        canvas,
        layers.below(),
        &state.theme,
        &ButtonParams {
            id: __auto_id_6,
            x: 384,
            y: 10,
            width: 36,
            height: 28,
            label: "SNAP".into(),
            toggled: state.snap.enabled,
            icon: ButtonIcon::None,
            hint: Some("Toggle snap to grid".into()),
            ..Default::default()
        },
    );
    if snap_clicked {
        state.snap.enabled = !state.snap.enabled;
    }
    draw_pixel_label(
        canvas,
        &state.theme,
        "SNAP",
        388,
        4,
        32,
        sdl2::pixels::Color::RGBA(140, 140, 140, 200),
    );

    // Grid resolution dropdown — top layer within the transport bar.
    // Processed via layers.top() so its consume() call blocks any widget
    // drawn at the same screen coordinates.
    // When OPEN: the full popup is re-rendered in draw_overlays on top.
    // We always draw the closed box here; the open state lives only in draw_overlays.
    {
        let snap_labels: Vec<&str> = SNAP_RESOLUTIONS.iter().map(|(l, _)| *l).collect();
        let is_open = state.dropdown_open_id == 200;
        let dd_x = 424i32;
        let dd_y = 10i32;
        let dd_w = 52i32;
        let dd_h = 28i32;
        let dd_inp = layers.top(); // top layer: dropdown has input priority
        let hover = dd_inp.mouse_in_rect(dd_x, dd_y, dd_w, dd_h);

        // Scroll wheel on closed dropdown cycles options
        if hover && dd_inp.scroll_y != 0 && !is_open && !dd_inp.scroll_consumed {
            let n = snap_labels.len();
            if dd_inp.scroll_y > 0 {
                state.snap.resolution_idx = state.snap.resolution_idx.saturating_sub(1);
            } else {
                state.snap.resolution_idx = (state.snap.resolution_idx + 1).min(n - 1);
            }
            dd_inp.scroll_consumed = true;
        }
        // Click to open/close — use consume() so no widget behind it can respond
        if hover && dd_inp.mouse_pressed {
            if is_open {
                state.dropdown_open_id = 0;
            } else {
                state.dropdown_open_id = 200;
            }
            dd_inp.consume();
        }

        // Draw the closed box (always)
        let bg = if is_open || hover {
            Theme::c(state.theme.button_hover)
        } else {
            Theme::c(state.theme.button_bg)
        };
        canvas.set_draw_color(bg);
        let _ = canvas.fill_rect(Rect::new(dd_x, dd_y, dd_w as u32, dd_h as u32));
        canvas.set_draw_color(if is_open {
            Theme::c(state.theme.accent)
        } else {
            Theme::c(state.theme.panel_border)
        });
        let _ = canvas.draw_rect(Rect::new(dd_x, dd_y, dd_w as u32, dd_h as u32));
        let label = snap_labels[state.snap.resolution_idx];
        draw_pixel_label(
            canvas,
            &state.theme,
            label,
            dd_x + 4,
            dd_y + (dd_h - 10) / 2,
            dd_w - 18,
            sdl2::pixels::Color::RGBA(220, 220, 220, 255),
        );
        // Chevron arrow
        let ax = dd_x + dd_w - 10;
        let ay = dd_y + dd_h / 2;
        canvas.set_draw_color(Theme::c(state.theme.text_secondary));
        let _ = canvas.fill_rect(Rect::new(ax, ay - 1, 7, 2));
        let _ = canvas.fill_rect(Rect::new(ax + 1, ay + 1, 5, 2));
        let _ = canvas.fill_rect(Rect::new(ax + 2, ay + 3, 3, 2));
        let _ = canvas.fill_rect(Rect::new(ax + 3, ay + 5, 1, 2));
    }
    draw_pixel_label(
        canvas,
        &state.theme,
        "GRID",
        388,
        4,
        40,
        sdl2::pixels::Color::RGBA(140, 140, 140, 200),
    );

    // ── Right-side buttons — RowLayout::layout_right() ──────────────────────
    // Items are declared left-to-right in visual order (leftmost = last in slice
    // for layout_right, which fills from the right edge).  The slice order here
    // matches the visual order left→right so we reverse for layout_right.
    {
        let inp = layers.below(); // below the snap dropdown top layer
        let btn_y = 9i32;
        let btn_h = 30i32;
        let sm = 38i32; // small button width
        let md = 56i32; // medium button width
        let qs = 30i32; // question-mark button

        // Slots in LEFT-TO-RIGHT visual order:
        //   [0]=?  [1]=STOP  [2]=KBD  [3]=Undo  [4]=Redo
        //   [5]=Project  [6]=Export  [7]=Save  [8]=Options  [9]=Home
        // layout_right expects index-0 = rightmost, so we declare them that way.
        let right_margin = 10i32;
        let row = RowLayout {
            x: 0,
            y: btn_y,
            total_width: w - right_margin,
            height: btn_h,
            gap: 4,
        };
        // Rightmost first: Home, Options, Save, Export, Project, Redo, Undo, KBD, STOP, ?
        let slots = row.layout_right(&[
            RowItem {
                width: sm,
                can_resize: false,
                min_width: sm,
            }, // Home
            RowItem {
                width: md,
                can_resize: false,
                min_width: md,
            }, // Options
            RowItem {
                width: md,
                can_resize: false,
                min_width: md,
            }, // Save
            RowItem {
                width: md,
                can_resize: false,
                min_width: md,
            }, // Export
            RowItem {
                width: md,
                can_resize: false,
                min_width: md,
            }, // Project
            RowItem {
                width: sm,
                can_resize: false,
                min_width: sm,
            }, // Redo
            RowItem {
                width: sm,
                can_resize: false,
                min_width: sm,
            }, // Undo
            RowItem {
                width: sm,
                can_resize: false,
                min_width: sm,
            }, // KBD
            RowItem {
                width: sm,
                can_resize: false,
                min_width: sm,
            }, // STOP
            RowItem {
                width: qs,
                can_resize: false,
                min_width: qs,
            }, // ?
        ]);
        // slots[0] = Home (rightmost), slots[9] = ? (leftmost of fixed buttons)

        // Home
        let __auto_id_7 = inp.next_id();
        let home_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_7,
                x: slots[0].0,
                y: btn_y,
                width: slots[0].1,
                height: btn_h,
                label: "Home".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Back to project manager".into()),
                ..Default::default()
            },
        );
        if home_clicked {
            state.mode = crate::state::AppMode::ProjectManager;
        }

        // Options
        let __auto_id_8 = inp.next_id();
        let opts_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_8,
                x: slots[1].0,
                y: btn_y,
                width: slots[1].1,
                height: btn_h,
                label: "Options".into(),
                toggled: state.options_open,
                icon: ButtonIcon::None,
                hint: Some("Open options panel".into()),
                ..Default::default()
            },
        );
        if opts_clicked {
            state.options_open = !state.options_open;
        }

        // Save (left-click = quick save, right-click = save as)
        let save_btn_x = slots[2].0;
        let save_btn_w = slots[2].1;
        let __auto_id_9 = inp.next_id();
        let save_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_9,
                x: save_btn_x,
                y: btn_y,
                width: save_btn_w,
                height: btn_h,
                label: "Save".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Save (Ctrl+S) | Right-click: Save As".into()),
                ..Default::default()
            },
        );
        if save_clicked {
            match state.quick_save() {
                Ok(()) => println!("[save] Project saved"),
                Err(e) => eprintln!("[save] Error: {}", e),
            }
        }
        // Right-click Save → open Save As popup
        if inp.mouse_in_rect(save_btn_x, btn_y, save_btn_w, btn_h)
            && inp.right_mouse_pressed
            && !inp.consumed
        {
            let default_name = if let Some(ref p) = state.last_save_path {
                p.clone()
            } else {
                format!("{}.eden.json", state.project.name)
            };
            state.save_as_name_buffer = default_name;
            state.save_as_popup_open = true;
            inp.consumed = true;
        }

        // Export
        let __auto_id_10 = inp.next_id();
        let render_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_10,
                x: slots[3].0,
                y: btn_y,
                width: slots[3].1,
                height: btn_h,
                label: "Export".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Export project to WAV".into()),
                ..Default::default()
            },
        );
        if render_clicked {
            if !state.render_popup_open {
                state.render_filename = format!("{}.wav", state.project.name);
                state.render_loop_only = state.project.transport.loop_enabled;
                state.render_popup_open = true;
            } else {
                state.render_popup_open = false;
            }
        }

        // Project
        let __auto_id_11 = inp.next_id();
        let project_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_11,
                x: slots[4].0,
                y: btn_y,
                width: slots[4].1,
                height: btn_h,
                label: "Project".into(),
                toggled: state.project_popup_open,
                icon: ButtonIcon::None,
                hint: Some("Project settings".into()),
                ..Default::default()
            },
        );
        if project_clicked {
            state.project_popup_open = !state.project_popup_open;
        }

        // Redo
        let redo_hint = match state.commands.redo_description() {
            Some(desc) => format!("Redo: {} (Ctrl+Y)", desc),
            None => "Nothing to redo (Ctrl+Y)".into(),
        };
        let __auto_id_12 = inp.next_id();
        let redo_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_12,
                x: slots[5].0,
                y: btn_y,
                width: slots[5].1,
                height: btn_h,
                label: "Redo".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some(redo_hint),
                ..Default::default()
            },
        );
        if redo_clicked {
            if let Some(desc) = state.commands.redo_description() {
                state.push_status(format!("Redo: {}", desc));
            }
            state.commands.redo(&mut state.project);
        }

        // Undo
        let undo_hint = match state.commands.undo_description() {
            Some(desc) => format!("Undo: {} (Ctrl+Z)", desc),
            None => "Nothing to undo (Ctrl+Z)".into(),
        };
        let __auto_id_13 = inp.next_id();
        let undo_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_13,
                x: slots[6].0,
                y: btn_y,
                width: slots[6].1,
                height: btn_h,
                label: "Undo".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some(undo_hint),
                ..Default::default()
            },
        );
        if undo_clicked {
            if let Some(desc) = state.commands.undo_description() {
                state.push_status(format!("Undo: {}", desc));
            }
            state.commands.undo(&mut state.project);
        }

        // KBD
        let __auto_id_14 = inp.next_id();
        let piano_mode_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_14,
                x: slots[7].0,
                y: btn_y,
                width: slots[7].1,
                height: btn_h,
                label: "KBD".into(),
                toggled: state.piano_keyboard_mode,
                icon: ButtonIcon::None,
                hint: Some("Toggle computer-keyboard piano mode".into()),
                ..Default::default()
            },
        );
        if piano_mode_clicked {
            state.piano_keyboard_mode = !state.piano_keyboard_mode;
            if !state.piano_keyboard_mode {
                state.piano_keyboard_held.clear();
            }
        }

        // STOP (panic)
        let __auto_id_15 = inp.next_id();
        let panic_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_15,
                x: slots[8].0,
                y: btn_y,
                width: slots[8].1,
                height: btn_h,
                label: "STOP".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Panic: stop all sounds".into()),
                ..Default::default()
            },
        );
        if panic_clicked {
            state.project.transport.playing = false;
            state.panic_triggered = true;
            state.preview_notes.clear();
            state.piano_keyboard_held.clear();
            state.push_status("Panic — all sounds stopped".to_string());
        }

        // ? (Help) — leftmost of the fixed buttons
        let __auto_id_18 = inp.next_id();
        let help_clicked = button(
            canvas,
            inp,
            &state.theme,
            &ButtonParams {
                id: __auto_id_18,
                x: slots[9].0,
                y: btn_y,
                width: slots[9].1,
                height: btn_h,
                label: "?".into(),
                toggled: state.help_screen_visible,
                icon: ButtonIcon::None,
                hint: Some("Help / Keyboard shortcuts (F1)".into()),
                ..Default::default()
            },
        );
        if help_clicked {
            state.help_screen_visible = !state.help_screen_visible;
        }

        // ── Octave controls — dynamic, placed left of the fixed buttons ──────
        // These only appear in piano-keyboard mode. They use a local cursor
        // that starts at slots[9].0 (the left edge of the '?' button) and
        // expands further left.
        if state.piano_keyboard_mode {
            let oct_btn_w = 22i32;
            let gap = 4i32;
            // '>' octave-up: immediately left of '?'
            let mut oct_rx = slots[9].0 - gap;

            oct_rx -= oct_btn_w;
            let __auto_id_16 = inp.next_id();
            let oct_up = button(
                canvas,
                inp,
                &state.theme,
                &ButtonParams {
                    id: __auto_id_16,
                    x: oct_rx,
                    y: btn_y + 4,
                    width: oct_btn_w,
                    height: 22,
                    label: ">".into(),
                    toggled: false,
                    icon: ButtonIcon::None,
                    hint: Some("Octave up (X)".into()),
                    ..Default::default()
                },
            );
            if oct_up {
                state.piano_keyboard_octave = (state.piano_keyboard_octave + 1).min(9);
            }

            // Octave label
            oct_rx -= gap + 28;
            let oct_label = format!("C{}", state.piano_keyboard_octave);
            draw_pixel_label(
                canvas,
                &state.theme,
                &oct_label,
                oct_rx,
                btn_y + 10,
                24,
                sdl2::pixels::Color::RGBA(255, 180, 80, 255),
            );

            // '<' octave-down
            oct_rx -= oct_btn_w;
            let __auto_id_17 = inp.next_id();
            let oct_down = button(
                canvas,
                inp,
                &state.theme,
                &ButtonParams {
                    id: __auto_id_17,
                    x: oct_rx,
                    y: btn_y + 4,
                    width: oct_btn_w,
                    height: 22,
                    label: "<".into(),
                    toggled: false,
                    icon: ButtonIcon::None,
                    hint: Some("Octave down (Z)".into()),
                    ..Default::default()
                },
            );
            if oct_down {
                state.piano_keyboard_octave = (state.piano_keyboard_octave - 1).max(0);
            }

            // Held-note dots
            if !state.piano_keyboard_held.is_empty() {
                let held_count = state.piano_keyboard_held.len().min(6);
                for i in 0..held_count {
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 200, 120, 200));
                    let dot_x = oct_rx - (i as i32 + 1) * 5;
                    let _ = canvas.fill_rect(Rect::new(dot_x, btn_y + 11, 4, 8));
                }
            }
        }
    }

    // ── Clicking the transport bar (anywhere not consumed) focuses arrangement ──
    if input.mouse_pressed && !input.consumed && input.mouse_in_rect(0, 0, w, h) {
        state.focused_panel = crate::state::FocusedPanel::Arrangement;
    }
}

// ── Loop ruler ── (merged into timeline; this fn is now a no-op) ─────

pub fn draw_loop_ruler(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let y = state.transport_bar_height();
    let h = state.loop_ruler_height();
    let w = state.window_width as i32;
    let header_w = state.arrangement.track_header_width + state.arrangement_left_offset();

    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, y, w as u32, h as u32));

    let zoom = state.arrangement.zoom_x;
    let scroll = state.arrangement.scroll_x;

    // ── "LOOP" label in header area ──
    draw_pixel_label(
        canvas,
        &state.theme,
        "LOOP",
        4,
        y + (h - 8) / 2,
        header_w - 8,
        Theme::c(state.theme.text_dim),
    );

    // ── Grid lines (matching timeline ruler grid) ──
    let beat_px = zoom;
    let start_beat = scroll.floor() as i32;
    let end_beat = start_beat + ((w - header_w) as f64 / beat_px) as i32 + 2;
    for beat in start_beat..end_beat {
        if beat < 0 {
            continue;
        }
        let x = header_w + ((beat as f64 - scroll) * beat_px) as i32;
        if x < header_w || x > w {
            continue;
        }
        if beat % 4 == 0 {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                state.theme.text_dim[0],
                state.theme.text_dim[1],
                state.theme.text_dim[2],
                60,
            ));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(x, y),
                sdl2::rect::Point::new(x, y + h),
            );
        }
    }

    let enabled = state.project.transport.loop_enabled;
    if enabled {
        let loop_start = state.project.transport.loop_region.start;
        let loop_end = state.project.transport.loop_region.end;
        let lx1 = header_w + ((loop_start - scroll) * zoom) as i32;
        let lx2 = header_w + ((loop_end - scroll) * zoom) as i32;
        let lc = state.theme.loop_color;

        let edge_alpha = 180u8;

        // Filled region between handles
        {
            let fill_x0 = lx1.max(header_w);
            let fill_x1 = lx2.min(w);
            if fill_x1 > fill_x0 {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], 45));
                let _ =
                    canvas.fill_rect(Rect::new(fill_x0, y, (fill_x1 - fill_x0) as u32, h as u32));
            }
        }

        // Left edge line
        if lx1 >= header_w && lx1 <= w {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], edge_alpha));
            let _ = canvas.fill_rect(Rect::new(lx1, y, 2, h as u32));
        }
        // Right edge line
        if lx2 >= header_w && lx2 <= w {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], edge_alpha));
            let _ = canvas.fill_rect(Rect::new(lx2 - 1, y, 2, h as u32));
        }

        let flag_w = 12i32;

        let ruler_area = input.mouse_in_rect(header_w, y, w - header_w, h);
        let mut near_start = (input.mouse_x - lx1).abs() <= flag_w + 2 && ruler_area;
        let mut near_end = (input.mouse_x - lx2).abs() <= flag_w + 2 && ruler_area;

        // Break tie when both handles are within hit range (they overlap or are very close).
        // Prioritize whichever handle the mouse is closer to; if equidistant, prefer end handle.
        if near_start && near_end {
            let dist_start = (input.mouse_x - lx1).abs();
            let dist_end = (input.mouse_x - lx2).abs();
            if dist_start <= dist_end {
                near_end = false;
            } else {
                near_start = false;
            }
        }

        // Start handle — brighter + white outline when hovered
        if lx1 >= header_w - flag_w && lx1 <= w {
            let ha = if near_start { 255u8 } else { 220u8 };
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], ha));
            let _ = canvas.fill_rect(Rect::new(lx1, y, 2, h as u32));
            let flag_col = if near_start {
                sdl2::pixels::Color::RGBA(255, 255, 255, 255)
            } else {
                sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], ha)
            };
            canvas.set_draw_color(flag_col);
            let _ = canvas.fill_rect(Rect::new(lx1 + 2, y, flag_w as u32, h as u32));
            // Label on hover
            if near_start {
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    "S",
                    lx1 + 3,
                    y + 2,
                    flag_w - 2,
                    sdl2::pixels::Color::RGBA(20, 20, 20, 255),
                );
            }
        }
        // End handle
        if lx2 >= header_w && lx2 <= w + flag_w {
            let ha = if near_end { 255u8 } else { 220u8 };
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], ha));
            let _ = canvas.fill_rect(Rect::new(lx2 - 2, y, 2, h as u32));
            let flag_col = if near_end {
                sdl2::pixels::Color::RGBA(255, 255, 255, 255)
            } else {
                sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], ha)
            };
            canvas.set_draw_color(flag_col);
            let _ = canvas.fill_rect(Rect::new(lx2 - flag_w - 2, y, flag_w as u32, h as u32));
            if near_end {
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    "E",
                    lx2 - flag_w - 1,
                    y + 2,
                    flag_w - 2,
                    sdl2::pixels::Color::RGBA(20, 20, 20, 255),
                );
            }
        }

        if ruler_area && input.right_mouse_pressed {
            state.focused_panel = crate::state::FocusedPanel::Arrangement;
            state.project.transport.loop_enabled = false;
            input.consume();
        }

        if ruler_area && input.mouse_pressed && !near_end && input.drag_widget == WidgetId::None {
            state.focused_panel = crate::state::FocusedPanel::Arrangement;
            if near_start {
                state.loop_drag_orig = Some((loop_start, loop_end));
                input.drag_widget = WidgetId::LoopStart;
                input.active_widget = WidgetId::LoopStart;
                input.drag_start_value = loop_start;
            } else {
                state.loop_drag_orig = Some((
                    state.project.transport.loop_region.start,
                    state.project.transport.loop_region.end,
                ));
                let raw = scroll + (input.mouse_x - header_w) as f64 / zoom;
                let beat = state.snap.snap(raw).max(0.0);
                // Click-and-drag: start and end both at click point,
                // dragging will expand bidirectionally via LoopBar handler.
                state.project.transport.loop_region.start = beat;
                state.project.transport.loop_region.end = beat;
                state.project.transport.loop_enabled = true;
                input.drag_widget = WidgetId::LoopBar;
                input.active_widget = WidgetId::LoopBar;
                input.drag_start_value = beat;
            }
            input.consume();
        }
        if ruler_area
            && input.mouse_pressed
            && near_end
            && !near_start
            && input.drag_widget == WidgetId::None
        {
            state.focused_panel = crate::state::FocusedPanel::Arrangement;
            state.loop_drag_orig = Some((loop_start, loop_end));
            input.drag_widget = WidgetId::LoopEnd;
            input.active_widget = WidgetId::LoopEnd;
            input.drag_start_value = loop_end;
            input.consume();
        }

        if input.mouse_down {
            let raw = scroll + (input.mouse_x - header_w) as f64 / zoom;
            let beat = state.snap.snap(raw).max(0.0);
            match input.drag_widget {
                WidgetId::LoopStart => {
                    state.project.transport.loop_region.start =
                        beat.min(state.project.transport.loop_region.end - 0.25);
                }
                WidgetId::LoopEnd => {
                    state.project.transport.loop_region.end =
                        beat.max(state.project.transport.loop_region.start + 0.25);
                }
                WidgetId::LoopBar => {
                    let anchor = input.drag_start_value;
                    let (lo, hi) = if beat < anchor {
                        (beat, anchor)
                    } else {
                        (anchor, beat)
                    };
                    state.project.transport.loop_region.start = lo.max(0.0);
                    state.project.transport.loop_region.end = hi.max(lo + 0.25);
                }
                _ => {}
            }
        }
    } else {
        // Allow drawing new loop region when disabled
        let ruler_area = input.mouse_in_rect(header_w, y, w - header_w, h);
        if ruler_area && input.mouse_pressed && input.drag_widget == WidgetId::None {
            state.focused_panel = crate::state::FocusedPanel::Arrangement;
            state.loop_drag_orig = Some((
                state.project.transport.loop_region.start,
                state.project.transport.loop_region.end,
            ));
            let raw = scroll + (input.mouse_x - header_w) as f64 / zoom;
            let beat = state.snap.snap(raw).max(0.0);
            // Click-and-drag: start and end both at click point.
            state.project.transport.loop_region.start = beat;
            state.project.transport.loop_region.end = beat;
            state.project.transport.loop_enabled = true;
            input.drag_widget = WidgetId::LoopBar;
            input.active_widget = WidgetId::LoopBar;
            input.drag_start_value = beat;
            input.consume();
        }
    }

    // Commit loop region change on release
    if input.mouse_released {
        if let Some((old_start, old_end)) = state.loop_drag_orig.take() {
            let new_start = state.project.transport.loop_region.start;
            let new_end = state.project.transport.loop_region.end;
            if (new_start - old_start).abs() > 1e-9 || (new_end - old_end).abs() > 1e-9 {
                // Restore old values so command can apply properly
                state.project.transport.loop_region.start = old_start;
                state.project.transport.loop_region.end = old_end;
                state.commands.execute(
                    Box::new(crate::commands::SetLoopRegion {
                        new_start,
                        new_end,
                        old_start: 0.0,
                        old_end: 0.0,
                    }),
                    &mut state.project,
                );
                state.dirty = true;
            }
        }
    }

    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, y + h - 1),
        sdl2::rect::Point::new(w, y + h - 1),
    );
}

pub fn draw_timeline_ruler(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    let y = state.transport_bar_height() + state.loop_ruler_height();
    let h = state.timeline_ruler_height();
    let w = state.window_width as i32;
    let header_w = state.arrangement.track_header_width + state.arrangement_left_offset();

    canvas.set_draw_color(Theme::c(state.theme.bg_light));
    let _ = canvas.fill_rect(Rect::new(0, y, w as u32, h as u32));

    let zoom = state.arrangement.zoom_x;
    let scroll = state.arrangement.scroll_x;
    let beat_px = zoom;
    let start_beat = scroll.floor() as i32;
    let end_beat = start_beat + ((w - header_w) as f64 / beat_px) as i32 + 2;

    for beat in start_beat..end_beat {
        if beat < 0 {
            continue;
        }
        let x = header_w + ((beat as f64 - scroll) * beat_px) as i32;
        if x < header_w || x > w {
            continue;
        }

        if beat % 4 == 0 {
            canvas.set_draw_color(Theme::c(state.theme.text_secondary));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(x, y),
                sdl2::rect::Point::new(x, y + h),
            );
            let bar_num = beat / 4 + 1;
            let label = format!("{}", bar_num);
            draw_pixel_label(
                canvas,
                &state.theme,
                &label,
                x + 3,
                y + 3,
                30,
                Theme::c(state.theme.text_primary),
            );
            if beat_px > 20.0 {
                for sub in 1..4 {
                    let sx = header_w + ((beat as f64 + sub as f64 - scroll) * beat_px) as i32;
                    if sx > header_w && sx < w {
                        canvas.set_draw_color(Theme::c(state.theme.text_dim));
                        let _ = canvas.draw_line(
                            sdl2::rect::Point::new(sx, y + h / 2),
                            sdl2::rect::Point::new(sx, y + h),
                        );
                        if beat_px > 40.0 {
                            let blabel = format!(".{}", sub + 1);
                            draw_pixel_label(
                                canvas,
                                &state.theme,
                                &blabel,
                                sx + 2,
                                y + h / 2 + 2,
                                14,
                                Theme::c(state.theme.text_dim),
                            );
                        }
                    }
                }
            }
        }
    }

    let ruler_area = input.mouse_in_rect(header_w, y, w - header_w, h);
    let not_blocked = input.mouse_y < state.bottom_panel_y();
    if ruler_area && input.mouse_pressed && not_blocked {
        state.focused_panel = crate::state::FocusedPanel::Arrangement;
        let raw = scroll + (input.mouse_x - header_w) as f64 / zoom;
        let beat = state.snap.snap(raw).max(0.0);
        state.project.transport.position = beat;
        state.seek_pending = true; // tell audio thread to seek immediately
        input.drag_widget = WidgetId::Playhead;
        input.active_widget = WidgetId::Playhead;
        input.consume();
    }

    if input.mouse_down && input.drag_widget == WidgetId::Playhead {
        let raw = scroll + (input.mouse_x - header_w) as f64 / zoom;
        let beat = state.snap.snap(raw).max(0.0);
        state.project.transport.position = beat;
        state.seek_pending = true;
    }

    // Draw the playhead marker in the timeline ruler
    let playhead_beat = state.project.transport.position;
    let px = header_w + ((playhead_beat - scroll) * zoom) as i32;
    if px >= header_w && px <= w {
        canvas.set_draw_color(Theme::c(state.theme.playhead));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(px, y),
            sdl2::rect::Point::new(px, y + h),
        );
        let _ = canvas.fill_rect(Rect::new(px - 4, y, 8, 4));
        let _ = canvas.fill_rect(Rect::new(px - 3, y + 4, 6, 4));
        let _ = canvas.fill_rect(Rect::new(px - 2, y + 8, 4, 4));
    }

    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, y + h - 1),
        sdl2::rect::Point::new(w, y + h - 1),
    );

    // Clicking the timeline ruler header-column area focuses the arrangement
    if input.mouse_pressed && !input.consumed && input.mouse_in_rect(0, y, header_w, h) {
        state.focused_panel = crate::state::FocusedPanel::Arrangement;
    }
}

// ── Mode tabs ────────────────────────────────────────────────────────

// ── Mode tabs ── (no-op, mode buttons are in the transport bar) ──────

pub fn draw_mode_tabs(
    _canvas: &mut Canvas<Window>,
    _input: &mut InputState,
    _state: &mut AppState,
) {
}

// ── Track headers ────────────────────────────────────────────────────

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
        crate::models::TrackType,
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
            && input.click_type == Some(crate::input::ClickType::Double)
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
                                Box::new(crate::commands::SetTrackName {
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
        if track_type != crate::models::TrackType::Automation {
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
                    default_value: Some(vol_gain_to_pos(0.8)),
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
                            Box::new(crate::commands::SetTrackVolume {
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
                            Box::new(crate::commands::SetTrackPan {
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
        if track_type != crate::models::TrackType::Automation {
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
                        Box::new(crate::commands::SetTrackMute {
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
                            Box::new(crate::commands::SetTrackSolo {
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
        if track_type == crate::models::TrackType::Automation {
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
                Box::new(crate::commands::ReorderTrack {
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
                Box::new(crate::commands::ReorderTrack {
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
                    Box::new(crate::commands::ResizeTrack {
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
            state.focused_panel = crate::state::FocusedPanel::Arrangement;
            if input.click_type == Some(crate::input::ClickType::Double) {
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
                ("♪ MIDI Track", crate::models::TrackType::Midi),
                ("♫ Audio Track", crate::models::TrackType::Audio),
                ("~ Auto Track", crate::models::TrackType::Automation),
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
                        crate::models::TrackType::Midi => format!("MIDI {}", new_id),
                        crate::models::TrackType::Audio => format!("Audio {}", new_id),
                        crate::models::TrackType::Automation => format!("Auto {}", new_id),
                    };
                    let new_track = crate::models::Track::new(new_id, &name, *tt);
                    state.commands.execute(
                        Box::new(crate::commands::AddTrack { track: new_track }),
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
            state.focused_panel = crate::state::FocusedPanel::Arrangement;
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
                    let mut new_track = crate::models::Track::new(
                        new_id,
                        &module_name,
                        crate::models::TrackType::Midi,
                    );
                    new_track.rack =
                        vec![crate::models::create_rack_slot_for_module(&module_name, 1)];
                    state.commands.execute(
                        Box::new(crate::commands::AddTrack { track: new_track }),
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
                crate::models::Clip::Midi(_) => [40, 55, 90, 230], // blue-ish
                crate::models::Clip::Audio(_) => [35, 65, 45, 230], // green-ish
                crate::models::Clip::Automation(_) => [70, 60, 30, 230], // amber-ish
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
                crate::models::Clip::Midi(_) => [120, 180, 255, 255],
                crate::models::Clip::Audio(_) => [100, 230, 140, 255],
                crate::models::Clip::Automation(_) => [240, 200, 80, 255],
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
                    && input.click_type == Some(crate::input::ClickType::Double)
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
                                    Some(crate::models::Clip::Midi(m)) => {
                                        m.name = new_name;
                                    }
                                    Some(crate::models::Clip::Audio(a)) => {
                                        a.name = new_name;
                                    }
                                    Some(crate::models::Clip::Automation(a)) => {
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

            if let crate::models::Clip::Midi(ref midi) = state.project.tracks[track_idx].clips[ci] {
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
            if let crate::models::Clip::Automation(ref auto) =
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
                // Restore previous clip rect
                if let Some(r) = prev_clip_rect {
                    canvas.set_clip_rect(Rect::new(r.x(), r.y(), r.width(), r.height()));
                } else {
                    canvas.set_clip_rect(None);
                }
            }

            // ── Audio waveform mini-preview ──
            if let crate::models::Clip::Audio(ref audio_clip) =
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
                let zone = hover_zone;
                match zone {
                    ClipHitZone::LeftHandle => {
                        input.active_widget = WidgetId::ClipLeftHandle(track_id, ci);
                        input.drag_widget = WidgetId::ClipLeftHandle(track_id, ci);
                        input.drag_start_value = clip_start + clip_len;
                        input.drag_start_value2 = clip_start;
                        if let crate::models::Clip::Audio(ref ac) =
                            state.project.tracks[track_idx].clips[ci]
                        {
                            state.drag_audio_offset_orig = ac.offset;
                        } else {
                            state.drag_audio_offset_orig = 0.0;
                        }
                        state.drag_original_positions.clear();
                        for &(t_id, c_idx) in &state.selected_clips {
                            if let Some(tr) = state.project.tracks.iter().find(|t| t.id == t_id) {
                                if let Some(c) = tr.clips.get(c_idx) {
                                    state
                                        .drag_original_positions
                                        .insert((t_id, c_idx), c.start_time());
                                }
                            }
                        }
                    }
                    ClipHitZone::RightHandle => {
                        input.active_widget = WidgetId::ClipRightHandle(track_id, ci);
                        input.drag_widget = WidgetId::ClipRightHandle(track_id, ci);
                        input.drag_start_value = clip_len;
                        input.drag_start_value2 = clip_start;
                        if let crate::models::Clip::Audio(ref ac) =
                            state.project.tracks[track_idx].clips[ci]
                        {
                            state.drag_audio_offset_orig = ac.offset;
                        } else {
                            state.drag_audio_offset_orig = 0.0;
                        }
                        state.drag_original_positions.clear();
                        for &(t_id, c_idx) in &state.selected_clips {
                            if let Some(tr) = state.project.tracks.iter().find(|t| t.id == t_id) {
                                if let Some(c) = tr.clips.get(c_idx) {
                                    state
                                        .drag_original_positions
                                        .insert((t_id, c_idx), c.length());
                                }
                            }
                        }
                    }
                    ClipHitZone::Header => {
                        // Header is the ONLY zone that selects/drags a clip.
                        if input.shift() {
                            // Shift+click header: toggle clip in multi-select
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

                            if input.click_type == Some(crate::input::ClickType::Double) {
                                // Double-click header = rename (handled above in text-field block)
                                input.active_widget = WidgetId::ClipBody(track_id, ci);
                            } else if input.ctrl() || input.alt() {
                                // Ctrl+drag from header = clone drag
                                state.clip_drag_is_copy = true;
                                state.clip_drag_copy = None;
                                input.active_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_start_value = clip_start;
                            } else {
                                // Normal drag from header = move
                                state.clip_drag_is_copy = false;
                                state.clip_drag_copy = None;
                                input.active_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_widget = WidgetId::ClipBody(track_id, ci);
                                input.drag_start_value = clip_start;
                            }

                            // Snapshot original positions for ALL selected clips
                            state.drag_original_positions.clear();
                            for &(tid, cidx) in &state.selected_clips {
                                if let Some(track) =
                                    state.project.tracks.iter().find(|t| t.id == tid)
                                {
                                    if let Some(c) = track.clips.get(cidx) {
                                        state
                                            .drag_original_positions
                                            .insert((tid, cidx), c.start_time());
                                    }
                                }
                            }
                        }
                    }
                    ClipHitZone::Body | ClipHitZone::None => {
                        // Double-click on clip body → open the clip in the editor.
                        // Single body clicks pass through (allow rubber-band and lane clicks).
                        if input.click_type == Some(crate::input::ClickType::Double)
                            && topmost_hovered_ci == Some(ci)
                            && !input.consumed
                        {
                            // Select this clip
                            state.selected_clip = Some((track_id, ci));
                            state.selected_clips.clear();
                            state.selected_clips.insert((track_id, ci));
                            state.selected_track = Some(track_id);
                            state.selected_tracks.clear();
                            state.selected_tracks.insert(track_id);
                            // Open the appropriate editor panel tab
                            let tab = match &state.project.tracks[track_idx].clips[ci] {
                                crate::models::Clip::Midi(_) => BottomPanelTab::PianoRoll,
                                crate::models::Clip::Audio(_) => BottomPanelTab::PianoRoll,
                                crate::models::Clip::Automation(_) => BottomPanelTab::PianoRoll,
                            };
                            state.bottom_panel_tab = tab;
                            if !state.bottom_panel_open {
                                state.bottom_panel_open = true;
                                state.bottom_panel_height = 320;
                            }
                            input.consume();
                        }
                        // Non-double-click body clicks still pass through for rubber-band etc.
                    }
                }
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
                        Box::new(crate::commands::DeleteClips {
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
                && input.click_type == Some(crate::input::ClickType::Double)
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
                    crate::models::TrackType::Midi => {
                        crate::models::Clip::Midi(crate::models::MidiClip {
                            name: clip_name,
                            color: track_color,
                            start_time: start,
                            length: 4.0, // 1 bar
                            notes: Vec::new(),
                        })
                    }
                    crate::models::TrackType::Audio => {
                        crate::models::Clip::Audio(crate::models::AudioClip {
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
                    crate::models::TrackType::Automation => {
                        crate::models::Clip::Automation(crate::models::AutomationClip {
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
                    Box::new(crate::commands::CreateClip {
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
                    let mut new_track = crate::models::Track::new(
                        new_id,
                        &module_name,
                        crate::models::TrackType::Midi,
                    );
                    new_track.rack =
                        vec![crate::models::create_rack_slot_for_module(&module_name, 1)];
                    state.commands.execute(
                        Box::new(crate::commands::AddTrack { track: new_track }),
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

    // ── Shift+drag right handle: time stretch visual indicator ────────
    if let WidgetId::ClipRightHandle(_, _) = input.drag_widget {
        if input.mouse_down && input.shift() {
            let mx = input.mouse_x;
            let my = input.mouse_y;
            // Draw a "TIME STRETCH" label near the cursor
            let label = "TIME STRETCH";
            let lw = label.len() as i32 * 9 + 12;
            let lh = 18;
            let lx = mx + 12;
            let ly = my - 24;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 140, 40, 230));
            let _ = canvas.fill_rect(Rect::new(lx, ly, lw as u32, lh as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, 255));
            let _ = canvas.draw_rect(Rect::new(lx, ly, lw as u32, lh as u32));
            draw_pixel_label(
                canvas,
                &state.theme,
                label,
                lx + 6,
                ly + 4,
                lw - 12,
                sdl2::pixels::Color::RGBA(30, 20, 10, 255),
            );
            // Draw stretch arrows icon (← →) next to handle
            let arrow_y = my;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, 200));
            // Right arrow
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(mx + 4, arrow_y),
                sdl2::rect::Point::new(mx + 14, arrow_y),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(mx + 11, arrow_y - 3),
                sdl2::rect::Point::new(mx + 14, arrow_y),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(mx + 11, arrow_y + 3),
                sdl2::rect::Point::new(mx + 14, arrow_y),
            );
            // Left arrow
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(mx - 4, arrow_y),
                sdl2::rect::Point::new(mx - 14, arrow_y),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(mx - 11, arrow_y - 3),
                sdl2::rect::Point::new(mx - 14, arrow_y),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(mx - 11, arrow_y + 3),
                sdl2::rect::Point::new(mx - 14, arrow_y),
            );
        }
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
        state.focused_panel = crate::state::FocusedPanel::Arrangement;

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
        state.clip_drag_copy = None;
        state.clip_drag_ghost_positions.clear();
        state.clip_drag_target_track = None;
        state.clip_drag_target_valid = false;
        // Clear drag snapshots — they're only valid during an active drag
        state.drag_original_positions.clear();
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
                        let is_cross = display_tid != src_tid;
                        let show_invalid = is_cross && !target_valid;

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
                        crate::models::Clip::Midi(m) => m.start_time + m.length,
                        crate::models::Clip::Audio(a) => a.start_time + a.length,
                        crate::models::Clip::Automation(a) => a.start_time + a.length,
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
    scroll_x: f64,
    header_w: i32,
) {
    // On the release frame, input.dragging is already false (cleared in apply_scale),
    // so we must also check for mouse_released with an active clip drag widget.
    let is_clip_drag_release = input.mouse_released
        && matches!(
            input.active_widget,
            WidgetId::ClipBody(_, _)
                | WidgetId::ClipLeftHandle(_, _)
                | WidgetId::ClipRightHandle(_, _)
        );

    if !input.dragging && !is_clip_drag_release {
        return;
    }

    let snap_threshold = state.snap.resolution_beats() * 0.30;
    // Minimum clip length = snap grid resolution (so you can zoom in and make very small clips)
    let min_clip_len = if state.snap.enabled {
        state.snap.resolution_beats()
    } else {
        0.03125 // 1/128 note, effectively no minimum
    };
    let cursor_beat = scroll_x + (input.mouse_x - header_w) as f64 / zoom;

    // On the release frame, drag_widget is already cleared to None (in apply_scale),
    // so use active_widget (which survives until begin_frame) for release detection.
    let effective_widget = if input.dragging {
        input.drag_widget
    } else {
        input.active_widget
    };

    match effective_widget {
        WidgetId::ClipBody(tid, ci) => {
            let total_dx_px = input.mouse_x - input.drag_start_x;
            let raw_delta = total_dx_px as f64 / zoom;
            let raw = (input.drag_start_value + raw_delta).max(0.0);
            let snapped = state.snap.snap_proximity(raw, snap_threshold);
            let actual_delta = snapped - input.drag_start_value;

            let top = state.track_area_top();
            let scroll_y = state.arrangement.scroll_y;
            let mut ty = top - scroll_y;
            let mut target_track_id: Option<u32> = None;
            let orig_track_type = state
                .project
                .tracks
                .iter()
                .find(|t| t.id == tid)
                .map(|t| t.track_type);

            for track in &state.project.tracks {
                let bot = ty + track.height;
                if input.mouse_y >= ty && input.mouse_y < bot {
                    target_track_id = Some(track.id);
                    break;
                }
                ty = bot;
            }

            // Check if the target track is compatible (same track type)
            let target_valid = match (orig_track_type, target_track_id) {
                (Some(orig_tt), Some(tgt_id)) => state
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == tgt_id)
                    .map(|t| t.track_type == orig_tt)
                    .unwrap_or(false),
                _ => false,
            };

            // Store target track for visual feedback
            state.clip_drag_target_track = target_track_id;
            state.clip_drag_target_valid = target_valid;

            let is_multi = state.selected_clips.contains(&(tid, ci));
            let is_single_selected = state.selected_clips.len() == 1;
            let mut moves = Vec::new();

            // For multi-clip drags, compute per-clip destination tracks
            // based on relative track index offsets from the dragged clip.
            let dragged_track_idx = state.project.tracks.iter().position(|t| t.id == tid);
            let target_track_idx = target_track_id
                .and_then(|ttid| state.project.tracks.iter().position(|t| t.id == ttid));
            let track_idx_delta: i32 = match (dragged_track_idx, target_track_idx) {
                (Some(d), Some(t)) => t as i32 - d as i32,
                _ => 0,
            };
            // Build track id list for index-based lookup
            let track_ids: Vec<u32> = state.project.tracks.iter().map(|t| t.id).collect();
            let track_types: Vec<crate::models::TrackType> =
                state.project.tracks.iter().map(|t| t.track_type).collect();

            if is_multi {
                for (&(t_id, c_idx), &old_start) in &state.drag_original_positions {
                    let new_start = (old_start + actual_delta).max(0.0);
                    // Also trigger move on cross-track even without horizontal movement
                    let cross_track = target_valid
                        && target_track_id.is_some()
                        && target_track_id.unwrap() != t_id;
                    if input.mouse_released
                        && (state.clip_drag_is_copy
                            || (new_start - old_start).abs() > 1e-9
                            || cross_track)
                    {
                        moves.push(((t_id, c_idx), old_start, new_start));
                    } else if !state.clip_drag_is_copy && !input.mouse_released {
                        // Only move originals live when NOT cloning
                        if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == t_id)
                        {
                            if let Some(clip) = track.clips.get_mut(c_idx) {
                                clip.set_start_time(new_start);
                            }
                        }
                    }
                }
            } else if input.mouse_released
                && (state.clip_drag_is_copy
                    || (snapped - input.drag_start_value).abs() > 1e-9
                    || (target_valid
                        && target_track_id.is_some()
                        && target_track_id.unwrap() != tid))
            {
                moves.push(((tid, ci), input.drag_start_value, snapped));
            } else if !state.clip_drag_is_copy && !input.mouse_released {
                // Only move originals live when NOT cloning and staying on the same track
                let same_track = target_track_id.is_none() || target_track_id == Some(tid);
                if same_track {
                    if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                        if let Some(clip) = track.clips.get_mut(ci) {
                            clip.set_start_time(snapped);
                        }
                    }
                }
            }

            // Store ghost positions for visual feedback during drag
            if !input.mouse_released {
                state.clip_drag_ghost_positions.clear();
                if is_multi {
                    // Multi-select drag: show ghost for all selected clips.
                    // Each clip gets its own destination track based on relative offset
                    // from the dragged clip, preserving the lane layout.
                    for (&(t_id, c_idx), &old_start) in &state.drag_original_positions {
                        let new_start = (old_start + actual_delta).max(0.0);
                        let ghost_tid = if track_idx_delta != 0 {
                            // Resolve destination for this clip's track
                            if let Some(src_idx) = track_ids.iter().position(|&id| id == t_id) {
                                let dst_idx = (src_idx as i32 + track_idx_delta).max(0) as usize;
                                track_ids.get(dst_idx).copied().unwrap_or(t_id)
                            } else {
                                t_id
                            }
                        } else {
                            t_id
                        };
                        state
                            .clip_drag_ghost_positions
                            .push((ghost_tid, t_id, c_idx, new_start));
                    }
                } else {
                    // Single clip: always show ghost (on target track or same track)
                    let ghost_tid = target_track_id.unwrap_or(tid);
                    state
                        .clip_drag_ghost_positions
                        .push((ghost_tid, tid, ci, snapped));
                }
            }

            if input.mouse_released && !moves.is_empty() {
                state.clip_drag_ghost_positions.clear();
                if state.clip_drag_is_copy {
                    // Ctrl+drag: instead of moving, create copies at the new positions
                    // Revert everything to original first
                    if is_multi {
                        for (&(t_id, c_idx), &old_start) in &state.drag_original_positions {
                            if let Some(track) =
                                state.project.tracks.iter_mut().find(|t| t.id == t_id)
                            {
                                if let Some(clip) = track.clips.get_mut(c_idx) {
                                    clip.set_start_time(old_start);
                                }
                            }
                        }
                    } else if let Some(track) =
                        state.project.tracks.iter_mut().find(|t| t.id == tid)
                    {
                        if let Some(clip) = track.clips.get_mut(ci) {
                            clip.set_start_time(input.drag_start_value);
                        }
                    }
                    // Build clone clips at new positions
                    let mut new_clips: Vec<(u32, crate::models::Clip)> = Vec::new();
                    for ((t_id, c_idx), _, new_start) in &moves {
                        if let Some(track) = state.project.tracks.iter().find(|t| t.id == *t_id) {
                            if let Some(orig) = track.clips.get(*c_idx).cloned() {
                                let mut new_clip = orig;
                                new_clip.set_start_time(*new_start);
                                // Resolve per-clip destination using track index offset
                                let dest_tid = if track_idx_delta != 0 && is_multi {
                                    if let Some(src_idx) =
                                        track_ids.iter().position(|&id| id == *t_id)
                                    {
                                        let dst_idx =
                                            (src_idx as i32 + track_idx_delta).max(0) as usize;
                                        track_ids.get(dst_idx).copied().unwrap_or(*t_id)
                                    } else {
                                        *t_id
                                    }
                                } else if !is_multi {
                                    target_track_id.unwrap_or(*t_id)
                                } else {
                                    *t_id
                                };
                                new_clips.push((dest_tid, new_clip));
                            }
                        }
                    }
                    if !new_clips.is_empty() {
                        state.commands.execute(
                            Box::new(crate::commands::AddClips {
                                clips: new_clips,
                                added_indices: Vec::new(),
                            }),
                            &mut state.project,
                        );
                    }
                } else {
                    // Revert live view before submitting command
                    if is_multi {
                        for (&(t_id, c_idx), &old_start) in &state.drag_original_positions {
                            if let Some(track) =
                                state.project.tracks.iter_mut().find(|t| t.id == t_id)
                            {
                                if let Some(clip) = track.clips.get_mut(c_idx) {
                                    clip.set_start_time(old_start);
                                }
                            }
                        }
                    } else if let Some(track) =
                        state.project.tracks.iter_mut().find(|t| t.id == tid)
                    {
                        if let Some(clip) = track.clips.get_mut(ci) {
                            clip.set_start_time(input.drag_start_value);
                        }
                    }

                    // Check if this is a cross-track move (using per-clip track offset)
                    let is_cross_track = track_idx_delta != 0;

                    if is_cross_track && target_valid {
                        if is_single_selected {
                            // Single clip: use the simpler single-clip command
                            let target_tid = target_track_id.unwrap();
                            let new_start = moves[0].2;
                            let old_start = moves[0].1;
                            state.commands.execute(
                                Box::new(crate::commands::MoveClipCrossTrack {
                                    src_track_id: tid,
                                    src_clip_idx: ci,
                                    dst_track_id: target_tid,
                                    old_start,
                                    new_start,
                                    dst_clip_idx: None,
                                }),
                                &mut state.project,
                            );
                            // Update selection to the new location
                            if let Some(dst_ci) = state
                                .project
                                .tracks
                                .iter()
                                .find(|t| t.id == target_tid)
                                .map(|t| t.clips.len().saturating_sub(1))
                            {
                                state.selected_clip = Some((target_tid, dst_ci));
                                state.selected_clips.clear();
                                state.selected_clips.insert((target_tid, dst_ci));
                            }
                        } else {
                            // Multiple clips: move each to its own destination track
                            // based on relative track index offset.
                            // Collect clips to remove and re-add at new positions.
                            let mut clips_to_delete: Vec<(u32, usize, crate::models::Clip)> =
                                Vec::new();
                            let mut clips_to_add: Vec<(u32, crate::models::Clip)> = Vec::new();
                            for &((t_id, c_idx), _old, new_start) in &moves {
                                let dest_tid = if let Some(src_idx) =
                                    track_ids.iter().position(|&id| id == t_id)
                                {
                                    let dst_idx =
                                        (src_idx as i32 + track_idx_delta).max(0) as usize;
                                    track_ids.get(dst_idx).copied().unwrap_or(t_id)
                                } else {
                                    t_id
                                };
                                // Verify destination track is compatible
                                let src_tt = track_ids
                                    .iter()
                                    .position(|&id| id == t_id)
                                    .and_then(|i| track_types.get(i));
                                let dst_tt = track_ids
                                    .iter()
                                    .position(|&id| id == dest_tid)
                                    .and_then(|i| track_types.get(i));
                                if src_tt == dst_tt && dest_tid != t_id {
                                    if let Some(track) =
                                        state.project.tracks.iter().find(|t| t.id == t_id)
                                    {
                                        if let Some(clip) = track.clips.get(c_idx) {
                                            clips_to_delete.push((t_id, c_idx, clip.clone()));
                                            let mut new_clip = clip.clone();
                                            new_clip.set_start_time(new_start);
                                            clips_to_add.push((dest_tid, new_clip));
                                        }
                                    }
                                }
                            }
                            if !clips_to_delete.is_empty() {
                                // Use a composite of Delete + Add for proper undo
                                let cmds: Vec<Box<dyn crate::commands::Command>> = vec![
                                    Box::new(crate::commands::DeleteClips {
                                        clips: clips_to_delete,
                                    }),
                                    Box::new(crate::commands::AddClips {
                                        clips: clips_to_add,
                                        added_indices: Vec::new(),
                                    }),
                                ];
                                state.commands.execute(
                                    Box::new(crate::commands::CompositeCommand {
                                        desc: "Move Clips Cross-Track".to_string(),
                                        cmds,
                                    }),
                                    &mut state.project,
                                );
                                // Update selection to new locations
                                state.selected_clips.clear();
                                state.selected_clip = None;
                            }
                        }
                    } else {
                        let move_cmd = crate::commands::MoveClips { moves };
                        state
                            .commands
                            .execute(Box::new(move_cmd), &mut state.project);
                    }
                }
                state.dirty = true;
            } else {
                state.dirty = true;
            }
        }
        WidgetId::ClipLeftHandle(tid, ci) => {
            let orig_end = input.drag_start_value; // fixed right edge (clip end beat)
            let orig_start = input.drag_start_value2; // original left edge (for undo)
            let orig_len = orig_end - orig_start;
            let raw_start = cursor_beat.max(0.0).min(orig_end - min_clip_len);
            let snapped_start = state
                .snap
                .snap_proximity(raw_start, snap_threshold)
                .clamp(0.0, orig_end - min_clip_len);
            let new_len = orig_end - snapped_start;
            let delta_start = snapped_start - orig_start;
            let is_multi_resize =
                state.selected_clips.len() > 1 && state.selected_clips.contains(&(tid, ci));

            if input.mouse_released {
                if is_multi_resize {
                    // Build multi-clip resize command using snapshots for orig values
                    let sel: Vec<(u32, usize)> = state.selected_clips.iter().cloned().collect();
                    let mut ops: Vec<(u32, usize, f64, f64, f64, f64)> = Vec::new();
                    for (t_id, c_idx) in &sel {
                        let c_orig_start = state
                            .drag_original_positions
                            .get(&(*t_id, *c_idx))
                            .cloned()
                            .unwrap_or(orig_start);
                        if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == *t_id)
                        {
                            if let Some(clip) = track.clips.get_mut(*c_idx) {
                                // end is fixed: orig_end = c_orig_start + orig_len
                                // now: new_start = clip.start_time(), new_len = clip.length()
                                // so orig_len = new_len + (new_start - c_orig_start)
                                let c_new_start = clip.start_time();
                                let c_new_len = clip.length();
                                let c_orig_len = c_new_len + (c_new_start - c_orig_start);
                                ops.push((
                                    *t_id,
                                    *c_idx,
                                    c_orig_start,
                                    c_orig_len,
                                    c_new_start,
                                    c_new_len,
                                ));
                                // revert live view so command.apply() sets it correctly
                                clip.set_start_time(c_orig_start);
                                clip.set_length(c_orig_len);
                            }
                        }
                    }
                    state.commands.execute(
                        Box::new(crate::commands::ResizeClips { clips: ops }),
                        &mut state.project,
                    );
                } else {
                    // Compute audio offset change for left-edge resize.
                    // For audio clips: clamp so offset never goes below 0.
                    // Read the actual live state.
                    let old_audio_off = state.drag_audio_offset_orig;
                    // Check if the clip is audio
                    let is_audio = state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == tid)
                        .and_then(|t| t.clips.get(ci))
                        .map(|c| matches!(c, crate::models::Clip::Audio(_)))
                        .unwrap_or(false);
                    // Read the actual current state set by the live update
                    let (actual_new_start, actual_new_len, actual_new_audio_off) = state
                        .project
                        .tracks
                        .iter()
                        .find(|t| t.id == tid)
                        .and_then(|t| t.clips.get(ci))
                        .map(|c| {
                            let s = c.start_time();
                            let l = c.length();
                            let off = if let crate::models::Clip::Audio(ac) = c {
                                ac.offset
                            } else {
                                0.0
                            };
                            (s, l, off)
                        })
                        .unwrap_or((snapped_start, new_len, 0.0));
                    // Revert live state before command applies it
                    if is_audio {
                        if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                            if let Some(clip) = track.clips.get_mut(ci) {
                                clip.set_start_time(orig_start);
                                clip.set_length(orig_len);
                                if let crate::models::Clip::Audio(ref mut ac) = clip {
                                    ac.offset = old_audio_off;
                                }
                            }
                        }
                    } else if let Some(track) =
                        state.project.tracks.iter_mut().find(|t| t.id == tid)
                    {
                        if let Some(clip) = track.clips.get_mut(ci) {
                            clip.set_start_time(orig_start);
                            clip.set_length(orig_len);
                        }
                    }
                    let cmd = crate::commands::ResizeClip {
                        track_id: tid,
                        clip_idx: ci,
                        old_start: orig_start,
                        old_len: orig_len,
                        new_start: actual_new_start,
                        new_len: actual_new_len,
                        old_audio_offset: if is_audio { Some(old_audio_off) } else { None },
                        new_audio_offset: if is_audio {
                            Some(actual_new_audio_off)
                        } else {
                            None
                        },
                    };
                    state.commands.execute(Box::new(cmd), &mut state.project);
                }
            } else if is_multi_resize {
                let sel: Vec<(u32, usize)> = state.selected_clips.iter().cloned().collect();
                for (t_id, c_idx) in sel {
                    if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == t_id) {
                        if let Some(clip) = track.clips.get_mut(c_idx) {
                            // drag_original_positions stores each clip's original start
                            let c_orig_start = *state
                                .drag_original_positions
                                .get(&(t_id, c_idx))
                                .unwrap_or(&clip.start_time());
                            let c_orig_end = c_orig_start + clip.length(); // end stays fixed per clip
                            let c_new_start = (c_orig_start + delta_start)
                                .max(0.0)
                                .min(c_orig_end - min_clip_len);
                            let c_new_len = (c_orig_end - c_new_start).max(min_clip_len);
                            clip.set_start_time(c_new_start);
                            clip.set_length(c_new_len);
                        }
                    }
                }
                state.dirty = true;
            } else {
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                    if let Some(clip) = track.clips.get_mut(ci) {
                        // For audio clips: adjust offset as left edge moves.
                        // Clamp so offset never goes below 0 (can't reveal audio
                        // before the start of the source file).
                        let (new_st, new_ln, new_off) =
                            if let crate::models::Clip::Audio(ref _ac) = *clip {
                                let bpm = state.project.tempo_map.bpm_at(0.0);
                                let delta_beats = snapped_start - orig_start;
                                let delta_secs = delta_beats * 60.0 / bpm;
                                let desired_offset = state.drag_audio_offset_orig + delta_secs;
                                if desired_offset < 0.0 {
                                    // Clamp: only allow dragging left until offset = 0
                                    let max_left_beats = state.drag_audio_offset_orig * bpm / 60.0;
                                    let clamped_start = (orig_start - max_left_beats).max(0.0);
                                    let clamped_len = orig_end - clamped_start;
                                    (clamped_start, clamped_len.max(min_clip_len), Some(0.0_f64))
                                } else {
                                    (snapped_start, new_len, Some(desired_offset))
                                }
                            } else {
                                (snapped_start, new_len, None)
                            };
                        clip.set_start_time(new_st);
                        clip.set_length(new_ln);
                        if let (Some(off), crate::models::Clip::Audio(ref mut ac)) = (new_off, clip)
                        {
                            ac.offset = off;
                        }
                    }
                }
                state.dirty = true;
            }
        }
        WidgetId::ClipRightHandle(tid, ci) => {
            let orig_start = input.drag_start_value2; // unchanged clip start
            let orig_len = input.drag_start_value; // original length
            let total_dx_px = input.mouse_x - input.drag_start_x;
            let raw_len = (orig_len + total_dx_px as f64 / zoom).max(min_clip_len);

            let bpm = state.project.tempo_map.bpm_at(0.0).max(1.0);

            // For audio clips: clamp length so we can't drag past the end of the
            // audio file (at the CURRENT offset). Offset never changes on right drag.
            let snapped_len = {
                let audio_max_len: Option<f64> = state
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == tid)
                    .and_then(|t| t.clips.get(ci))
                    .and_then(|c| {
                        if let crate::models::Clip::Audio(ac) = c {
                            if !ac.source_file.is_empty() {
                                if let Some((_, total_dur)) =
                                    state.waveform_cache.get(&ac.source_file)
                                {
                                    // Available audio = total_duration - current_offset
                                    let avail_secs = (*total_dur - ac.offset).max(0.0);
                                    Some(avail_secs * bpm / 60.0)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                if let Some(max_len) = audio_max_len {
                    state
                        .snap
                        .snap_proximity(raw_len, snap_threshold)
                        .max(min_clip_len)
                        .min(max_len)
                } else {
                    // MIDI clip or no waveform info — allow free resize
                    state
                        .snap
                        .snap_proximity(raw_len, snap_threshold)
                        .max(min_clip_len)
                }
            };

            let delta_len = snapped_len - orig_len;
            let is_multi_resize =
                state.selected_clips.len() > 1 && state.selected_clips.contains(&(tid, ci));

            if input.mouse_released {
                if is_multi_resize {
                    let sel: Vec<(u32, usize)> = state.selected_clips.iter().cloned().collect();
                    let mut ops: Vec<(u32, usize, f64, f64, f64, f64)> = Vec::new();
                    for (t_id, c_idx) in &sel {
                        if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == *t_id)
                        {
                            if let Some(clip) = track.clips.get_mut(*c_idx) {
                                let c_orig_len = clip.length() - delta_len;
                                let c_new_len = clip.length();
                                ops.push((
                                    *t_id,
                                    *c_idx,
                                    clip.start_time(),
                                    c_orig_len,
                                    clip.start_time(),
                                    c_new_len,
                                ));
                                clip.set_length(c_orig_len); // revert
                            }
                        }
                    }
                    state.commands.execute(
                        Box::new(crate::commands::ResizeClips { clips: ops }),
                        &mut state.project,
                    );
                } else {
                    // Audio offset never changes on right-handle drag
                    let old_offset = Some(state.drag_audio_offset_orig);
                    let cmd = crate::commands::ResizeClip {
                        track_id: tid,
                        clip_idx: ci,
                        old_start: orig_start,
                        old_len: orig_len,
                        new_start: orig_start,
                        new_len: snapped_len,
                        old_audio_offset: old_offset,
                        new_audio_offset: old_offset, // offset unchanged
                    };
                    state.commands.execute(Box::new(cmd), &mut state.project);
                }
            } else if is_multi_resize {
                let sel: Vec<(u32, usize)> = state.selected_clips.iter().cloned().collect();
                for (t_id, c_idx) in sel {
                    if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == t_id) {
                        if let Some(clip) = track.clips.get_mut(c_idx) {
                            let c_orig_len = *state
                                .drag_original_positions
                                .get(&(t_id, c_idx))
                                .unwrap_or(&clip.length());
                            let c_new_len = (c_orig_len + delta_len).max(min_clip_len);
                            clip.set_length(c_new_len);
                        }
                    }
                }
                state.dirty = true;
            } else {
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == tid) {
                    if let Some(clip) = track.clips.get_mut(ci) {
                        clip.set_length(snapped_len);
                        // Offset is NEVER changed by right-handle drag
                    }
                }
                state.dirty = true;
            }
        }
        _ => {}
    }
}

// ── Bottom panel ─────────────────────────────────────────────────────

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

fn draw_bottom_mixer(
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
    let strip_w_slim = 60i32;
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
        .filter(|t| t.track_type != crate::models::TrackType::Automation)
        .count() as i32;
    let _ = non_auto_count; // used only for legacy uniform-width fallback
                            // Compute total width accounting for slim tracks
    let total_content_w = {
        let mut tw = 12i32;
        for t in &state.project.tracks {
            if t.track_type == crate::models::TrackType::Automation {
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
        if state.project.tracks[i].track_type == crate::models::TrackType::Automation {
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
                    .filter(|t| t.track_type != crate::models::TrackType::Automation)
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
                default_value: Some(vol_gain_to_pos(0.8)),
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
                    Box::new(crate::commands::SetTrackVolume {
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
                        Box::new(crate::commands::SetTrackPan {
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
                // In slim mode: M and S side by side, centered, below pan
                let bsz = 14i32;
                let total = bsz * 2 + 4;
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
                    Box::new(crate::commands::SetTrackMute {
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
                        Box::new(crate::commands::SetTrackSolo {
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
                state.project.tracks[i].cstrip2_bypass = !bypass_on;
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
                    default_value: Some(vol_gain_to_pos(0.8)),
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

    // Out (post-everything) pair — uses post-effect RMS so it matches Pre when no FX
    let m_out_x = m_meter_x + (m_meter_bar_w as i32) * 2 + m_meter_gap + m_pair_gap;
    let m_out_l = state.meters.master_rms_post_l;
    let m_out_r = state.meters.master_rms_post_r;
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

fn draw_instrument_rack(
    canvas: &mut Canvas<Window>,
    input: &mut crate::input::InputState,
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
            crate::models::TrackType::Midi => "MIDI -> Instrument -> FX -> Out",
            crate::models::TrackType::Audio => "Audio -> FX -> Out",
            crate::models::TrackType::Automation => "Automation (no rack)",
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
    if track_type == Some(crate::models::TrackType::Automation) || sel_track_idx.is_none() {
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
        let has_vis_panel = matches!(
            plugin_name_ref.as_str(),
            "LP Filter" | "HP Filter" | "Compressor" | "EQ" | "Distortion" | "Delay" | "Limiter"
        );
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
                Box::new(crate::commands::RackSlotToggle {
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
                Box::new(crate::commands::RackSlotRemove {
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
                            Box::new(crate::commands::SetRackParam {
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
                                Box::new(crate::commands::SetRackParam {
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
                            Box::new(crate::commands::SetRackParam {
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
                                Box::new(crate::commands::SetRackParam {
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
                    t.track_type == crate::models::TrackType::Automation
                        && t.clips.iter().any(|c| {
                            if let crate::models::Clip::Automation(ac) = c {
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
                    let mut auto_track = crate::models::Track::new(
                        new_id,
                        &auto_name,
                        crate::models::TrackType::Automation,
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
                    auto_track.clips.push(crate::models::Clip::Automation(
                        crate::models::AutomationClip {
                            points: vec![
                                crate::models::AutomationPoint {
                                    time: 0.0,
                                    value: norm_val,
                                },
                                crate::models::AutomationPoint {
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
                        Box::new(crate::commands::AddTrack { track: auto_track }),
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
                            Box::new(crate::commands::SetSamplerFile {
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
                                Box::new(crate::commands::SetSamplerFile {
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
                let build_sc_choices =
                    |tracks: &Vec<crate::models::Track>, self_id: u32| -> Vec<Option<u32>> {
                        std::iter::once(None)
                            .chain(
                                tracks
                                    .iter()
                                    .filter(|t| {
                                        t.id != self_id
                                            && t.track_type != crate::models::TrackType::Automation
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
                                Box::new(crate::commands::SetRackSidechain {
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
                        Box::new(crate::commands::SetRackSidechain {
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
                crate::models::TrackType::Midi => {
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
                crate::models::TrackType::Audio => {
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
                crate::models::TrackType::Automation => {
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
                            Box::new(crate::commands::RackSlotRemove {
                                track_id,
                                slot_idx: replace_idx,
                                removed_slot: None,
                            }),
                            &mut state.project,
                        );
                        let slot = create_rack_slot_for_module(&module_name, next_id);
                        state.commands.execute(
                            Box::new(crate::commands::RackSlotAdd {
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
                        Box::new(crate::commands::RackSlotAdd {
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
                        state.project.tracks[ti].track_type == crate::models::TrackType::Midi;
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
            if t.id != self_id && t.track_type != crate::models::TrackType::Automation {
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
                    Box::new(crate::commands::SetRackSidechain {
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
fn draw_master_rack(
    canvas: &mut Canvas<Window>,
    input: &mut crate::input::InputState,
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
        let has_vis_panel = matches!(
            plugin_name.as_str(),
            "LP Filter" | "HP Filter" | "Compressor" | "EQ" | "Distortion" | "Delay" | "Limiter"
        );
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
                                Box::new(crate::commands::SetMasterRackParam {
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
                            Box::new(crate::commands::SetMasterRackParam {
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
                            Box::new(crate::commands::SetMasterRackParam {
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
        state.left_panel_tab = crate::state::LeftPanelTab::Instruments;
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

        // Out (post-effects) pair
        let rm_out_x = rm_meter_x + (rm_bar_w as i32) * 2 + rm_bar_gap + rm_pair_gap;
        let rm_out_l = state.meters.master_rms_post_l;
        let rm_out_r = state.meters.master_rms_post_r;
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

            // dB label for master volume
            let db_val = if state.master_volume_ui > 1e-6 {
                20.0 * (state.master_volume_ui as f64).log10()
            } else {
                -60.0
            };
            let vol_str = if db_val <= -60.0 {
                "Vol -∞".to_string()
            } else {
                format!("Vol {:.1}dB", db_val)
            };
            draw_pixel_label(
                canvas,
                &state.theme,
                &vol_str,
                info_x,
                rm_fader_top + 30,
                info_w,
                sdl2::pixels::Color::RGBA(140, 150, 160, 170),
            );
        }

        // Border
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(70, 80, 120, 200));
        let _ = canvas.draw_rect(Rect::new(rmx, rmy, rack_master_w as u32, rmh as u32));
    }
}
// ── Sample Browser (left panel) ─────────────────────────────────────

/// Draw the left-side sample browser panel.
/// Width = state.sample_browser_width; spans from transport bar bottom to bottom panel.
/// Tabbed left panel: Files, Clips, Instruments
fn draw_left_panel(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let bw = state.sample_browser_width;
    let bottom_h = state.bottom_panel_effective_h();
    let wh = state.window_height as i32;
    // Start the left panel below the rulers so tabs are not covered by re-drawn rulers
    let panel_y = state.track_area_top();
    let panel_h = wh - panel_y - bottom_h;

    // ── Consume all mouse presses inside the left panel so they don't bleed through
    //    to the arrangement track area drawn underneath.
    //    We do NOT set consumed=true here (widgets inside the panel do it themselves).
    //    Instead, we just check if the click is inside the panel after all widgets run.
    let mouse_in_panel = input.mouse_pressed
        && input.mouse_x < bw
        && input.mouse_y >= panel_y
        && input.mouse_y < panel_y + panel_h;

    // Panel background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(0, panel_y, bw as u32, panel_h as u32));

    // Right border
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(bw - 1, panel_y),
        sdl2::rect::Point::new(bw - 1, panel_y + panel_h),
    );

    // ── Tab bar ──
    let tab_h = 22i32;
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, panel_y, bw as u32, tab_h as u32));
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, panel_y + tab_h - 1),
        sdl2::rect::Point::new(bw, panel_y + tab_h - 1),
    );

    let tab_labels = [
        (LeftPanelTab::Files, "FILES"),
        (LeftPanelTab::Clips, "CLIPS"),
        (LeftPanelTab::Instruments, "MOD"),
        (LeftPanelTab::Themes, "THEME"),
    ];
    let tw = (bw - 4) / tab_labels.len() as i32;
    for (i, (tab, label)) in tab_labels.iter().enumerate() {
        let tx = 2 + i as i32 * tw;
        let active = state.left_panel_tab == *tab;
        let __auto_id_28 = input.next_id();
        let clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_28,
                x: tx,
                y: panel_y,
                width: tw,
                height: tab_h,
                label: label.to_string(),
                toggled: active,
                icon: ButtonIcon::None,
                hint: Some(format!("{} panel", label)),

                ..Default::default()
            },
        );
        if clicked {
            state.left_panel_tab = *tab;
        }
    }

    let content_y = panel_y + tab_h;
    let content_h = panel_h - tab_h;

    // ── Project name (editable text field at top of left panel) ──
    let name_area_h = 22i32;
    {
        let name_x = 4i32;
        let name_y = content_y + 2;
        let name_w = bw - 8;
        let name_h = 16i32;
        let (committed, new_val) = text_field(
            canvas,
            input,
            &state.theme,
            &TextFieldParams {
                id: 99,
                x: name_x,
                y: name_y,
                width: name_w,
                height: name_h,
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
                        Box::new(crate::commands::SetProjectName {
                            old_name,
                            new_name: trimmed,
                        }),
                        &mut state.project,
                    );
                    state.dirty = true;
                }
            }
        }
    }

    let content_y = content_y + name_area_h;
    let content_h = content_h - name_area_h;

    match state.left_panel_tab {
        LeftPanelTab::Files => {
            draw_left_panel_files(canvas, input, state, content_y, bw, content_h);
        }
        LeftPanelTab::Clips => {
            draw_clip_manager(canvas, input, state, content_y, bw, content_h);
        }
        LeftPanelTab::Instruments => {
            draw_left_panel_instruments(canvas, input, state, content_y, bw, content_h);
        }
        LeftPanelTab::Themes => {
            draw_left_panel_themes(canvas, input, state, content_y, bw, content_h);
        }
    }

    // Consume any unhandled press inside the panel to prevent bleed-through
    if mouse_in_panel && !input.consumed {
        input.consumed = true;
    }

    // ── Resize grabber on right edge ──
    {
        let grab_w = 6i32;
        let grab_x = bw - grab_w;
        let grab_h = panel_h;
        let grab_y = panel_y;
        let grab_hover = input.mouse_in_rect(grab_x - 2, grab_y, grab_w + 4, grab_h);
        let is_dragging = input.drag_widget == WidgetId::LeftPanelResize;

        // Draw grabber handle — centered dot pattern (clamped within panel bounds)
        let dots_y = (panel_y + panel_h / 2 - 20).max(panel_y + 4);
        let dot_col = if is_dragging || grab_hover {
            Theme::c(state.theme.accent)
        } else {
            Theme::c(state.theme.text_dim)
        };
        canvas.set_draw_color(dot_col);
        for i in 0..5 {
            let _ = canvas.fill_rect(Rect::new(bw - 3, dots_y + i * 8, 2, 3));
        }

        // Highlight strip on hover
        if grab_hover || is_dragging {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                state.theme.accent[0],
                state.theme.accent[1],
                state.theme.accent[2],
                if is_dragging { 100 } else { 50 },
            ));
            let _ = canvas.fill_rect(Rect::new(bw - 3, grab_y, 3, grab_h as u32));
        }

        // Start drag
        if grab_hover && input.mouse_pressed && input.drag_widget == WidgetId::None {
            input.drag_widget = WidgetId::LeftPanelResize;
            input.active_widget = WidgetId::LeftPanelResize;
            input.drag_start_x = input.mouse_x;
            input.drag_start_value = bw as f64;
        }

        // Live resize while dragging
        if is_dragging && input.mouse_down {
            let new_w = input.mouse_x.clamp(120, 500);
            state.sample_browser_width = new_w;
        }
    }
}

/// Files tab content — sample browser with tree view and folder navigator
fn draw_left_panel_files(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    bw: i32,
    panel_h: i32,
) {
    // ── If folder navigator is open, draw that instead ──
    if state.folder_nav_open {
        draw_folder_navigator(canvas, input, state, top, bw, panel_h);
        return;
    }

    // ── Favorites section ──
    let fav_row_h = 20i32;
    let fav_start_y = top + 4;
    let mut fav_bottom_y = fav_start_y;
    let mut fav_to_remove: Option<usize> = None;
    let mut fav_to_open: Option<usize> = None;

    if !state.favorite_folders.is_empty() {
        // "FAVORITES" header
        draw_pixel_label(
            canvas,
            &state.theme,
            "FAVORITES",
            6,
            fav_bottom_y + 3,
            bw - 12,
            Theme::c(state.theme.text_dim),
        );
        fav_bottom_y += 14;

        for (fi, fav_path) in state.favorite_folders.iter().enumerate() {
            let ry = fav_bottom_y;
            if ry + fav_row_h > top + panel_h {
                break;
            }
            let folder_name = std::path::Path::new(fav_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| fav_path.clone());

            let row_hover = input.mouse_in_rect(0, ry, bw - 20, fav_row_h);
            if row_hover {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 70, 200));
                let _ = canvas.fill_rect(Rect::new(0, ry, (bw - 20) as u32, fav_row_h as u32));
            }

            // Star icon
            draw_pixel_label(
                canvas,
                &state.theme,
                "★",
                6,
                ry + 5,
                12,
                sdl2::pixels::Color::RGBA(220, 180, 60, 230),
            );

            // Folder name
            draw_pixel_label(
                canvas,
                &state.theme,
                &folder_name,
                20,
                ry + 5,
                bw - 42,
                Theme::c(state.theme.text_secondary),
            );

            // Remove button (×)
            let rm_x = bw - 18;
            let rm_hover = input.mouse_in_rect(rm_x, ry, 16, fav_row_h);
            let rm_col = if rm_hover {
                sdl2::pixels::Color::RGBA(255, 100, 100, 255)
            } else {
                Theme::c(state.theme.text_dim)
            };
            draw_pixel_label(canvas, &state.theme, "x", rm_x + 3, ry + 5, 10, rm_col);
            if rm_hover && input.mouse_pressed {
                fav_to_remove = Some(fi);
                input.consumed = true;
            }

            // Click to open this folder in tree (only when inside panel width, not on remove btn)
            if row_hover && input.mouse_pressed && !rm_hover && !input.consumed {
                fav_to_open = Some(fi);
                input.consumed = true;
            }

            fav_bottom_y += fav_row_h;
        }

        // Separator after favorites
        fav_bottom_y += 2;
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(4, fav_bottom_y),
            sdl2::rect::Point::new(bw - 4, fav_bottom_y),
        );
        fav_bottom_y += 4;
    }

    // Handle favorite removal
    if let Some(idx) = fav_to_remove {
        state.favorite_folders.remove(idx);
    }
    // Handle favorite click — ensure the folder is in the tree, expand it, and scroll to it
    if let Some(idx) = fav_to_open {
        let fav_path = state.favorite_folders[idx].clone();
        let pb = std::path::PathBuf::from(&fav_path);
        // Add to tree if not already present (no-op if already there)
        state.add_sample_folder(pb.clone());
        // Expand the root node
        for node in state.sample_tree.iter_mut() {
            if node.path == pb {
                node.expanded = true;
                break;
            }
        }
        // Defer scroll computation to the tree-rendering pass below (needs flat_rows)
        state.sample_browser_scroll_to = Some(pb);
    }

    // ── Add Folder button ──
    let btn_y = fav_bottom_y + 2;
    let btn_w = bw - 10;
    let btn_h = 22;
    let __auto_id_29 = input.next_id();
    let add_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_29,
            x: 5,
            y: btn_y,
            width: btn_w,
            height: btn_h,
            label: "+ Add Folder".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Browse for a sample folder".into()),
            ..Default::default()
        },
    );
    if add_clicked {
        // Open the in-app folder navigator
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/"));
        state.folder_nav_path = home;
        state.refresh_folder_nav();
        state.folder_nav_open = true;
    }

    // ── Auto-play toggle + Stop button ──
    let ctrl_y = btn_y + btn_h + 3;
    let ctrl_h = 18;
    let stop_w = 22;
    let auto_w = bw - 10 - stop_w - 4;
    // Auto-play: draw a custom toggle button (not square — use button widget instead)
    let __auto_id_30 = input.next_id();
    let auto_play_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_30,
            x: 5,
            y: ctrl_y,
            width: auto_w,
            height: ctrl_h,
            label: if state.sample_auto_play {
                "Auto ✓".into()
            } else {
                "Auto".into()
            },
            toggled: state.sample_auto_play,
            icon: ButtonIcon::None,
            hint: Some("Auto-play samples on click".into()),
            ..Default::default()
        },
    );
    if auto_play_clicked {
        state.sample_auto_play = !state.sample_auto_play;
    }

    let __auto_id_31 = input.next_id();
    let stop_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_31,
            x: 5 + auto_w + 4,
            y: ctrl_y,
            width: stop_w,
            height: ctrl_h,
            label: "■".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Stop preview".into()),
            ..Default::default()
        },
    );
    if stop_clicked {
        state.sample_preview_path = None;
        state.sample_preview_trigger = false;
    }

    // ── Separator line ──
    let sep_y = ctrl_y + ctrl_h + 4;
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(4, sep_y),
        sdl2::rect::Point::new(bw - 4, sep_y),
    );

    // ── Tree view ──
    let tree_y_start = sep_y + 4;
    let row_h = 20i32;
    let available_h = panel_h - (tree_y_start - top) - 4;
    let visible_rows = available_h / row_h;

    // Flatten the tree into a displayable list of (indent, name, path, is_dir, is_expanded, tree_addr)
    // tree_addr is a Vec<usize> path to the node for toggle operations
    let mut flat_rows: Vec<FlatTreeRow> = Vec::new();
    for (ri, root_node) in state.sample_tree.iter().enumerate() {
        flatten_tree_node(root_node, 0, &mut flat_rows, vec![ri]);
    }

    let total_rows = flat_rows.len() as i32;

    // Deferred scroll-to from favourite click: find the row index of the target path
    // and scroll so it appears near the top of the view, properly clamped.
    if let Some(ref target_pb) = state.sample_browser_scroll_to.clone() {
        if let Some(row_idx) = flat_rows.iter().position(|r| &r.path == target_pb) {
            let clamped = (row_idx as i32)
                .max(0)
                .min((total_rows - visible_rows).max(0));
            state.sample_browser_scroll = clamped;
        }
        state.sample_browser_scroll_to = None;
    }

    // Scroll with mouse wheel
    if input.mouse_in_rect(0, tree_y_start, bw, available_h.max(1))
        && input.scroll_y != 0
        && !input.scroll_consumed
    {
        state.sample_browser_scroll -= input.scroll_y * 3;
        state.sample_browser_scroll = state
            .sample_browser_scroll
            .max(0)
            .min((total_rows - visible_rows).max(0));
    }

    let scroll = state.sample_browser_scroll;

    // Track which preview path is active for highlighting
    let preview_path = state.sample_preview_path.clone();

    // Collect drag path before mutable borrow
    let drag_path = state.sample_drag_path.clone();

    for (i, row) in flat_rows.iter().enumerate().skip(scroll as usize) {
        let row_idx = i as i32 - scroll;
        if row_idx >= visible_rows {
            break;
        }
        let ry = tree_y_start + row_idx * row_h;
        let indent = row.indent as i32 * 12;
        let label_x = 16 + indent; // shifted right of 14px left scrollbar

        let is_hovered =
            input.mouse_in_rect(14, ry, bw - 14, row_h) && input.active_widget == WidgetId::None;

        // Row background
        let is_previewing = preview_path.as_ref() == Some(&row.path);
        let row_bg = if is_previewing {
            sdl2::pixels::Color::RGBA(60, 110, 80, 200)
        } else if is_hovered {
            sdl2::pixels::Color::RGBA(50, 55, 70, 200)
        } else {
            sdl2::pixels::Color::RGBA(0, 0, 0, 0)
        };
        if row_bg.a > 0 {
            canvas.set_draw_color(row_bg);
            let _ = canvas.fill_rect(Rect::new(0, ry, bw as u32, row_h as u32));
        }

        // Row separator
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 40, 50, 80));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(label_x, ry + row_h - 1),
            sdl2::rect::Point::new(bw - 4, ry + row_h - 1),
        );

        if row.is_dir {
            // Folder row: arrow + name
            let arrow = if row.expanded { "▼" } else { "▶" };
            let col = Theme::c(state.theme.text_secondary);
            draw_pixel_label(canvas, &state.theme, arrow, label_x, ry + 5, 10, col);
            draw_pixel_label(
                canvas,
                &state.theme,
                &row.name,
                label_x + 12,
                ry + 5,
                bw - label_x - 38,
                col,
            );

            // Star button on folders to add/remove from favorites
            {
                let path_str = row.path.to_string_lossy().to_string();
                let is_fav = state.favorite_folders.contains(&path_str);
                let star_x = bw - 20;
                let star_hover = input.mouse_in_rect(star_x, ry, 18, row_h);
                let star_col = if is_fav {
                    sdl2::pixels::Color::RGBA(220, 180, 60, 230)
                } else if star_hover {
                    sdl2::pixels::Color::RGBA(180, 150, 60, 180)
                } else {
                    sdl2::pixels::Color::RGBA(80, 80, 90, 150)
                };
                let star_label = if is_fav { "★" } else { "☆" };
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    star_label,
                    star_x + 2,
                    ry + 5,
                    14,
                    star_col,
                );
                if star_hover && input.mouse_pressed && input.active_widget == WidgetId::None {
                    if is_fav {
                        state.favorite_folders.retain(|f| f != &path_str);
                    } else {
                        state.favorite_folders.push(path_str);
                    }
                    state.save_config_now();
                    input.consume();
                }
            }

            // Click to toggle expand/collapse
            if is_hovered
                && input.mouse_pressed
                && !input.consumed
                && input.mouse_y < state.bottom_panel_y()
                && input.active_widget == WidgetId::None
            {
                // Toggle using the tree address
                toggle_tree_node(&mut state.sample_tree, &row.addr);
            }
        } else {
            // Audio file row: play button on left, then name
            let col = sdl2::pixels::Color::RGBA(190, 190, 200, 230);

            // Preview button (left side)
            let play_btn_x = label_x;
            let play_btn_w = 18;
            let __auto_id_32 = input.next_id();
            let preview_clicked = button(
                canvas,
                input,
                &state.theme,
                &ButtonParams {
                    id: __auto_id_32,
                    x: play_btn_x,
                    y: ry + 1,
                    width: play_btn_w,
                    height: row_h - 2,
                    label: "▶".into(),
                    toggled: is_previewing,
                    icon: ButtonIcon::None,
                    hint: Some("Preview sample".into()),
                    ..Default::default()
                },
            );
            if preview_clicked {
                if is_previewing {
                    // Stop preview
                    state.sample_preview_path = None;
                    state.sample_preview_trigger = false;
                } else {
                    // Start preview
                    state.sample_preview_path = Some(row.path.clone());
                    state.sample_preview_trigger = true;
                }
            }

            // File name (right of play button)
            let name_x = label_x + play_btn_w + 2;
            draw_pixel_label(
                canvas,
                &state.theme,
                &row.name,
                name_x,
                ry + 5,
                bw - name_x - 6,
                col,
            );

            // Drag detection: click on file name area starts drag + preview
            if is_hovered
                && input.mouse_pressed
                && input.mouse_y < state.bottom_panel_y()
                && input.active_widget == WidgetId::None
            {
                // Double-click: add sample to clip manager and open in audio editor
                if input.click_type == Some(crate::input::ClickType::Double) {
                    let file_path = row.path.to_string_lossy().to_string();
                    let file_name = row.name.clone();
                    // Create an AudioClip for the clip library
                    let new_clip = crate::models::Clip::Audio(crate::models::AudioClip {
                        source_file: file_path.clone(),
                        start_time: 0.0,
                        offset: 0.0,
                        length: 4.0, // default 4 beats; will be recalculated from waveform
                        gain: 1.0,
                        name: file_name.clone(),
                        color: [100, 220, 130, 200],
                        fade_in: 0.0,
                        fade_out: 0.0,
                    });
                    // Add to clip library if not already there
                    let already = state.clip_library.iter().any(|(_, lc)| {
                        if let crate::models::Clip::Audio(ac) = lc {
                            ac.source_file == file_path
                        } else {
                            false
                        }
                    });
                    if !already {
                        state.clip_library.push((0, new_clip));
                    }
                    // Switch to Clips tab
                    state.left_panel_tab = LeftPanelTab::Clips;
                    // Also open in audio editor: open bottom panel to Audio Editor
                    // by creating/selecting a dummy audio clip in the selected track
                    // and opening the edit panel with it
                    state.push_status(format!("Added {} to clip manager", file_name));
                } else {
                    state.sample_drag_path = Some(row.path.clone());
                    // Cache the clip length in beats for the drag preview.
                    // Use waveform_cache (already loaded) when available to avoid
                    // a redundant disk read and to guarantee the same duration
                    // value that the right-handle resize will later clamp to.
                    let file_str = row.path.to_string_lossy().to_string();
                    let beats_per_sec = state.project.tempo_map.bpm_at(0.0) / 60.0;
                    let clip_len_beats =
                        if let Some((_, total_dur)) = state.waveform_cache.get(&file_str) {
                            // Cache hit — use the exact same duration the handle-resize uses
                            (*total_dur * beats_per_sec).max(0.01)
                        } else if let Ok((samples, sr)) =
                            crate::audio::load_audio(std::path::Path::new(&file_str))
                        {
                            let duration_secs = samples.len() as f64 / sr as f64;
                            (duration_secs * beats_per_sec).max(0.01)
                        } else {
                            4.0
                        };
                    state.sample_drag_len_beats = Some(clip_len_beats);
                }
                // Auto-play preview on click (if enabled)
                if state.sample_auto_play && state.sample_preview_path.as_ref() != Some(&row.path) {
                    state.sample_preview_path = Some(row.path.clone());
                    state.sample_preview_trigger = true;
                }
            }
        }
    }

    // Show placeholder when no folders loaded
    if state.sample_tree.is_empty() {
        draw_pixel_label(
            canvas,
            &state.theme,
            "No folders loaded",
            6,
            tree_y_start + 12,
            bw - 12,
            sdl2::pixels::Color::RGBA(80, 80, 90, 180),
        );
        draw_pixel_label(
            canvas,
            &state.theme,
            "Click + Add Folder",
            6,
            tree_y_start + 26,
            bw - 12,
            sdl2::pixels::Color::RGBA(70, 70, 80, 150),
        );
    }

    // ── Drag ghost: dragging a sample, follows cursor everywhere ──
    if let Some(ref drag_file) = drag_path {
        if input.mouse_down {
            let ghost_name = drag_file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Check if mouse is over the arrangement lane area — show a snap-to-track preview
            let left = state.arrangement_left_offset();
            let header_w = state.arrangement.track_header_width;
            let lane_left = left + header_w;
            let track_area_top = state.track_area_top();
            let bottom_panel_y = if state.bottom_panel_open {
                state.window_height as i32 - state.bottom_panel_effective_h()
            } else {
                state.window_height as i32
            };

            let over_arrangement = input.mouse_x > lane_left
                && input.mouse_y > track_area_top
                && input.mouse_y < bottom_panel_y;

            if over_arrangement {
                // Calculate snap position on the arrangement
                let zoom = state.arrangement.zoom_x;
                let scroll_x = state.arrangement.scroll_x;
                let scroll_y = state.arrangement.scroll_y;
                let beat = scroll_x + (input.mouse_x - lane_left) as f64 / zoom;
                let beat = (beat * 2.0).round() / 2.0; // snap to half-beat

                // Use cached clip length in beats
                let clip_len_beats = state.sample_drag_len_beats.unwrap_or(4.0);

                // Find which track the mouse is over
                let mut target_y = 0i32;
                let mut target_h = 60i32;
                let mut found_track = false;
                let mut y_acc = track_area_top - scroll_y;
                for track in state.project.tracks.iter() {
                    let th = track.height;
                    if input.mouse_y >= y_acc && input.mouse_y < y_acc + th {
                        target_y = y_acc + 2;
                        target_h = (th - 4).max(4);
                        found_track = true;
                        break;
                    }
                    y_acc += th;
                }

                // If past all tracks, show preview at the next track slot with a new-track lane hint
                if !found_track {
                    target_y = y_acc + 2;
                    target_h = 56;

                    // Draw a hint lane background for the new track
                    let lane_w = (state.window_width as i32 - lane_left).max(1);
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
                    let _ = canvas.fill_rect(Rect::new(lane_left, y_acc, lane_w as u32, 60u32));
                    // Lane separator line
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 85, 100, 100));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(lane_left, y_acc),
                        sdl2::rect::Point::new(lane_left + lane_w, y_acc),
                    );
                    // "New Track" label — detect MIDI vs Audio
                    let is_midi_file = drag_file
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| {
                            let el = e.to_lowercase();
                            el == "mid" || el == "midi"
                        })
                        .unwrap_or(false);
                    let new_track_label = if is_midi_file {
                        "+ New MIDI Track"
                    } else {
                        "+ New Audio Track"
                    };
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        new_track_label,
                        left + 6,
                        y_acc + 22,
                        header_w - 12,
                        sdl2::pixels::Color::RGBA(140, 180, 255, 150),
                    );
                }

                // Draw preview clip on the arrangement
                let preview_x = lane_left + ((beat - scroll_x) * zoom) as i32;
                let preview_w = (clip_len_beats * zoom).max(4.0) as u32;

                // Semi-transparent filled clip
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 60));
                let _ =
                    canvas.fill_rect(Rect::new(preview_x, target_y, preview_w, target_h as u32));
                // Border
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 180));
                let _ =
                    canvas.draw_rect(Rect::new(preview_x, target_y, preview_w, target_h as u32));
                // Label
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &ghost_name,
                    preview_x + 4,
                    target_y + 4,
                    preview_w.saturating_sub(8) as i32,
                    sdl2::pixels::Color::RGBA(200, 220, 255, 200),
                );
            } else {
                // Cursor-following ghost when not over arrangement
                let gx = input.mouse_x + 8;
                let gy = input.mouse_y - 12;
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(35, 65, 45, 180));
                let _ = canvas.fill_rect(Rect::new(gx, gy, 100, 24));
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(140, 255, 180, 200));
                let _ = canvas.draw_rect(Rect::new(gx, gy, 100, 24));
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    &ghost_name,
                    gx + 4,
                    gy + 7,
                    92,
                    sdl2::pixels::Color::RGBA(220, 255, 230, 240),
                );
            }
        } else if !input.mouse_down {
            // Mouse released — if over arrangement lane area, create clip
            let mut handled = false;
            if input.mouse_x > bw {
                let left = state.arrangement_left_offset();
                let header_w = state.arrangement.track_header_width;
                let lane_left = left + header_w;
                let track_area_top = state.track_area_top();

                if input.mouse_x > lane_left && input.mouse_y > track_area_top {
                    // Check if mouse is above the bottom panel (i.e. actually in the track area)
                    let bottom_panel_y = if state.bottom_panel_open {
                        state.window_height as i32 - state.bottom_panel_effective_h()
                    } else {
                        state.window_height as i32
                    };
                    if input.mouse_y < bottom_panel_y {
                        handled = true;
                        let zoom = state.arrangement.zoom_x;
                        let scroll_x = state.arrangement.scroll_x;
                        let scroll_y = state.arrangement.scroll_y;
                        let beat = scroll_x + (input.mouse_x - lane_left) as f64 / zoom;
                        let beat = (beat * 2.0).round() / 2.0;

                        let file_str = drag_file.to_string_lossy().to_string();
                        let stem = drag_file
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Sample".to_string());

                        // Detect MIDI file
                        let is_midi_drop = drag_file
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| {
                                let el = e.to_lowercase();
                                el == "mid" || el == "midi"
                            })
                            .unwrap_or(false);

                        if is_midi_drop {
                            // ── MIDI file import ──
                            let bpm = state.project.tempo_map.bpm_at(0.0);
                            match crate::models::import_midi_file(&file_str, bpm) {
                                Ok(tracks_data) => {
                                    for (track_name, mut midi_clip) in tracks_data {
                                        midi_clip.start_time = beat;
                                        midi_clip.name = track_name.clone();
                                        // Offset all note starts relative to clip start
                                        // (notes are already in beats from 0, clip sits at `beat`)

                                        let new_id = state
                                            .project
                                            .tracks
                                            .iter()
                                            .map(|t| t.id)
                                            .max()
                                            .unwrap_or(0)
                                            + 1;
                                        let mut new_track = crate::models::Track::new(
                                            new_id,
                                            &track_name,
                                            crate::models::TrackType::Midi,
                                        );
                                        // Give the track an Analog instrument
                                        new_track.rack =
                                            vec![crate::models::create_rack_slot_for_module(
                                                "Analog", 1,
                                            )];
                                        midi_clip.color = new_track.color;
                                        new_track.clips.push(crate::models::Clip::Midi(midi_clip));
                                        state.commands.execute(
                                            Box::new(crate::commands::AddTrack {
                                                track: new_track,
                                            }),
                                            &mut state.project,
                                        );
                                        // Select the new track
                                        state.selected_clip = Some((new_id, 0));
                                        state.selected_clips.clear();
                                        state.selected_clips.insert((new_id, 0));
                                        state.selected_track = Some(new_id);
                                        state.selected_tracks.clear();
                                        state.selected_tracks.insert(new_id);
                                    }
                                    state.push_status(format!("Imported MIDI: {}", stem));
                                }
                                Err(e) => {
                                    state.push_status(format!("MIDI import failed: {}", e));
                                }
                            }
                        } else {
                            // ── Audio file drop (existing logic) ──

                            // Find which existing track the mouse is over
                            let mut target_row: Option<usize> = None;
                            let mut y_acc = track_area_top - scroll_y;
                            for (ti, track) in state.project.tracks.iter().enumerate() {
                                let th = track.height;
                                if input.mouse_y >= y_acc && input.mouse_y < y_acc + th {
                                    // Only drop onto Audio tracks
                                    if track.track_type == crate::models::TrackType::Audio {
                                        target_row = Some(ti);
                                    }
                                    break;
                                }
                                y_acc += th;
                            }

                            // Check if dropping onto an existing audio clip (replace source)
                            let mut replaced_clip = false;
                            if let Some(row) = target_row {
                                let track = &state.project.tracks[row];
                                for ci in 0..track.clips.len() {
                                    if let crate::models::Clip::Audio(ref ac) = track.clips[ci] {
                                        let clip_x =
                                            lane_left + ((ac.start_time - scroll_x) * zoom) as i32;
                                        let clip_w = (ac.length * zoom) as i32;
                                        // Determine clip y position
                                        let mut cy_acc = track_area_top - scroll_y;
                                        for ti2 in 0..row {
                                            cy_acc += state.project.tracks[ti2].height;
                                        }
                                        let clip_y = cy_acc + 2;
                                        let clip_h = (track.height - 4).max(4);
                                        if input.mouse_x >= clip_x
                                            && input.mouse_x < clip_x + clip_w.max(4)
                                            && input.mouse_y >= clip_y
                                            && input.mouse_y < clip_y + clip_h
                                        {
                                            // Replace this clip's source
                                            if let crate::models::Clip::Audio(ref mut ac_mut) =
                                                state.project.tracks[row].clips[ci]
                                            {
                                                ac_mut.source_file = file_str.clone();
                                                ac_mut.name = stem.clone();
                                                // Invalidate waveform cache so it reloads
                                                state.waveform_cache.remove(&file_str);
                                            }
                                            replaced_clip = true;
                                            break;
                                        }
                                    }
                                }
                            }

                            if !replaced_clip {
                                // Calculate clip length from audio file duration.
                                // Prefer waveform_cache (same value the right-handle resize uses)
                                // to avoid any mismatch from duplicate disk reads.
                                let beats_per_sec = state.project.tempo_map.bpm_at(0.0) / 60.0;
                                let clip_len_beats = if let Some((_, total_dur)) =
                                    state.waveform_cache.get(&file_str)
                                {
                                    (*total_dur * beats_per_sec).max(0.01)
                                } else if let Ok((samples, sr)) =
                                    crate::audio::load_audio(std::path::Path::new(&file_str))
                                {
                                    let duration_secs = samples.len() as f64 / sr as f64;
                                    (duration_secs * beats_per_sec).max(0.01)
                                } else {
                                    4.0
                                };
                                let mut audio_clip =
                                    crate::models::Clip::Audio(crate::models::AudioClip {
                                        source_file: file_str,
                                        start_time: beat,
                                        offset: 0.0,
                                        length: clip_len_beats,
                                        gain: 1.0,
                                        name: stem.clone(),
                                        color: [100, 160, 255, 255],
                                        fade_in: 0.0,
                                        fade_out: 0.0,
                                    });

                                if let Some(row) = target_row {
                                    // Drop onto existing audio track (empty area)
                                    let track_id = state.project.tracks[row].id;
                                    let track_color = state.project.tracks[row].color;
                                    // Use track color for the new clip
                                    if let crate::models::Clip::Audio(ref mut ac) = audio_clip {
                                        ac.color = track_color;
                                    }
                                    let new_ci = state.project.tracks[row].clips.len();
                                    state.commands.execute(
                                        Box::new(crate::commands::AddClips {
                                            clips: vec![(track_id, audio_clip)],
                                            added_indices: vec![],
                                        }),
                                        &mut state.project,
                                    );
                                    // Auto-select the newly placed clip
                                    state.selected_clip = Some((track_id, new_ci));
                                    state.selected_clips.clear();
                                    state.selected_clips.insert((track_id, new_ci));
                                    state.selected_track = Some(track_id);
                                    state.selected_tracks.clear();
                                    state.selected_tracks.insert(track_id);
                                } else {
                                    // Create a new audio track
                                    let new_id = state
                                        .project
                                        .tracks
                                        .iter()
                                        .map(|t| t.id)
                                        .max()
                                        .unwrap_or(0)
                                        + 1;
                                    let mut new_track = crate::models::Track::new(
                                        new_id,
                                        &stem,
                                        crate::models::TrackType::Audio,
                                    );
                                    let mut clip_with_color = audio_clip;
                                    if let crate::models::Clip::Audio(ac) = &mut clip_with_color {
                                        ac.color = new_track.color
                                    }
                                    new_track.clips.push(clip_with_color);
                                    state.commands.execute(
                                        Box::new(crate::commands::AddTrack { track: new_track }),
                                        &mut state.project,
                                    );
                                    // Auto-select the new clip on the new track
                                    state.selected_clip = Some((new_id, 0));
                                    state.selected_clips.clear();
                                    state.selected_clips.insert((new_id, 0));
                                    state.selected_track = Some(new_id);
                                    state.selected_tracks.clear();
                                    state.selected_tracks.insert(new_id);
                                }
                            }
                        } // end else (audio file drop)
                    } // end if mouse_y < bottom_panel_y
                }
                // ── Drop sample onto track header area below all tracks → create MIDI track with Sampler ──
                if !handled
                    && input.mouse_x >= left
                    && input.mouse_x < lane_left
                    && input.mouse_y > track_area_top
                {
                    let bottom_panel_y = if state.bottom_panel_open {
                        state.window_height as i32 - state.bottom_panel_effective_h()
                    } else {
                        state.window_height as i32
                    };
                    if input.mouse_y < bottom_panel_y {
                        // Check mouse is below all existing tracks
                        let scroll_y = state.arrangement.scroll_y;
                        let mut y_acc = track_area_top - scroll_y;
                        for t in &state.project.tracks {
                            y_acc += t.height;
                        }
                        if input.mouse_y >= y_acc {
                            handled = true;
                            let file_str = drag_file.to_string_lossy().to_string();
                            let stem = drag_file
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Sample".to_string());

                            let new_id =
                                state.project.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                            let mut new_track = crate::models::Track::new(
                                new_id,
                                &stem,
                                crate::models::TrackType::Midi,
                            );
                            // Replace the default rack with a Sampler
                            new_track.rack = vec![crate::models::RackSlot::sampler(1)];
                            new_track.sampler_file = Some(file_str);
                            state.commands.execute(
                                Box::new(crate::commands::AddTrack { track: new_track }),
                                &mut state.project,
                            );
                            state.selected_track = Some(new_id);
                            state.selected_tracks.clear();
                            state.selected_tracks.insert(new_id);
                            state.dirty = true;
                            state
                                .push_status(format!("Created MIDI track '{}' with Sampler", stem));
                        }
                    }
                }
            }
            // Only clear drag if arrangement handled the drop;
            // otherwise let the bottom panel (sampler) handle it
            if handled {
                state.sample_drag_path = None;
                state.sample_drag_len_beats = None;
            }
        }
    }

    // ── Scrollbar ──
    if total_rows > visible_rows && visible_rows > 0 {
        let sb_w = 14i32;
        let sb_x = 0i32; // left edge of panel
        let sb_h = available_h;
        let max_scroll_val = (total_rows - visible_rows).max(1);
        let thumb_h = ((visible_rows as f32 / total_rows as f32) * sb_h as f32).max(16.0) as i32;
        let thumb_y = tree_y_start
            + ((scroll as f32 / max_scroll_val as f32) * (sb_h - thumb_h) as f32) as i32;

        // Scrollbar track
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 50, 160));
        let _ = canvas.fill_rect(Rect::new(sb_x, tree_y_start, sb_w as u32, sb_h as u32));

        let hover_sb = input.mouse_in_rect(sb_x, tree_y_start, sb_w, sb_h);
        let hover_thumb = input.mouse_in_rect(sb_x, thumb_y, sb_w, thumb_h);

        if hover_thumb && input.mouse_pressed {
            input.drag_widget = WidgetId::LeftPanelScrollbar;
            input.active_widget = WidgetId::LeftPanelScrollbar;
            input.drag_start_value = state.sample_browser_scroll as f64;
        }

        if hover_sb && !hover_thumb && input.mouse_pressed {
            let rel = (input.mouse_y - tree_y_start) as f32 / sb_h as f32;
            state.sample_browser_scroll = (rel * total_rows as f32) as i32;
            state.sample_browser_scroll = state.sample_browser_scroll.max(0).min(max_scroll_val);
            input.drag_widget = WidgetId::LeftPanelScrollbar;
            input.active_widget = WidgetId::LeftPanelScrollbar;
            input.drag_start_value = state.sample_browser_scroll as f64;
        }

        if input.drag_widget == WidgetId::LeftPanelScrollbar && input.mouse_down {
            let dy = input.mouse_y - input.drag_start_y;
            let scroll_range = sb_h - thumb_h;
            if scroll_range > 0 {
                let delta_scroll = (dy as f32 / scroll_range as f32 * max_scroll_val as f32) as i32;
                state.sample_browser_scroll = (input.drag_start_value as i32 + delta_scroll)
                    .max(0)
                    .min(max_scroll_val);
            }
        }

        let thumb_color = if input.drag_widget == WidgetId::LeftPanelScrollbar {
            sdl2::pixels::Color::RGBA(160, 170, 200, 255)
        } else if hover_thumb || hover_sb {
            sdl2::pixels::Color::RGBA(140, 150, 180, 240)
        } else {
            sdl2::pixels::Color::RGBA(100, 110, 140, 200)
        };
        canvas.set_draw_color(thumb_color);
        let _ = canvas.fill_rect(Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h as u32));
    }
}

// ── Tree helpers ──────────────────────────────────────────────────────

/// A flattened row from the sample tree for display purposes.
struct FlatTreeRow {
    name: String,
    path: std::path::PathBuf,
    is_dir: bool,
    expanded: bool,
    indent: usize,
    /// Address in the tree: indices at each level (e.g. [0, 2, 1])
    addr: Vec<usize>,
}

/// Recursively flatten a tree node into displayable rows.
fn flatten_tree_node(
    node: &crate::state::SampleTreeNode,
    indent: usize,
    out: &mut Vec<FlatTreeRow>,
    addr: Vec<usize>,
) {
    out.push(FlatTreeRow {
        name: node.name.clone(),
        path: node.path.clone(),
        is_dir: node.is_dir,
        expanded: node.expanded,
        indent,
        addr: addr.clone(),
    });
    if node.is_dir && node.expanded {
        for (ci, child) in node.children.iter().enumerate() {
            let mut child_addr = addr.clone();
            child_addr.push(ci);
            flatten_tree_node(child, indent + 1, out, child_addr);
        }
    }
}

/// Toggle expand/collapse of a tree node at the given address.
fn toggle_tree_node(tree: &mut [crate::state::SampleTreeNode], addr: &[usize]) {
    if addr.is_empty() {
        return;
    }
    let root_idx = addr[0];
    if root_idx >= tree.len() {
        return;
    }
    let mut node = &mut tree[root_idx];
    for &idx in &addr[1..] {
        if idx >= node.children.len() {
            return;
        }
        node = &mut node.children[idx];
    }
    node.expanded = !node.expanded;
}

// ── In-app folder navigator ──────────────────────────────────────────

/// Draw the in-app folder browser for selecting a sample folder.
fn draw_folder_navigator(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    bw: i32,
    panel_h: i32,
) {
    // Header: current path
    let header_h = 36;
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, bw as u32, header_h as u32));

    // Path label (truncated from the right to fit)
    let path_str = state.folder_nav_path.to_string_lossy().to_string();
    let display_path = if path_str.len() > 28 {
        format!("...{}", &path_str[path_str.len() - 28..])
    } else {
        path_str.clone()
    };
    draw_pixel_label(
        canvas,
        &state.theme,
        &display_path,
        6,
        top + 5,
        bw - 12,
        Theme::c(state.theme.text_primary),
    );

    // Back (..) button and Load button
    let btn_w = (bw - 15) / 2;
    let __auto_id_33 = input.next_id();
    let back_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_33,
            x: 5,
            y: top + 18,
            width: btn_w,
            height: 16,
            label: "← Back".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Go to parent folder".into()),
            ..Default::default()
        },
    );
    let __auto_id_34 = input.next_id();
    let load_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_34,
            x: 10 + btn_w,
            y: top + 18,
            width: btn_w,
            height: 16,
            label: "Load ✓".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Load this folder".into()),
            ..Default::default()
        },
    );
    // Cancel button (small X in top-right)
    let __auto_id_35 = input.next_id();
    let cancel_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_35,
            x: bw - 18,
            y: top + 2,
            width: 16,
            height: 14,
            label: "✕".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Cancel".into()),
            ..Default::default()
        },
    );

    if cancel_clicked {
        state.folder_nav_open = false;
    }

    if back_clicked {
        if let Some(parent) = state.folder_nav_path.parent() {
            state.folder_nav_path = parent.to_path_buf();
            state.refresh_folder_nav();
        }
    }

    if load_clicked {
        // Load this folder into the sample tree
        let folder = state.folder_nav_path.clone();
        state.add_sample_folder(folder);
        state.folder_nav_open = false;
    }

    // ── Directory listing ──
    let list_y = top + header_h + 2;
    let row_h = 18i32;
    let available_h = panel_h - header_h - 4;
    let visible_rows = available_h / row_h;

    // Scroll
    if input.mouse_in_rect(0, list_y, bw, available_h.max(1))
        && input.scroll_y != 0
        && !input.scroll_consumed
    {
        state.folder_nav_scroll -= input.scroll_y * 3;
        let total = state.folder_nav_entries.len() as i32;
        state.folder_nav_scroll = state
            .folder_nav_scroll
            .max(0)
            .min((total - visible_rows).max(0));
    }

    let entries = state.folder_nav_entries.clone();
    let scroll = state.folder_nav_scroll;

    for (i, (name, path, is_dir)) in entries.iter().enumerate().skip(scroll as usize) {
        let row_idx = i as i32 - scroll;
        if row_idx >= visible_rows {
            break;
        }
        let ry = list_y + row_idx * row_h;

        let is_hovered = input.mouse_in_rect(0, ry, bw, row_h);

        if is_hovered {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 70, 200));
            let _ = canvas.fill_rect(Rect::new(0, ry, bw as u32, row_h as u32));
        }

        // Icon + name
        let icon = if *is_dir { ">" } else { "♪" };
        let col = if *is_dir {
            Theme::c(state.theme.text_secondary)
        } else {
            sdl2::pixels::Color::RGBA(150, 150, 160, 180)
        };
        draw_pixel_label(canvas, &state.theme, icon, 6, ry + 5, 12, col);
        draw_pixel_label(canvas, &state.theme, name, 20, ry + 5, bw - 24, col);

        // Click to navigate into directory
        if is_hovered && input.mouse_pressed && *is_dir {
            state.folder_nav_path = path.clone();
            state.refresh_folder_nav();
        }
    }

    if entries.is_empty() {
        draw_pixel_label(
            canvas,
            &state.theme,
            "Empty folder",
            6,
            list_y + 12,
            bw - 12,
            sdl2::pixels::Color::RGBA(80, 80, 90, 180),
        );
    }
}

/// Modules tab — 3 categories: MIDI, Sound Generators, FX
fn draw_left_panel_instruments(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    use sdl2::rect::Rect;

    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // Header
    let header_h = 24i32;
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, header_h as u32));
    draw_pixel_label(
        canvas,
        &state.theme,
        "MODULES",
        8,
        top + 6,
        w - 16,
        Theme::c(state.theme.text_secondary),
    );
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, top + header_h - 1),
        sdl2::rect::Point::new(w, top + header_h - 1),
    );

    // Hint text
    let hint_y = top + header_h + 4;
    draw_pixel_label(
        canvas,
        &state.theme,
        "Drag into rack to add",
        8,
        hint_y,
        w - 16,
        Theme::c(state.theme.text_dim),
    );

    // Module categories with items
    struct ModuleEntry {
        icon: &'static str,
        name: &'static str,
    }
    struct ModuleCategory {
        icon: &'static str,
        label: &'static str,
        color: [u8; 4],
        items: Vec<ModuleEntry>,
    }

    let categories = vec![
        ModuleCategory {
            icon: "♪",
            label: "MIDI",
            color: [100, 160, 255, 220],
            items: vec![
                ModuleEntry {
                    icon: "♪",
                    name: "Arpeggiator",
                },
                ModuleEntry {
                    icon: "♪",
                    name: "Chord",
                },
                ModuleEntry {
                    icon: "♪",
                    name: "Transpose",
                },
                ModuleEntry {
                    icon: "♪",
                    name: "Velocity",
                },
            ],
        },
        ModuleCategory {
            icon: "~",
            label: "GENERATORS",
            color: [100, 220, 130, 220],
            items: vec![
                ModuleEntry {
                    icon: "~",
                    name: "Analog",
                },
                ModuleEntry {
                    icon: "~",
                    name: "HyperSaw",
                },
                ModuleEntry {
                    icon: "~",
                    name: "Monolith",
                },
                ModuleEntry {
                    icon: "~",
                    name: "Sampler",
                },
            ],
        },
        ModuleCategory {
            icon: "≈",
            label: "FX",
            color: [220, 160, 80, 220],
            items: vec![
                ModuleEntry {
                    icon: "≈",
                    name: "LP Filter",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "HP Filter",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "Delay",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "Reverb",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "Chorus",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "Distortion",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "Compressor",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "EQ",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "Gain",
                },
                ModuleEntry {
                    icon: "≈",
                    name: "Utility",
                },
                ModuleEntry {
                    icon: "▐",
                    name: "Limiter",
                },
                ModuleEntry {
                    icon: "⌄",
                    name: "Autoduck",
                },
            ],
        },
    ];

    let row_h = 20i32;
    let cat_h = 22i32;

    // Calculate total content height for scrollbar
    let total_content_h: i32 = categories
        .iter()
        .map(|cat| cat_h + cat.items.len() as i32 * row_h + 4)
        .sum();
    let content_top = hint_y + 14;
    let available_h = h - (content_top - top);
    let max_scroll = (total_content_h - available_h).max(0);

    // Offset content rightward when scrollbar is visible (scrollbar on left)
    let sb_visible = max_scroll > 0;
    let sb_off = if sb_visible { 14i32 } else { 0 };

    // Scroll with mouse wheel
    if input.mouse_y >= top
        && input.mouse_y < top + h
        && input.scroll_y != 0
        && !input.scroll_consumed
    {
        state.instruments_scroll = (state.instruments_scroll - input.scroll_y * 20)
            .max(0)
            .min(max_scroll);
    }

    let scroll = state.instruments_scroll;
    let mut cy = content_top - scroll;

    // Clip content to panel
    canvas.set_clip_rect(Rect::new(
        0,
        content_top,
        w as u32,
        available_h.max(0) as u32,
    ));

    for cat in &categories {
        let cat_total_h = cat_h + cat.items.len() as i32 * row_h + 4;
        if cy + cat_total_h < content_top {
            cy += cat_total_h;
            continue;
        }
        if cy > content_top + available_h {
            break;
        }

        {
            // Category header
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 32, 40, 255));
            let _ = canvas.fill_rect(Rect::new(sb_off, cy, (w - sb_off) as u32, cat_h as u32));
            let cat_col =
                sdl2::pixels::Color::RGBA(cat.color[0], cat.color[1], cat.color[2], cat.color[3]);
            draw_pixel_label(
                canvas,
                &state.theme,
                cat.icon,
                sb_off + 6,
                cy + 6,
                12,
                cat_col,
            );
            draw_pixel_label(
                canvas,
                &state.theme,
                cat.label,
                sb_off + 20,
                cy + 6,
                w - sb_off - 28,
                cat_col,
            );
            canvas.set_draw_color(Theme::c(state.theme.panel_border));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(sb_off, cy + cat_h - 1),
                sdl2::rect::Point::new(w, cy + cat_h - 1),
            );
            cy += cat_h;

            // Items
            for item in &cat.items {
                if cy > content_top + available_h {
                    break;
                }

                let is_hovered = input.mouse_in_rect(sb_off, cy, w - sb_off, row_h);
                if is_hovered {
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 70, 200));
                    let _ =
                        canvas.fill_rect(Rect::new(sb_off, cy, (w - sb_off) as u32, row_h as u32));
                }

                let item_col = sdl2::pixels::Color::RGBA(
                    cat.color[0].saturating_sub(40),
                    cat.color[1].saturating_sub(40),
                    cat.color[2].saturating_sub(40),
                    200,
                );
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    item.icon,
                    sb_off + 14,
                    cy + 5,
                    12,
                    item_col,
                );
                draw_pixel_label(
                    canvas,
                    &state.theme,
                    item.name,
                    sb_off + 28,
                    cy + 5,
                    w - sb_off - 36,
                    sdl2::pixels::Color::RGBA(190, 195, 210, 230),
                );

                // Start drag on mouse press
                if is_hovered && input.mouse_pressed && input.active_widget == WidgetId::None {
                    state.module_drag = Some(item.name.to_string());
                }

                // Separator
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 40, 50, 60));
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(sb_off + 14, cy + row_h - 1),
                    sdl2::rect::Point::new(w - 6, cy + row_h - 1),
                );

                cy += row_h;
            }

            cy += 4; // spacing between categories
        } // end category block
    }

    canvas.set_clip_rect(None);

    // Scrollbar on LEFT edge
    if max_scroll > 0 {
        let sb_x = 0i32;
        let sb_w = 14i32;
        let sb_h = available_h;
        let thumb_frac = (available_h as f32 / total_content_h as f32).clamp(0.05, 1.0);
        let thumb_h = (thumb_frac * sb_h as f32).max(16.0) as i32;
        let scroll_frac = scroll as f32 / max_scroll as f32;
        let thumb_y = content_top + (scroll_frac * (sb_h - thumb_h) as f32) as i32;

        // Track
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 32, 40, 160));
        let _ = canvas.fill_rect(Rect::new(
            sb_x,
            content_top,
            sb_w as u32,
            sb_h.max(0) as u32,
        ));
        // Thumb
        let hover_sb = input.mouse_in_rect(sb_x, content_top, sb_w, sb_h);
        let thumb_color = if hover_sb {
            sdl2::pixels::Color::RGBA(140, 150, 180, 240)
        } else {
            sdl2::pixels::Color::RGBA(100, 110, 140, 200)
        };
        canvas.set_draw_color(thumb_color);
        let _ = canvas.fill_rect(Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h as u32));

        // Click on scrollbar track to jump
        if hover_sb && input.mouse_pressed {
            let rel = (input.mouse_y - content_top) as f32 / sb_h as f32;
            state.instruments_scroll = (rel * max_scroll as f32) as i32;
            state.instruments_scroll = state.instruments_scroll.max(0).min(max_scroll);
            input.active_widget = WidgetId::Auto(87100);
            input.drag_widget = WidgetId::Auto(87100);
            input.drag_start_value = state.instruments_scroll as f64;
        }

        if input.drag_widget == WidgetId::Auto(87100) && input.mouse_down {
            let dy = input.mouse_y - input.drag_start_y;
            let scroll_range = sb_h - thumb_h;
            if scroll_range > 0 {
                let delta = (dy as f32 / scroll_range as f32 * max_scroll as f32) as i32;
                state.instruments_scroll = (input.drag_start_value as i32 + delta)
                    .max(0)
                    .min(max_scroll);
            }
        }
    }

    // Clear drag if mouse released outside any drop zone
    if input.mouse_released && state.module_drag.is_some() {
        // The drop is handled in draw_track_rack; if we still have a drag here, it wasn't dropped
        // on a valid target, so we'll let the rack code handle clearing it
    }
}

/// Themes tab — pick a color scheme
fn draw_left_panel_themes(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    use sdl2::rect::Rect;

    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // Header
    let header_h = 24i32;
    canvas.set_draw_color(Theme::c(state.theme.bg_dark));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, header_h as u32));
    draw_pixel_label(
        canvas,
        &state.theme,
        "THEMES",
        8,
        top + 6,
        w - 16,
        Theme::c(state.theme.text_secondary),
    );
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, top + header_h - 1),
        sdl2::rect::Point::new(w, top + header_h - 1),
    );

    let list_top = top + header_h + 4;
    let item_h = 52i32;
    let padding = 6i32;
    let all_themes = Theme::all_themes();
    let total_content_h = all_themes.len() as i32 * (item_h + 4);
    let available_h = h - header_h - 8;

    // Offset content rightward when scrollbar is visible (scrollbar on left)
    let sb_visible = total_content_h > available_h && available_h > 10;
    let sb_offset = if sb_visible { 14 } else { 0 };

    // Handle scroll in themes panel
    if input.mouse_in_rect(0, top, w, h) && input.scroll_y != 0 && !input.scroll_consumed {
        state.theme_scroll -= input.scroll_y * 20;
        state.theme_scroll = state
            .theme_scroll
            .max(0)
            .min((total_content_h - available_h).max(0));
        input.scroll_y = 0;
    }

    // Clip drawing to the content area below the header
    canvas.set_clip_rect(Rect::new(0, list_top, w as u32, available_h as u32));

    for (i, theme) in all_themes.iter().enumerate() {
        let iy = list_top + i as i32 * (item_h + 4) - state.theme_scroll;
        if iy + item_h < list_top {
            continue; // above visible area
        }
        if iy > top + h {
            break;
        }

        let is_current = theme.name == state.theme.name;
        let pad = padding + sb_offset;
        let hover = input.mouse_in_rect(pad, iy, w - pad - padding, item_h);

        // Item background
        let item_bg = if is_current {
            let a = state.theme.accent;
            sdl2::pixels::Color::RGBA(a[0] / 3, a[1] / 3, a[2] / 3, 255)
        } else if hover {
            sdl2::pixels::Color::RGBA(50, 50, 55, 255)
        } else {
            sdl2::pixels::Color::RGBA(35, 35, 38, 255)
        };
        canvas.set_draw_color(item_bg);
        let _ = canvas.fill_rect(Rect::new(
            pad,
            iy,
            (w - pad - padding) as u32,
            item_h as u32,
        ));

        // Selection border for current theme
        if is_current {
            canvas.set_draw_color(Theme::c(state.theme.accent));
            let _ = canvas.draw_rect(Rect::new(
                pad,
                iy,
                (w - pad - padding) as u32,
                item_h as u32,
            ));
        }

        // Theme name
        let text_col = if is_current {
            Theme::c(state.theme.text_primary)
        } else {
            sdl2::pixels::Color::RGBA(180, 180, 190, 255)
        };
        draw_pixel_label(
            canvas,
            &state.theme,
            &theme.name,
            pad + 6,
            iy + 4,
            w - pad - padding - 12,
            text_col,
        );

        // Mini color preview — show a row of color swatches
        let swatch_y = iy + 18;
        let swatch_h = 10i32;
        let swatch_w = 14i32;
        let swatch_gap = 2i32;
        let swatches = [
            theme.bg_dark,
            theme.panel_bg,
            theme.accent,
            theme.clip_midi,
            theme.clip_audio,
            theme.note_on,
            theme.play_color,
            theme.record_color,
        ];
        let swatch_x_start = pad + 6;
        for (si, sw) in swatches.iter().enumerate() {
            let sx = swatch_x_start + si as i32 * (swatch_w + swatch_gap);
            if sx + swatch_w > w - padding {
                break;
            }
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(sw[0], sw[1], sw[2], 255));
            let _ = canvas.fill_rect(Rect::new(sx, swatch_y, swatch_w as u32, swatch_h as u32));
        }

        // Mini arrangement preview (bg + grid + clip bars)
        let prev_y = iy + 32;
        let prev_h = 16i32;
        let prev_w = w - pad - padding - 12;
        let prev_x = pad + 6;

        // Background
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            theme.track_bg[0],
            theme.track_bg[1],
            theme.track_bg[2],
            255,
        ));
        let _ = canvas.fill_rect(Rect::new(prev_x, prev_y, prev_w as u32, prev_h as u32));

        // Grid lines
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            theme.grid_line[0],
            theme.grid_line[1],
            theme.grid_line[2],
            255,
        ));
        for g in 0..8 {
            let gx = prev_x + g * prev_w / 8;
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(gx, prev_y),
                sdl2::rect::Point::new(gx, prev_y + prev_h),
            );
        }

        // Fake clip bars
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            theme.clip_midi[0],
            theme.clip_midi[1],
            theme.clip_midi[2],
            200,
        ));
        let _ = canvas.fill_rect(Rect::new(prev_x + 2, prev_y + 2, (prev_w / 3) as u32, 5));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            theme.clip_audio[0],
            theme.clip_audio[1],
            theme.clip_audio[2],
            200,
        ));
        let _ = canvas.fill_rect(Rect::new(
            prev_x + prev_w / 3 + 4,
            prev_y + 9,
            (prev_w / 4) as u32,
            5,
        ));

        // Playhead
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(
            theme.playhead[0],
            theme.playhead[1],
            theme.playhead[2],
            200,
        ));
        let ph_x = prev_x + prev_w / 4;
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(ph_x, prev_y),
            sdl2::rect::Point::new(ph_x, prev_y + prev_h),
        );

        // Click to apply theme (guard: ignore clicks in the bottom panel zone)
        if hover
            && input.mouse_pressed
            && input.mouse_y < state.bottom_panel_y()
            && input.active_widget == WidgetId::None
        {
            state.set_theme_by_name(&theme.name.clone());
        }
    }

    // Remove clip rect before drawing scrollbar (which may extend into header)
    canvas.set_clip_rect(None);

    // ── Scrollbar indicator ──
    if total_content_h > available_h && available_h > 10 {
        let sb_w = 14i32;
        let sb_x = 0i32; // place on the LEFT side for consistency
        let sb_h = available_h;
        let sb_y = list_top;
        let thumb_frac = (available_h as f32 / total_content_h as f32).clamp(0.05, 1.0);
        let thumb_h = (thumb_frac * sb_h as f32) as i32;
        let scroll_frac = state.theme_scroll as f32 / (total_content_h - available_h).max(1) as f32;
        let thumb_y = sb_y + (scroll_frac * (sb_h - thumb_h) as f32) as i32;
        // Track
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 30, 35, 160));
        let _ = canvas.fill_rect(Rect::new(sb_x, sb_y, sb_w as u32, sb_h as u32));

        // Thumb interaction
        let hover_sb = input.mouse_in_rect(sb_x, sb_y, sb_w, sb_h);
        let hover_thumb = input.mouse_in_rect(sb_x, thumb_y, sb_w, thumb_h.max(8));

        if hover_thumb && input.mouse_pressed {
            input.active_widget = WidgetId::Auto(87200);
            input.drag_widget = WidgetId::Auto(87200);
            input.drag_start_value = state.theme_scroll as f64;
        }

        if hover_sb && !hover_thumb && input.mouse_pressed {
            let rel = (input.mouse_y - sb_y) as f32 / sb_h as f32;
            state.theme_scroll = (rel * (total_content_h - available_h) as f32) as i32;
            state.theme_scroll = state
                .theme_scroll
                .max(0)
                .min((total_content_h - available_h).max(0));
            input.active_widget = WidgetId::Auto(87200);
            input.drag_widget = WidgetId::Auto(87200);
            input.drag_start_value = state.theme_scroll as f64;
        }

        if input.drag_widget == WidgetId::Auto(87200) && input.mouse_down {
            let dy = input.mouse_y - input.drag_start_y;
            let max_scroll = (total_content_h - available_h).max(1);
            let scroll_range = sb_h - thumb_h.max(8);
            if scroll_range > 0 {
                let delta = (dy as f32 / scroll_range as f32 * max_scroll as f32) as i32;
                state.theme_scroll = (input.drag_start_value as i32 + delta)
                    .max(0)
                    .min(max_scroll);
            }
        }

        // Thumb
        let thumb_col = if input.drag_widget == WidgetId::Auto(87200) {
            sdl2::pixels::Color::RGBA(160, 170, 200, 255)
        } else if hover_thumb || hover_sb {
            sdl2::pixels::Color::RGBA(140, 150, 180, 240)
        } else {
            sdl2::pixels::Color::RGBA(100, 110, 140, 220)
        };
        canvas.set_draw_color(thumb_col);
        let _ = canvas.fill_rect(Rect::new(sb_x, thumb_y, sb_w as u32, thumb_h.max(8) as u32));
    }
}

// Draw order = back → front:
//   1. Transport bar (includes oscilloscope)
//   2. Timeline ruler (includes loop handles)
//   3. Track headers + lanes (behind everything)
//   4. Bottom panel (on top of track area)
//   5. OVERLAYS (dropdowns, tooltips) — always drawn last so they appear in front

/// Project manager startup screen — shown on first launch.
/// User can create a new project or open a recent one.
pub fn draw_project_manager(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
    use sdl2::rect::Rect;

    canvas.set_viewport(None);
    canvas.set_clip_rect(None);

    let w = state.window_width as i32;
    let h = state.window_height as i32;

    // When the project browser overlay or new-project popup is open, the
    // underlying card must not react to input — use a dead InputState so
    // buttons are drawn but inert.
    let mut dead_input = InputState {
        mouse_x: input.mouse_x,
        mouse_y: input.mouse_y,
        ..Default::default()
    };
    let card_input = if state.project_browser_open || state.new_project_popup_open {
        &mut dead_input
    } else {
        &mut *input
    };

    // Full-screen dark background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 20, 26, 255));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    // Central card
    let card_w = 480i32;
    let card_h = 520i32;
    let card_x = (w - card_w) / 2;
    let card_y = (h - card_h) / 2;

    // Card shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
    let _ = canvas.fill_rect(Rect::new(
        card_x + 4,
        card_y + 4,
        card_w as u32,
        card_h as u32,
    ));

    // Card background
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(card_x, card_y, card_w as u32, card_h as u32));

    // Card border
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_rect(Rect::new(card_x, card_y, card_w as u32, card_h as u32));

    // Accent stripe at top
    let accent = Theme::c(state.theme.accent);
    canvas.set_draw_color(accent);
    let _ = canvas.fill_rect(Rect::new(card_x, card_y, card_w as u32, 3));

    // Title — "EDEN"
    let title_scale = 4i32;
    let title_text = "EDEN";
    let char_w = 6 * title_scale;
    let title_total_w = title_text.len() as i32 * char_w;
    let title_x = card_x + (card_w - title_total_w) / 2;
    let title_y = card_y + 22;
    draw_pixel_label_scaled(
        canvas,
        &state.theme,
        title_text,
        title_x,
        title_y,
        card_w,
        accent,
        title_scale,
    );

    // Subtitle
    draw_pixel_label(
        canvas,
        &state.theme,
        "Digital Audio Workstation",
        card_x + 20,
        card_y + 22 + char_w + 6,
        card_w - 40,
        sdl2::pixels::Color::RGBA(120, 125, 140, 220),
    );

    // Divider
    let div_y = card_y + 22 + char_w + 28;
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 70, 200));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(card_x + 20, div_y),
        sdl2::rect::Point::new(card_x + card_w - 20, div_y),
    );

    // "Continue" button — only shown when a project is already open
    let has_project = !state.project.tracks.is_empty() || state.last_save_path.is_some();
    let btn_w = card_w - 60;
    let btn_x = card_x + 30;
    let mut btn_y = div_y + 20;

    if has_project {
        let proj_name = if state.project.name.is_empty() {
            "Untitled Project".to_string()
        } else {
            state.project.name.clone()
        };
        let continue_label = format!("▶ Continue  \"{}\"", proj_name);
        let __auto_id_36 = card_input.next_id();
        let continue_clicked = button(
            canvas,
            card_input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_36,
                x: btn_x,
                y: btn_y,
                width: btn_w,
                height: 36,
                label: continue_label,
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Return to the current project".into()),
                ..Default::default()
            },
        );
        if continue_clicked {
            state.mode = crate::state::AppMode::Arrangement;
        }
        // Accent highlight on the continue button border
        canvas.set_draw_color(Theme::c(state.theme.accent));
        let _ = canvas.draw_rect(Rect::new(btn_x - 1, btn_y - 1, (btn_w + 2) as u32, 38));
        btn_y += 48;
    }

    // "New Project" button
    let __auto_id_37 = card_input.next_id();
    let new_clicked = button(
        canvas,
        card_input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_37,
            x: btn_x,
            y: btn_y,
            width: btn_w,
            height: 36,
            label: "+ New Project".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Create a blank new project".into()),
            ..Default::default()
        },
    );
    if new_clicked {
        state.new_project_name_buffer = "Untitled Project".to_string();
        state.new_project_popup_open = true;
    }

    // "Open Project…" button (launches folder navigator style: opens last used dir or home)
    let open_y = btn_y + 46;
    let __auto_id_38 = card_input.next_id();
    let open_clicked = button(
        canvas,
        card_input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_38,
            x: btn_x,
            y: open_y,
            width: btn_w,
            height: 28,
            label: "Open Project…".into(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: Some("Open a project file from disk".into()),
            ..Default::default()
        },
    );
    if open_clicked {
        // Open the project file browser overlay
        state.project_browser_open = true;
        state.refresh_project_browser();
    }

    // "Recent Projects" header
    let recent_y = open_y + 46;
    draw_pixel_label(
        canvas,
        &state.theme,
        "Recent Projects",
        card_x + 20,
        recent_y,
        card_w - 40,
        sdl2::pixels::Color::RGBA(130, 135, 155, 200),
    );

    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 70, 200));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(card_x + 20, recent_y + 14),
        sdl2::rect::Point::new(card_x + card_w - 20, recent_y + 14),
    );

    // List recent projects
    let item_h = 28i32;
    let list_top = recent_y + 20;
    let max_items = 8usize;
    let recents = state.recent_projects.clone();

    if recents.is_empty() {
        draw_pixel_label(
            canvas,
            &state.theme,
            "No recent projects",
            card_x + 20,
            list_top + 8,
            card_w - 40,
            sdl2::pixels::Color::RGBA(70, 75, 90, 180),
        );
    } else {
        for (i, path) in recents.iter().take(max_items).enumerate() {
            let iy = list_top + i as i32 * item_h;
            let hover = card_input.mouse_in_rect(card_x + 10, iy, card_w - 20, item_h - 2);

            // Row background
            if hover {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 70, 180));
                let _ = canvas.fill_rect(Rect::new(
                    card_x + 10,
                    iy,
                    (card_w - 20) as u32,
                    (item_h - 2) as u32,
                ));
            }

            // File name (bold)
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(path.as_str());
            draw_pixel_label(
                canvas,
                &state.theme,
                file_name,
                card_x + 16,
                iy + 4,
                card_w - 60,
                Theme::c(state.theme.text_primary),
            );

            // Directory path (dim, below name)
            let dir_str = std::path::Path::new(path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("");
            draw_pixel_label(
                canvas,
                &state.theme,
                dir_str,
                card_x + 16,
                iy + 16,
                card_w - 60,
                sdl2::pixels::Color::RGBA(80, 85, 100, 180),
            );

            // Click to open
            if hover && card_input.mouse_pressed && card_input.active_widget == WidgetId::None {
                let path_clone = path.clone();
                match state.load_project(&path_clone) {
                    Ok(()) => {
                        state.mode = crate::state::AppMode::Arrangement;
                    }
                    Err(e) => {
                        state.push_status(format!("Failed to load: {}", e));
                        state.mode = crate::state::AppMode::Arrangement;
                    }
                }
            }
        }
    }

    // Version info at bottom of card
    draw_pixel_label(
        canvas,
        &state.theme,
        "Eden DAW  v0.1",
        card_x + 20,
        card_y + card_h - 18,
        card_w - 40,
        sdl2::pixels::Color::RGBA(60, 65, 80, 180),
    );

    // ── Project file browser overlay (uses generic file browser) ────
    if state.project_browser_open {
        // Redirect to generic file browser if not already open
        if !state.file_browser_open {
            let start_path = state.project_browser_path.clone();
            state.open_file_browser(
                crate::state::FileBrowserCaller::OpenProject,
                "Open Project",
                ".eden.json",
                false,
                Some(&start_path),
            );
        }
        if state.file_browser_open {
            if let Some(selected) = draw_file_browser_popup(canvas, input, state) {
                // Load the selected project file
                let path_str = selected.to_string_lossy().to_string();
                match state.load_project(&path_str) {
                    Ok(()) => {
                        state.project_browser_open = false;
                        state.mode = crate::state::AppMode::Arrangement;
                    }
                    Err(e) => {
                        state.push_status(format!("Failed to load: {}", e));
                    }
                }
            }
            // If file browser was closed without selection, close project browser too
            if !state.file_browser_open {
                state.project_browser_open = false;
            }
        }
    }

    // ── New-project name popup (drawn on top of everything) ──
    if state.new_project_popup_open {
        draw_new_project_popup(canvas, input, state);
    }
}

pub fn draw_arrangement(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    // Reset viewport and clip rect to full window to prevent stale state from prior frames
    canvas.set_viewport(None);
    canvas.set_clip_rect(None);

    // ── Layer state machine ───────────────────────────────────────────────────
    // Determine which UI layer owns input this frame. Layers strictly shadow
    // each other: background layers receive a dead (no-event) InputState so
    // they cannot react to clicks, scroll, or keyboard shortcuts.
    let layer = state.active_layer();

    // Build a dead input for background draws: preserves mouse position (so
    // hover highlights still track the cursor visually) but has no events.
    let mut dead_input = InputState {
        mouse_x: input.mouse_x,
        mouse_y: input.mouse_y,
        mouse_down: false,
        ..Default::default()
    };

    // Helper: pick which input to give to a background draw function.
    // Block input when an overlay is active (popups, dialogs, etc.).
    macro_rules! bg {
        ($inp:expr) => {
            if layer > crate::state::UiLayer::Base {
                &mut dead_input
            } else {
                $inp
            }
        };
    }

    // Additionally block arranger track/header input when the mouse is in the
    // bottom panel area so clicks don't bleed through.
    let mouse_below_panel = state.bottom_panel_open && input.mouse_y >= state.bottom_panel_y();
    macro_rules! bg_track {
        ($inp:expr) => {
            if layer > crate::state::UiLayer::Base || mouse_below_panel {
                &mut dead_input
            } else {
                $inp
            }
        };
    }

    // Pre-consume clicks inside the transport snap dropdown popup area so
    // widgets drawn before the dropdown overlay can't steal the click.
    // The dropdown overlay (draw_overlays) processes item-click via
    // input.mouse_pressed directly, so consumed=true doesn't block it.
    if state.dropdown_open_id == 200 && layer == crate::state::UiLayer::Base {
        let dd_x = 424i32;
        let dd_y = 10i32;
        let dd_w = 52i32;
        let dd_h = 28i32;
        let popup_h = SNAP_RESOLUTIONS.len() as i32 * dd_h;
        if input.mouse_in_rect(dd_x, dd_y, dd_w, dd_h + popup_h) {
            input.consumed = true;
        }
    }

    // Keep widget ID counters in lock-step between `input` and `dead_input`.
    // Depending on mouse position, different draw calls receive one or the
    // other InputState.  If we don't sync after every draw, the bottom-panel
    // widget IDs shift when the mouse crosses the panel divider — breaking
    // drag_widget / active_widget comparisons mid-drag.
    macro_rules! sync_counters {
        () => {{
            let m = input.widget_counter.max(dead_input.widget_counter);
            input.widget_counter = m;
            dead_input.widget_counter = m;
        }};
    }

    // Layer 0 — background elements (blocked when any overlay is active)
    draw_transport(canvas, bg!(input), state);
    sync_counters!();
    // Block loop/timeline ruler input if the mouse is over the bottom panel handle,
    // so the handle intercepts clicks before the ruler does.
    {
        let total_h = state.window_height as i32;
        let panel_h = state.bottom_panel_effective_h();
        let panel_y = total_h - panel_h;
        let handle_h = state.bottom_panel_handle_h();
        let w = state.window_width as i32;
        let over_handle = input.mouse_in_rect(0, panel_y, w, handle_h + 4);
        if over_handle {
            draw_loop_ruler(canvas, &mut dead_input, state);
            sync_counters!();
            draw_timeline_ruler(canvas, &mut dead_input, state);
            sync_counters!();
        } else {
            draw_loop_ruler(canvas, bg_track!(input), state);
            sync_counters!();
            draw_timeline_ruler(canvas, bg_track!(input), state);
            sync_counters!();
        }
    }
    draw_mode_tabs(canvas, bg!(input), state);
    sync_counters!();
    if state.sample_browser_open {
        draw_left_panel(canvas, bg!(input), state);
        sync_counters!();
    }
    // Reset viewport/clip_rect before track rendering
    canvas.set_viewport(None);
    canvas.set_clip_rect(None);
    draw_track_headers(canvas, bg_track!(input), state);
    sync_counters!();
    draw_track_lanes(canvas, bg_track!(input), state);
    sync_counters!();

    // Re-draw rulers on top so they layer over the scrollbar that extends upward
    canvas.set_viewport(None);
    canvas.set_clip_rect(None);
    draw_loop_ruler(canvas, &mut dead_input, state);
    sync_counters!();
    draw_timeline_ruler(canvas, &mut dead_input, state);
    sync_counters!();

    // ── Drag-drop handlers (background layer only) ────────────────────────────
    if layer == crate::state::UiLayer::Base {
        // ── Clip sidebar drag → arrangement drop ──
        if let Some((src_track_id, src_clip_idx)) = state.clip_sidebar_drag {
            let left = state.arrangement_left_offset();
            let header_w = state.arrangement.track_header_width;
            let lane_left = left + header_w;
            let track_top = state.track_area_top();
            let zoom = state.arrangement.zoom_x;
            let scroll_x = state.arrangement.scroll_x;
            let scroll_y = state.arrangement.scroll_y;

            // Find which track row the mouse is over
            let mut target_row: Option<usize> = None;
            let mut y_acc = track_top - scroll_y;
            for (ti, track) in state.project.tracks.iter().enumerate() {
                let th = track.height;
                if input.mouse_y >= y_acc && input.mouse_y < y_acc + th {
                    target_row = Some(ti);
                    break;
                }
                y_acc += th;
            }

            if input.mouse_down {
                // Draw ghost at mouse position when over arrangement
                if input.mouse_x > lane_left && input.mouse_y > track_top {
                    if let Some(row) = target_row {
                        let beat = scroll_x + (input.mouse_x - lane_left) as f64 / zoom;
                        let snapped = (beat * 2.0).round() / 2.0;
                        let clip_info = state
                            .project
                            .tracks
                            .iter()
                            .find(|t| t.id == src_track_id)
                            .and_then(|t| t.clips.get(src_clip_idx));
                        let clip_len = clip_info
                            .map(|c| match c {
                                crate::models::Clip::Midi(m) => m.length,
                                crate::models::Clip::Audio(a) => a.length,
                                crate::models::Clip::Automation(a) => a.length,
                            })
                            .unwrap_or(4.0);
                        // Check type compatibility
                        let clip_type_ok = clip_info
                            .map(|c| {
                                let ct = match c {
                                    crate::models::Clip::Midi(_) => crate::models::TrackType::Midi,
                                    crate::models::Clip::Audio(_) => {
                                        crate::models::TrackType::Audio
                                    }
                                    crate::models::Clip::Automation(_) => {
                                        crate::models::TrackType::Automation
                                    }
                                };
                                ct == state.project.tracks[row].track_type
                            })
                            .unwrap_or(false);
                        let gx = lane_left + ((snapped - scroll_x) * zoom) as i32;
                        let gw = (clip_len * zoom) as i32;
                        // Compute target track y
                        let mut gy = track_top - scroll_y;
                        for ti in 0..row {
                            gy += state.project.tracks[ti].height;
                        }
                        let gh = state.project.tracks[row].height;
                        // Set clip rect to prevent ghost from overlapping headers
                        canvas.set_clip_rect(Rect::new(
                            lane_left,
                            track_top,
                            (state.window_width as i32 - lane_left) as u32,
                            state.track_area_height() as u32,
                        ));
                        if clip_type_ok {
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 80));
                            let _ =
                                canvas.fill_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 200));
                            let _ =
                                canvas.draw_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                        } else {
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 60, 60, 60));
                            let _ =
                                canvas.fill_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 60, 60, 180));
                            let _ =
                                canvas.draw_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                        }
                        canvas.set_clip_rect(None);
                    }
                }
            } else {
                // Mouse released — drop the clip
                if input.mouse_x > lane_left && input.mouse_y > track_top {
                    if let Some(row) = target_row {
                        let beat = scroll_x + (input.mouse_x - lane_left) as f64 / zoom;
                        let snapped = (beat * 2.0).round() / 2.0;

                        // Clone the source clip, but only if type matches target track
                        if let Some(src_clip) = state
                            .project
                            .tracks
                            .iter()
                            .find(|t| t.id == src_track_id)
                            .and_then(|t| t.clips.get(src_clip_idx))
                            .cloned()
                        {
                            let clip_track_type = match &src_clip {
                                crate::models::Clip::Midi(_) => crate::models::TrackType::Midi,
                                crate::models::Clip::Audio(_) => crate::models::TrackType::Audio,
                                crate::models::Clip::Automation(_) => {
                                    crate::models::TrackType::Automation
                                }
                            };
                            if clip_track_type == state.project.tracks[row].track_type {
                                let mut new_clip = src_clip;
                                match &mut new_clip {
                                    crate::models::Clip::Midi(m) => m.start_time = snapped,
                                    crate::models::Clip::Audio(a) => a.start_time = snapped,
                                    crate::models::Clip::Automation(a) => a.start_time = snapped,
                                }
                                let track_id = state.project.tracks[row].id;
                                state.commands.execute(
                                    Box::new(crate::commands::AddClips {
                                        clips: vec![(track_id, new_clip)],
                                        added_indices: vec![],
                                    }),
                                    &mut state.project,
                                );
                            } else {
                                state.push_status(format!(
                                    "Cannot drop {} clip on {} track",
                                    match clip_track_type {
                                        crate::models::TrackType::Midi => "MIDI",
                                        crate::models::TrackType::Audio => "Audio",
                                        crate::models::TrackType::Automation => "Auto",
                                    },
                                    match state.project.tracks[row].track_type {
                                        crate::models::TrackType::Midi => "MIDI",
                                        crate::models::TrackType::Audio => "Audio",
                                        crate::models::TrackType::Automation => "Auto",
                                    },
                                ));
                            }
                        }
                    }
                }
                // Always clear the drag state on release
                state.clip_sidebar_drag = None;
            }
        }

        // ── Library clip drag → arrangement drop ──────────────────────────────────
        if state.library_drag_clip.is_some() {
            let left = state.arrangement_left_offset();
            let header_w = state.arrangement.track_header_width;
            let lane_left = left + header_w;
            let track_top = state.track_area_top();
            let zoom = state.arrangement.zoom_x;
            let scroll_x = state.arrangement.scroll_x;

            // Gather track y ranges
            let mut track_rows: Vec<(i32, i32, u32)> = Vec::new(); // (y_top, y_bot, track_id)
            {
                let mut y_acc = track_top - state.arrangement.scroll_y;
                for track in &state.project.tracks {
                    track_rows.push((y_acc, y_acc + track.height, track.id));
                    y_acc += track.height;
                }
            }
            let tracks_bottom = track_rows.last().map(|(_, b, _)| *b).unwrap_or(track_top);

            // Which row is mouse in? (or below all = new track)
            let mut target_row: Option<usize> = None;
            let mut below_all = false;
            if input.mouse_x > lane_left && input.mouse_y > track_top {
                if input.mouse_y >= tracks_bottom {
                    below_all = true;
                } else {
                    for (ri, (yt, yb, _)) in track_rows.iter().enumerate() {
                        if input.mouse_y >= *yt && input.mouse_y < *yb {
                            target_row = Some(ri);
                            break;
                        }
                    }
                }
            }

            let (clip_len, clip_type) = if let Some((_, ref c)) = state.library_drag_clip {
                let len = match c {
                    crate::models::Clip::Midi(m) => m.length,
                    crate::models::Clip::Audio(a) => a.length,
                    crate::models::Clip::Automation(a) => a.length,
                };
                let ct = match c {
                    crate::models::Clip::Midi(_) => crate::models::TrackType::Midi,
                    crate::models::Clip::Audio(_) => crate::models::TrackType::Audio,
                    crate::models::Clip::Automation(_) => crate::models::TrackType::Automation,
                };
                (len, ct)
            } else {
                (4.0, crate::models::TrackType::Midi)
            };

            let beat = scroll_x + (input.mouse_x - lane_left).max(0) as f64 / zoom;
            let snapped = (beat * 2.0).round() / 2.0;
            let gx = lane_left + ((snapped - scroll_x) * zoom) as i32;
            let gw = (clip_len * zoom) as i32;

            if input.mouse_down {
                canvas.set_clip_rect(Rect::new(
                    lane_left,
                    track_top,
                    (state.window_width as i32 - lane_left).max(0) as u32,
                    state.track_area_height() as u32,
                ));

                if let Some(row) = target_row {
                    let type_ok = state.project.tracks[row].track_type == clip_type;
                    let (yt, _, _) = track_rows[row];
                    let gh = state.project.tracks[row].height;
                    if type_ok {
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 80));
                        let _ = canvas.fill_rect(Rect::new(gx, yt, gw.max(4) as u32, gh as u32));
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 200));
                        let _ = canvas.draw_rect(Rect::new(gx, yt, gw.max(4) as u32, gh as u32));
                    } else {
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 60, 60, 60));
                        let _ = canvas.fill_rect(Rect::new(gx, yt, gw.max(4) as u32, gh as u32));
                        canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 60, 60, 180));
                        let _ = canvas.draw_rect(Rect::new(gx, yt, gw.max(4) as u32, gh as u32));
                    }
                } else if below_all {
                    // Ghost below all tracks — shows where new track will be created
                    let new_track_h = 80i32;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 160, 60));
                    let _ = canvas.fill_rect(Rect::new(
                        gx,
                        tracks_bottom,
                        gw.max(4) as u32,
                        new_track_h as u32,
                    ));
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 160, 180));
                    let _ = canvas.draw_rect(Rect::new(
                        gx,
                        tracks_bottom,
                        gw.max(4) as u32,
                        new_track_h as u32,
                    ));
                }
                canvas.set_clip_rect(None);
            } else {
                // Mouse released — perform the drop
                if input.mouse_x > lane_left && input.mouse_y > track_top {
                    if let Some((_, lib_clip)) = state.library_drag_clip.take() {
                        let mut placed_clip = lib_clip.clone();
                        match &mut placed_clip {
                            crate::models::Clip::Midi(m) => m.start_time = snapped,
                            crate::models::Clip::Audio(a) => a.start_time = snapped,
                            crate::models::Clip::Automation(a) => a.start_time = snapped,
                        }

                        if let Some(row) = target_row {
                            if state.project.tracks[row].track_type == clip_type {
                                let tid = state.project.tracks[row].id;
                                state.commands.execute(
                                    Box::new(crate::commands::AddClips {
                                        clips: vec![(tid, placed_clip)],
                                        added_indices: vec![],
                                    }),
                                    &mut state.project,
                                );
                                state.dirty = true;
                            } else {
                                state.push_status(format!(
                                    "Cannot drop {} clip on {} track",
                                    match clip_type {
                                        crate::models::TrackType::Midi => "MIDI",
                                        crate::models::TrackType::Audio => "Audio",
                                        crate::models::TrackType::Automation => "Auto",
                                    },
                                    match state.project.tracks[row].track_type {
                                        crate::models::TrackType::Midi => "MIDI",
                                        crate::models::TrackType::Audio => "Audio",
                                        crate::models::TrackType::Automation => "Auto",
                                    },
                                ));
                            }
                        } else if below_all {
                            // Create a new track matching the clip type
                            let new_id =
                                state.project.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                            let track_name = match clip_type {
                                crate::models::TrackType::Midi => format!("MIDI {}", new_id),
                                crate::models::TrackType::Audio => format!("Audio {}", new_id),
                                crate::models::TrackType::Automation => format!("Auto {}", new_id),
                            };
                            let new_track =
                                crate::models::Track::new(new_id, &track_name, clip_type);
                            state.commands.execute(
                                Box::new(crate::commands::AddTrack { track: new_track }),
                                &mut state.project,
                            );
                            state.commands.execute(
                                Box::new(crate::commands::AddClips {
                                    clips: vec![(new_id, placed_clip)],
                                    added_indices: vec![],
                                }),
                                &mut state.project,
                            );
                            state.dirty = true;
                        }
                    }
                } else {
                    state.library_drag_clip = None;
                }
            }
        }

        // ── Audio editor selection drag → arrangement drop ─────────────────────
        if state.audio_drag_to_arranger {
            let left = state.arrangement_left_offset();
            let header_w = state.arrangement.track_header_width;
            let lane_left = left + header_w;
            let track_top = state.track_area_top();
            let zoom = state.arrangement.zoom_x;
            let scroll_x = state.arrangement.scroll_x;
            let bpm = state.project.tempo_map.bpm_at(0.0);
            let drag_len_beats = if bpm > 0.0 {
                state.audio_drag_length_secs * bpm / 60.0
            } else {
                4.0
            };

            // Gather track y ranges
            let mut track_rows: Vec<(i32, i32, u32, crate::models::TrackType)> = Vec::new();
            {
                let mut y_acc = track_top - state.arrangement.scroll_y;
                for track in &state.project.tracks {
                    track_rows.push((y_acc, y_acc + track.height, track.id, track.track_type));
                    y_acc += track.height;
                }
            }

            // Which row is mouse in?
            let mut target_row: Option<usize> = None;
            if input.mouse_x > lane_left && input.mouse_y > track_top {
                for (ri, (yt, yb, _, _)) in track_rows.iter().enumerate() {
                    if input.mouse_y >= *yt && input.mouse_y < *yb {
                        target_row = Some(ri);
                        break;
                    }
                }
            }

            if input.mouse_down {
                // Draw ghost clip at mouse position in the arrangement
                if input.mouse_x > lane_left && input.mouse_y > track_top {
                    if let Some(row) = target_row {
                        let beat = scroll_x + (input.mouse_x - lane_left) as f64 / zoom;
                        let snapped = state.snap.snap(beat);
                        let gx = lane_left + ((snapped - scroll_x) * zoom) as i32;
                        let gw = (drag_len_beats * zoom) as i32;
                        let mut gy = track_top - state.arrangement.scroll_y;
                        for ti in 0..row {
                            gy += state.project.tracks[ti].height;
                        }
                        let gh = state.project.tracks[row].height;
                        canvas.set_clip_rect(Rect::new(
                            lane_left,
                            track_top,
                            (state.window_width as i32 - lane_left) as u32,
                            state.track_area_height() as u32,
                        ));
                        let is_audio_track = track_rows[row].3 == crate::models::TrackType::Audio;
                        if is_audio_track {
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 140, 80));
                            let _ =
                                canvas.fill_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 140, 200));
                            let _ =
                                canvas.draw_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                        } else {
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 60, 60));
                            let _ =
                                canvas.fill_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 60, 180));
                            let _ =
                                canvas.draw_rect(Rect::new(gx, gy, gw.max(4) as u32, gh as u32));
                        }
                        canvas.set_clip_rect(None);
                    }
                }
            } else {
                // Mouse released — drop
                if input.mouse_x > lane_left && input.mouse_y > track_top {
                    if let Some(row) = target_row {
                        let beat = scroll_x + (input.mouse_x - lane_left) as f64 / zoom;
                        let snapped = state.snap.snap(beat);
                        let track_type = track_rows[row].3;
                        let track_id = track_rows[row].2;

                        if track_type == crate::models::TrackType::Audio {
                            // Drop as an audio clip on this audio track
                            let clip = crate::models::Clip::Audio(crate::models::AudioClip {
                                source_file: state.audio_drag_source.clone(),
                                start_time: snapped,
                                offset: state.audio_drag_offset,
                                length: drag_len_beats,
                                gain: 1.0,
                                name: {
                                    let p = std::path::Path::new(&state.audio_drag_source);
                                    p.file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("audio")
                                        .to_string()
                                },
                                color: [100, 200, 140, 255],
                                fade_in: 0.0,
                                fade_out: 0.0,
                            });
                            state.commands.execute(
                                Box::new(crate::commands::AddClips {
                                    clips: vec![(track_id, clip)],
                                    added_indices: vec![],
                                }),
                                &mut state.project,
                            );
                            state.dirty = true;
                            state.push_status("Dropped audio region as clip");
                        } else {
                            state.push_status("Drop audio regions on audio tracks");
                        }
                    }
                }
                state.audio_drag_to_arranger = false;
                state.audio_drag_source.clear();
            }
        }
    } // end if layer == Base (drag-drop handlers)

    // Reset viewport/clip_rect before bottom panel
    canvas.set_viewport(None);
    canvas.set_clip_rect(None);
    // Final counter sync before bottom panel (counters have been synced after
    // every draw call above, but drag-drop handlers may have called next_id
    // on one InputState only).
    sync_counters!();
    // Layer 1 — bottom panel (sits above track area)
    draw_bottom_panel(canvas, bg!(input), state);
    // Reset viewport/clip_rect after bottom panel
    canvas.set_viewport(None);
    canvas.set_clip_rect(None);

    // Fallback: clear sample drag if mouse released and no drop target handled it
    if state.sample_drag_path.is_some() && !input.mouse_down {
        state.sample_drag_path = None;
        state.sample_drag_len_beats = None;
    }

    // Fallback: clear audio drag to arranger if mouse released
    if state.audio_drag_to_arranger && !input.mouse_down {
        state.audio_drag_to_arranger = false;
        state.audio_drag_source.clear();
    }

    // ── Focus indicator: a 2-px accent border on the active panel ────
    {
        use crate::state::FocusedPanel;
        let ac = state.theme.accent;
        let focus_color = sdl2::pixels::Color::RGBA(ac[0], ac[1], ac[2], 200);
        let w = state.window_width as i32;
        let total_h = state.window_height as i32;
        let panel_h = state.bottom_panel_effective_h();
        let panel_y = total_h - panel_h;
        let handle_h = state.bottom_panel_handle_h();
        let track_top = state.track_area_top();
        let track_bottom = if state.bottom_panel_open {
            panel_y
        } else {
            total_h - handle_h
        };

        canvas.set_draw_color(focus_color);
        let left_off = state.arrangement_left_offset();
        match state.focused_panel {
            FocusedPanel::Arrangement => {
                // Border around arrangement track area (offset by sample browser width)
                let fx = left_off;
                let fw = (w - left_off).max(1);
                let fh = (track_bottom - track_top).max(0);
                if fh > 4 {
                    let _ = canvas.draw_rect(Rect::new(fx, track_top, fw as u32, fh as u32));
                    let _ = canvas.draw_rect(Rect::new(
                        fx + 1,
                        track_top + 1,
                        (fw - 2).max(1) as u32,
                        (fh - 2).max(1) as u32,
                    ));
                }
            }
            FocusedPanel::PianoRoll
            | FocusedPanel::AutomationEditor
            | FocusedPanel::AudioEditor => {
                // Border around bottom panel content (full width — bottom panel spans entire window)
                if state.bottom_panel_open {
                    let content_y = panel_y + handle_h;
                    let content_h = panel_h - handle_h;
                    let _ = canvas.draw_rect(Rect::new(0, content_y, w as u32, content_h as u32));
                    let _ = canvas.draw_rect(Rect::new(
                        1,
                        content_y + 1,
                        (w - 2).max(1) as u32,
                        (content_h - 2).max(1) as u32,
                    ));
                }
            }
        }
    }

    // Layer 2 — dropdowns and popups (MUST be last — always on top)
    draw_overlays(canvas, input, state);

    // Layer 3 — Hover tooltip (drawn absolutely last, on top of everything)
    if let Some(ref hint_text) = input.hover_hint_text {
        // Track hover timer for delay before showing
        if input.hover_hint_widget == state.hover_last_widget {
            state.hover_timer += 1;
        } else {
            state.hover_timer = 0;
            state.hover_last_widget = input.hover_hint_widget;
        }

        // Show after ~20 frames (~330ms at 60fps)
        if state.hover_timer > 20 {
            let tip_x = input.mouse_x + 12;
            let tip_y = input.mouse_y - 22;
            let char_w = 9; // pixel font width at 2x (8px glyph + 1px gap)
            let text_w = hint_text.len() as i32 * char_w;
            let pad = 4;
            let tw = text_w + pad * 2;
            let th = 18;

            // Clamp to screen
            let tx = tip_x.min(state.window_width as i32 - tw - 4);
            let ty = tip_y.max(2);

            // Shadow
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 140));
            let _ = canvas.fill_rect(Rect::new(tx + 1, ty + 1, tw as u32, th as u32));
            // Background
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 50, 240));
            let _ = canvas.fill_rect(Rect::new(tx, ty, tw as u32, th as u32));
            // Border
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 85, 100, 255));
            let _ = canvas.draw_rect(Rect::new(tx, ty, tw as u32, th as u32));
            // Text
            draw_pixel_label(
                canvas,
                &state.theme,
                hint_text,
                tx + pad,
                ty + 4,
                tw - pad * 2,
                sdl2::pixels::Color::RGBA(220, 220, 230, 255),
            );
        }
    } else {
        state.hover_timer = 0;
        state.hover_last_widget = crate::input::WidgetId::None;
    }

    // ── Module drag indicator ─────────────────────────────────────────
    if let Some(ref module_name) = state.module_drag {
        let tip_x = input.mouse_x + 12;
        let tip_y = input.mouse_y + 12;
        let char_w = 9;
        let text_w = module_name.len() as i32 * char_w;
        let pad = 6;
        let tw = text_w + pad * 2;
        let th = 20;

        // Check if the module would be valid on the currently selected track
        let drop_valid = if let Some(sel_ti) = state.selected_track {
            if let Some(track) = state.project.tracks.iter().find(|t| t.id == sel_ti) {
                match track.track_type {
                    crate::models::TrackType::Midi => true,
                    crate::models::TrackType::Audio => {
                        !crate::modules::is_midi_effect(module_name)
                            && !crate::modules::is_instrument(module_name)
                    }
                    crate::models::TrackType::Automation => false,
                }
            } else {
                true
            }
        } else {
            true
        };

        // Clamp to screen
        let tx = tip_x.min(state.window_width as i32 - tw - 4);
        let ty = tip_y.min(state.window_height as i32 - th - 4);

        // Shadow
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 160));
        let _ = canvas.fill_rect(Rect::new(tx + 2, ty + 2, tw as u32, th as u32));

        if drop_valid {
            // Green — valid drop
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 140, 80, 240));
            let _ = canvas.fill_rect(Rect::new(tx, ty, tw as u32, th as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 200, 120, 255));
            let _ = canvas.draw_rect(Rect::new(tx, ty, tw as u32, th as u32));
        } else {
            // Red — invalid drop
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(140, 50, 50, 240));
            let _ = canvas.fill_rect(Rect::new(tx, ty, tw as u32, th as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 80, 255));
            let _ = canvas.draw_rect(Rect::new(tx, ty, tw as u32, th as u32));
        }

        // Text
        draw_pixel_label(
            canvas,
            &state.theme,
            module_name,
            tx + pad,
            ty + 5,
            tw - pad * 2,
            sdl2::pixels::Color::RGBA(255, 255, 255, 255),
        );
    }

    // ── Status toast notification ─────────────────────────────────────
    if state.status_timer > 0 {
        state.status_timer -= 1;
        if let Some(ref msg) = state.status_message {
            let w = state.window_width as i32;
            let h = state.window_height as i32;
            let alpha = if state.status_timer < 30 {
                (state.status_timer as f32 / 30.0 * 220.0) as u8
            } else {
                220u8
            };
            let char_w = 9i32;
            let text_w = msg.len() as i32 * char_w;
            let pad = 10;
            let tw = text_w + pad * 2;
            let th = 24;
            let tx = (w - tw) / 2;
            let ty = h - th - 40;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 30, alpha));
            let _ = canvas.fill_rect(Rect::new(tx, ty, tw as u32, th as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 130, 200, alpha));
            let _ = canvas.draw_rect(Rect::new(tx, ty, tw as u32, th as u32));
            draw_pixel_label(
                canvas,
                &state.theme,
                msg,
                tx + pad,
                ty + 7,
                tw - pad * 2,
                sdl2::pixels::Color::RGBA(220, 230, 255, alpha),
            );
        }
        if state.status_timer == 0 {
            state.status_message = None;
        }
    }
}

/// Draw all popup overlays that must appear above everything else.
/// Input routing is handled by the UiLayer state machine in draw_arrangement —
/// by the time this function is called, `input` is already the real input
/// (background layers were given dead_input). No restore/block logic needed here.
fn draw_overlays(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    canvas.set_clip_rect(None);

    let layer = state.active_layer();

    // Dropdown for snap resolution — only when no popup is shadowing it
    if layer == crate::state::UiLayer::Base && state.dropdown_open_id == 200 {
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
    if layer > crate::state::UiLayer::Base {
        input.hover_hint_text = None;
        input.hover_hint_widget = crate::input::WidgetId::None;
        input.hot_widget = crate::input::WidgetId::None;
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
                Some(crate::state::FileBrowserCaller::AudioExportDir) => {
                    state.audio_export_dir = selected_path.to_string_lossy().to_string();
                    // Update text field buffer if directory field is active
                    if state.text_field_active_id == 303 {
                        state.text_field_buffer = state.audio_export_dir.clone();
                        state.text_field_cursor = state.text_field_buffer.len();
                    }
                }
                Some(crate::state::FileBrowserCaller::MidiExportDir) => {
                    state.midi_export_dir = selected_path.to_string_lossy().to_string();
                    if state.text_field_active_id == 304 {
                        state.text_field_buffer = state.midi_export_dir.clone();
                        state.text_field_cursor = state.text_field_buffer.len();
                    }
                }
                Some(crate::state::FileBrowserCaller::OpenProject) => {
                    // Handled in draw_home_screen
                }
                Some(crate::state::FileBrowserCaller::RenderExportDir) => {
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
                Box::new(crate::commands::RemoveTrack {
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
                    Box::new(crate::commands::SetProjectName {
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
        .filter(|t| t.track_type == crate::models::TrackType::Midi)
        .count();
    let audio_count = state
        .project
        .tracks
        .iter()
        .filter(|t| t.track_type == crate::models::TrackType::Audio)
        .count();
    let auto_count = state
        .project
        .tracks
        .iter()
        .filter(|t| t.track_type == crate::models::TrackType::Automation)
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

fn draw_new_project_popup(
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
        state.project = crate::models::Project::default();
        state.project.name = name;
        state.last_save_path = None;
        state.dirty = false;
        state.commands = crate::commands::CommandManager::new(1000);
        state.mode = crate::state::AppMode::Arrangement;
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
            crate::state::FileBrowserCaller::AudioExportDir,
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
            crate::state::FileBrowserCaller::MidiExportDir,
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
                if let Some(crate::models::Clip::Midi(m)) = track.clips.get(ci) {
                    let clip_name = if m.name.is_empty() {
                        format!("clip_{}", ci)
                    } else {
                        m.name.clone()
                    };
                    export_result = crate::models::export_midi_file(
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
            let (_, secs) = crate::config::AUTOSAVE_INTERVALS[state.autosave_interval_idx];
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
        let interval_labels: Vec<&str> = crate::config::AUTOSAVE_INTERVALS
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
            let (_, secs) = crate::config::AUTOSAVE_INTERVALS[state.autosave_interval_idx];
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
        state.audio_device_names = crate::audio::list_output_devices();
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
        state.audio_device_names = crate::audio::list_output_devices();
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
            crate::state::FileBrowserCaller::RenderExportDir,
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
        let settings = crate::render::RenderSettings {
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
            let result = crate::render::render_to_wav_with_progress(
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
                default_value: Some(vol_gain_to_pos(0.8)),
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
                        Box::new(crate::commands::SetTrackVolume {
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
                        Box::new(crate::commands::SetTrackPan {
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
            state.project.tracks[i].track_type == crate::models::TrackType::Automation;
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
                        Box::new(crate::commands::SetTrackMute {
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
                        Box::new(crate::commands::SetTrackSolo {
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

/// Draw the clip manager sidebar — scrollable list of all clips across all tracks
/// with mini-preview thumbnails. Click a clip to select it.
fn draw_clip_manager(
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
                crate::models::Clip::Midi(_) => 0,
                crate::models::Clip::Audio(_) => 1,
                crate::models::Clip::Automation(_) => 2,
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

fn draw_piano_roll(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    draw_piano_roll_at(canvas, input, state, 0, top, w, h);
}

fn draw_piano_roll_at(
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

fn draw_piano_roll_impl(
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

/// Find the nearest zero crossing to `idx` in `samples`, searching up to `max_search`
/// samples in each direction. A zero crossing is where the sample value crosses zero
/// (sign change) or is exactly zero. Returns the adjusted index.
fn nearest_zero_crossing(samples: &[f32], idx: usize, max_search: usize) -> usize {
    let len = samples.len();
    if idx >= len {
        return idx;
    }
    // If the sample at idx is already very close to zero, use it
    if samples[idx].abs() < 0.001 {
        return idx;
    }
    // Search outward from idx in both directions
    for offset in 1..=max_search {
        // Search forward
        if idx + offset < len {
            let prev = samples[idx + offset - 1];
            let curr = samples[idx + offset];
            if curr.abs() < 0.001 || (prev.signum() != curr.signum() && prev != 0.0) {
                return idx + offset;
            }
        }
        // Search backward
        if offset <= idx {
            let curr = samples[idx - offset];
            let next = samples[idx - offset + 1];
            if curr.abs() < 0.001 || (curr.signum() != next.signum() && next != 0.0) {
                return idx - offset;
            }
        }
    }
    idx // no crossing found within range, keep original
}

fn draw_audio_editor(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
    top: i32,
    w: i32,
    h: i32,
) {
    use sdl2::rect::Rect;
    // Always clear any clip rect left over from the arrangement view drawing
    canvas.set_clip_rect(None);
    let bg = Theme::c(state.theme.bg_dark);
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, h as u32));

    // ── Click anywhere in audio editor to focus it ───────────────────
    if input.mouse_in_rect(0, top, w, h) && input.mouse_pressed {
        state.focused_panel = crate::state::FocusedPanel::AudioEditor;
    }

    // ── Gather clip info ─────────────────────────────────────────────
    #[allow(clippy::type_complexity)]
    let clip_info: Option<(String, String, f64, f64, f32, f64, f64, f64)> =
        if let Some((track_id, clip_idx)) = state.selected_clip {
            state
                .project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| t.clips.get(clip_idx))
                .and_then(|c| {
                    if let crate::models::Clip::Audio(ac) = c {
                        let name = if ac.name.is_empty() {
                            std::path::Path::new(&ac.source_file)
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("audio")
                                .to_string()
                        } else {
                            ac.name.clone()
                        };
                        Some((
                            name,
                            ac.source_file.clone(),
                            ac.length,
                            ac.offset,
                            ac.gain,
                            ac.start_time,
                            ac.fade_in,
                            ac.fade_out,
                        ))
                    } else {
                        None
                    }
                })
        } else {
            None
        };

    let (
        clip_name,
        source_file,
        clip_len_beats,
        clip_offset_secs,
        clip_gain,
        clip_start_beats,
        clip_fade_in,
        clip_fade_out,
    ) = match clip_info {
        Some(info) => info,
        None => {
            draw_pixel_label(
                canvas,
                &state.theme,
                "No audio clip selected",
                10,
                top + h / 2 - 5,
                w - 20,
                Theme::c(state.theme.text_dim),
            );
            return;
        }
    };

    // Sync fade state from clip (keeps sliders in sync when clip changes)
    state.audio_editor_fade_in = clip_fade_in;
    state.audio_editor_fade_out = clip_fade_out;

    // ── Full audio file duration (seconds) ───────────────────────────
    let file_dur_secs = state
        .waveform_cache
        .get(&source_file)
        .map(|(_, dur)| *dur)
        .unwrap_or(0.0);

    let total_secs = if file_dur_secs > 0.0 {
        file_dur_secs
    } else {
        (clip_offset_secs + 10.0).max(1.0)
    };

    let bpm_early = state.project.tempo_map.bpm_at(0.0);

    // ── Sync audio editor playhead to main transport during playback ──
    if state.project.transport.playing && !state.audio_editor_playing {
        let transport_beats = state.project.transport.position;
        if bpm_early > 0.0 {
            let beats_into_clip = transport_beats - clip_start_beats;
            let secs_into_clip = beats_into_clip * 60.0 / bpm_early;
            let file_pos = clip_offset_secs + secs_into_clip;
            if file_pos >= 0.0 && file_pos <= total_secs {
                state.audio_editor_playhead = file_pos;
            }
        }
    }
    let clip_len_secs = if bpm_early > 0.0 {
        clip_len_beats * 60.0 / bpm_early
    } else {
        total_secs
    };

    let clip_win_start_secs = clip_offset_secs.min(total_secs);
    let clip_win_end_secs = (clip_offset_secs + clip_len_secs).min(total_secs);

    // ── Layout constants ─────────────────────────────────────────────
    let toolbar_h = 28i32;
    let loop_ruler_h = 14i32; // NEW: loop region bar
    let ruler_h = 20i32;
    let info_h = 18i32;
    let scroll_bar_h = 14i32;
    let wave_top = top + toolbar_h + loop_ruler_h + ruler_h;
    let wave_h = (h - toolbar_h - loop_ruler_h - ruler_h - info_h - scroll_bar_h).max(30);
    let wave_left = 10i32;
    let wave_w = (w - 20).max(10);
    let ch_h = (wave_h / 2).max(10);

    // ── Viewport (zoom + scroll) in SECONDS ──────────────────────────
    if total_secs > 0.0 && state.audio_editor_zoom == 1.0 {
        state.audio_editor_zoom = (wave_w as f64 / total_secs).clamp(4.0, 1000.0);
    }
    let zoom = state.audio_editor_zoom.clamp(1.0, 4000.0);
    let visible_secs = wave_w as f64 / zoom;
    let max_scroll_secs = (total_secs - visible_secs).max(0.0);
    let scroll = state.audio_editor_scroll.clamp(0.0, max_scroll_secs);
    state.audio_editor_scroll = scroll;

    let sec_to_x = |s: f64| -> i32 { wave_left + ((s - scroll) * zoom) as i32 };
    let x_to_sec = |x: i32| -> f64 { (x - wave_left) as f64 / zoom + scroll };

    let bpm = bpm_early;

    // ── Toolbar ──────────────────────────────────────────────────────
    canvas.set_draw_color(Theme::c(state.theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(0, top, w as u32, toolbar_h as u32));
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(0, top + toolbar_h - 1),
        sdl2::rect::Point::new(w, top + toolbar_h - 1),
    );

    // ── Audio editor mini transport controls ───────────────────────
    // Rewind (|◀) — resets playhead to 0
    let is_previewing = state.audio_editor_playing;
    {
        let __auto_id_rw = input.next_id();
        let rewind_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_rw,
                x: 6,
                y: top + 4,
                width: 22,
                height: 20,
                label: "|◀".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Rewind to start".into()),
                ..Default::default()
            },
        );
        if rewind_clicked {
            state.audio_editor_playhead = 0.0;
            // If playing, restart from beginning
            if is_previewing && !source_file.is_empty() {
                let preview_sr = 44100usize;
                state.sample_preview_start_sample = 0;
                if state.audio_editor_loop_enabled
                    && state.audio_editor_loop_end > state.audio_editor_loop_start
                {
                    state.sample_preview_end_sample =
                        (state.audio_editor_loop_end * preview_sr as f64) as usize;
                } else {
                    state.sample_preview_end_sample = 0;
                }
                state.sample_preview_path = Some(std::path::PathBuf::from(&source_file));
                state.sample_preview_trigger = true;
            }
        }
    }

    // Stop (■) — stops playback and rewinds playhead to where play started
    {
        let __auto_id_stop = input.next_id();
        let stop_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_stop,
                x: 30,
                y: top + 4,
                width: 22,
                height: 20,
                label: "■".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Stop playback".into()),
                ..Default::default()
            },
        );
        if stop_clicked && is_previewing {
            state.audio_editor_playing = false;
            state.sample_preview_path = None;
            state.sample_preview_start_sample = 0;
            state.sample_preview_end_sample = 0;
        }
    }

    // Play (▶) — starts/stops playback
    let __auto_id_56 = input.next_id();
    let play_clicked = button(
        canvas,
        input,
        &state.theme,
        &ButtonParams {
            id: __auto_id_56,
            x: 54,
            y: top + 4,
            width: 22,
            height: 20,
            label: "▶".into(),
            toggled: is_previewing,
            icon: ButtonIcon::None,
            hint: Some("Play / pause preview".into()),
            ..Default::default()
        },
    );
    if play_clicked {
        if is_previewing {
            // Stop
            state.audio_editor_playing = false;
            state.sample_preview_path = None;
            state.sample_preview_start_sample = 0;
            state.sample_preview_end_sample = 0;
        } else if !source_file.is_empty() {
            // Start playback from playhead (or selection if any)
            let preview_sr = 44100usize;
            if let Some((sel_s, sel_e)) = state.audio_editor_selection {
                let s = sel_s.min(sel_e).max(0.0);
                let e = sel_s.max(sel_e);
                state.audio_editor_playhead = s;
                state.sample_preview_start_sample = (s * preview_sr as f64) as usize;
                state.sample_preview_end_sample = (e * preview_sr as f64) as usize;
            } else if state.audio_editor_loop_enabled
                && state.audio_editor_loop_end > state.audio_editor_loop_start
            {
                // Play from playhead (or loop start), loop between loop_start and loop_end
                let loop_s = state.audio_editor_loop_start;
                let loop_e = state.audio_editor_loop_end;
                let start = if state.audio_editor_playhead >= loop_s
                    && state.audio_editor_playhead < loop_e
                {
                    state.audio_editor_playhead
                } else {
                    loop_s
                };
                state.audio_editor_playhead = start;
                state.sample_preview_start_sample = (start * preview_sr as f64) as usize;
                state.sample_preview_end_sample = (loop_e * preview_sr as f64) as usize;
            } else {
                // Play from playhead to end
                let start = state.audio_editor_playhead;
                state.sample_preview_start_sample = (start * preview_sr as f64) as usize;
                state.sample_preview_end_sample = 0; // play to end
            }
            state.audio_editor_playing = true;
            state.sample_preview_path = Some(std::path::PathBuf::from(&source_file));
            state.sample_preview_trigger = true;
        }
    }

    // Loop toggle button
    {
        let loop_id = input.next_id();
        let loop_clicked = toggle_button(
            canvas,
            input,
            &state.theme,
            80,
            top + 4,
            20,
            state.theme.loop_color,
            state.audio_editor_loop_enabled,
            loop_id,
            "L",
            Some("Toggle audio editor loop"),
        );
        if loop_clicked {
            state.audio_editor_loop_enabled = !state.audio_editor_loop_enabled;
        }
    }

    // ── Make Unique button ──
    // Only available when the current clip's source_file is shared by another clip.
    let is_clone = if !source_file.is_empty() {
        let sf = &source_file;
        let mut count = 0usize;
        for track in &state.project.tracks {
            for clip in &track.clips {
                if let crate::models::Clip::Audio(ac) = clip {
                    if ac.source_file == *sf {
                        count += 1;
                        if count > 1 {
                            break;
                        }
                    }
                }
            }
            if count > 1 {
                break;
            }
        }
        count > 1
    } else {
        false
    };
    if is_clone {
        let unique_id = input.next_id();
        let unique_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: unique_id,
                x: 106,
                y: top + 4,
                width: 52,
                height: 20,
                label: "UNIQUE".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some(
                    "Make a unique copy of this clip's audio so edits don't affect clones".into(),
                ),
                ..Default::default()
            },
        );
        if unique_clicked {
            // Snapshot for undo before mutating
            let snapshot = state.project.clone();
            // Copy source file to a new unique file
            let src_path = std::path::Path::new(&source_file);
            let dir = src_path.parent().unwrap_or(std::path::Path::new("."));
            let stem = src_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("audio");
            let ext = src_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("wav");
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let new_name = format!("{}_unique_{}.{}", stem, ts, ext);
            let new_path = dir.join(&new_name);
            match std::fs::copy(&source_file, &new_path) {
                Ok(_) => {
                    let new_path_str = new_path.to_string_lossy().to_string();
                    // Update this clip's source_file to the new copy
                    if let Some((track_id, clip_idx)) = state.selected_clip {
                        if let Some(track) =
                            state.project.tracks.iter_mut().find(|t| t.id == track_id)
                        {
                            if let Some(crate::models::Clip::Audio(ac)) =
                                track.clips.get_mut(clip_idx)
                            {
                                ac.source_file = new_path_str.clone();
                            }
                        }
                    }
                    // Invalidate caches for the new file so it loads fresh
                    state.waveform_cache.remove(&new_path_str);
                    state.dirty = true;
                    state
                        .commands
                        .push_undo_snapshot(snapshot, "Make Clip Unique");
                    state.push_status("Clip made unique — edits are now independent");
                }
                Err(e) => {
                    state.push_status(format!("Make unique failed: {}", e));
                }
            }
        }
    }

    // Toolbar buttons — SEL, NORM, TRIM, FIT, CUT, PASTE
    let tool_labels = ["SEL", "NORM", "TRIM", "FIT", "CUT", "PASTE"];
    let mut bx = if is_clone { 164i32 } else { 106i32 };

    // ── Keyboard shortcuts for toolbar tools (left-hand keys) ────────
    // Q=SEL(all), W=NORM, E=TRIM, R=FIT, T=CUT, Y=PASTE
    let key_triggered_tool: Option<usize> = if state.focused_panel
        == crate::state::FocusedPanel::AudioEditor
        && state.text_field_active_id == 0
    {
        if input.key_available(sdl2::keyboard::Keycode::Q) {
            input.consume_key(sdl2::keyboard::Keycode::Q);
            Some(0)
        } else if input.key_available(sdl2::keyboard::Keycode::W) && !input.ctrl() {
            input.consume_key(sdl2::keyboard::Keycode::W);
            Some(1)
        } else if input.key_available(sdl2::keyboard::Keycode::E) {
            input.consume_key(sdl2::keyboard::Keycode::E);
            Some(2)
        } else if input.key_available(sdl2::keyboard::Keycode::R) {
            input.consume_key(sdl2::keyboard::Keycode::R);
            Some(3)
        } else if input.key_available(sdl2::keyboard::Keycode::T) {
            input.consume_key(sdl2::keyboard::Keycode::T);
            Some(4)
        } else if input.key_available(sdl2::keyboard::Keycode::Y) {
            input.consume_key(sdl2::keyboard::Keycode::Y);
            Some(5)
        } else {
            None
        }
    } else {
        None
    };

    // Helper: create an undo backup of the source file before destructive operations.
    // Returns Ok(backup_path) or Err(message).
    let make_undo_backup = |src: &str| -> Result<String, String> {
        let src_path = std::path::Path::new(src);
        let dir = src_path.parent().unwrap_or(std::path::Path::new("."));
        let stem = src_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("audio");
        let ext = src_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("wav");
        // Use a timestamp-based backup name to avoid collisions
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let backup_name = format!(".{}_undo_{}.{}", stem, ts, ext);
        let backup_path = dir.join(backup_name);
        std::fs::copy(src, &backup_path)
            .map(|_| backup_path.to_string_lossy().to_string())
            .map_err(|e| format!("Backup failed: {}", e))
    };

    let tool_hints: [&str; 6] = [
        "Select entire waveform (Q)",
        "Normalize selection to 0 dB peak (W)",
        "Trim — remove audio outside selection, keeps selected region (E)",
        "Fit — set clip window to selection without modifying audio (R)",
        "Cut — remove selection from file and copy to clipboard (T)",
        "Paste — insert clipboard audio at playhead position (Y)",
    ];
    for (i, &label) in tool_labels.iter().enumerate() {
        let bw = (label.len() as i32 * 8 + 12).max(40);
        let __auto_id_57 = input.next_id();
        let clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: __auto_id_57,
                x: bx,
                y: top + 4,
                width: bw,
                height: 20,
                label: label.into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some(tool_hints.get(i).unwrap_or(&"").to_string()),
                ..Default::default()
            },
        );
        if clicked || key_triggered_tool == Some(i) {
            match i {
                0 => {
                    // Select all
                    state.audio_editor_selection = Some((0.0, total_secs));
                }
                1 => {
                    // Normalize — destructive: scales samples in selected region to peak at 0dB
                    if let Some((_track_id, _clip_idx)) = state.selected_clip {
                        let (sel_s, sel_e) = state
                            .audio_editor_selection
                            .map(|(a, b)| (a.min(b), a.max(b)))
                            .unwrap_or((0.0, total_secs));
                        if (sel_e - sel_s) > 0.001 {
                            let path = std::path::Path::new(&source_file);
                            if let Ok((raw, channels, sr)) =
                                crate::audio::load_audio_interleaved(path)
                            {
                                let total_frames = raw.len() / channels.max(1);
                                let start_frame = ((sel_s * sr as f64) as usize).min(total_frames);
                                let end_frame = ((sel_e * sr as f64) as usize).min(total_frames);
                                if end_frame > start_frame {
                                    // Find peak in selected region
                                    let region = &raw[start_frame * channels..end_frame * channels];
                                    let peak =
                                        region.iter().cloned().map(f32::abs).fold(0.0f32, f32::max);
                                    if peak > 0.001 {
                                        // Create undo backup
                                        match make_undo_backup(&source_file) {
                                            Ok(backup) => {
                                                state.audio_redo_stack.clear();
                                                state.audio_undo_stack.push((
                                                    source_file.clone(),
                                                    backup,
                                                    "Normalize".to_string(),
                                                    None,
                                                ));
                                                // Scale the selected region
                                                let gain = 1.0 / peak;
                                                let mut modified = raw.clone();
                                                for s in &mut modified
                                                    [start_frame * channels..end_frame * channels]
                                                {
                                                    *s *= gain;
                                                }
                                                let save_result = if channels >= 2 {
                                                    crate::audio::save_wav_stereo(
                                                        path, &modified, sr,
                                                    )
                                                } else {
                                                    crate::audio::save_wav_mono(path, &modified, sr)
                                                };
                                                match save_result {
                                                    Ok(()) => {
                                                        state.waveform_cache.remove(&source_file);
                                                        state
                                                            .waveform_stereo_cache
                                                            .remove(&source_file);
                                                        state
                                                            .waveform_raw_cache
                                                            .remove(&source_file);
                                                        state
                                                            .audio_sample_invalidate
                                                            .push(source_file.clone());
                                                        state.push_status(format!("Normalized selection (peak {:.1}dB → 0dB)", 20.0 * peak.log10()));
                                                    }
                                                    Err(e) => state.push_status(format!(
                                                        "Normalize failed: {}",
                                                        e
                                                    )),
                                                }
                                            }
                                            Err(e) => state.push_status(e),
                                        }
                                    } else {
                                        state.push_status(
                                            "Selection is silent, nothing to normalize",
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                2 => {
                    // TRIM — destructive: removes audio OUTSIDE selection from file
                    if let Some((sel_s, sel_e)) = state.audio_editor_selection {
                        let s = sel_s.min(sel_e);
                        let e = sel_s.max(sel_e);
                        if (e - s) > 0.01 {
                            let path = std::path::Path::new(&source_file);
                            if let Ok((raw, channels, sr)) =
                                crate::audio::load_audio_interleaved(path)
                            {
                                let ch = channels.max(1);
                                let total_frames = raw.len() / ch;
                                let start_frame_raw = ((s * sr as f64) as usize).min(total_frames);
                                let end_frame_raw = ((e * sr as f64) as usize).min(total_frames);
                                // Snap to zero crossings only when snap is off
                                let (start_frame, end_frame) = if !state.audio_editor_snap_enabled {
                                    let mono: Vec<f32> = raw.iter().step_by(ch).copied().collect();
                                    let zc_search = (sr as usize / 100).max(64);
                                    (
                                        nearest_zero_crossing(&mono, start_frame_raw, zc_search),
                                        nearest_zero_crossing(&mono, end_frame_raw, zc_search),
                                    )
                                } else {
                                    (start_frame_raw, end_frame_raw)
                                };
                                if end_frame > start_frame {
                                    match make_undo_backup(&source_file) {
                                        Ok(backup) => {
                                            state.audio_redo_stack.clear();
                                            state.audio_undo_stack.push((
                                                source_file.clone(),
                                                backup,
                                                "Trim".to_string(),
                                                Some(state.project.clone()),
                                            ));
                                            let trimmed: Vec<f32> =
                                                raw[start_frame * ch..end_frame * ch].to_vec();
                                            let save_result = if ch >= 2 {
                                                crate::audio::save_wav_stereo(path, &trimmed, sr)
                                            } else {
                                                crate::audio::save_wav_mono(path, &trimmed, sr)
                                            };
                                            match save_result {
                                                Ok(()) => {
                                                    state.waveform_cache.remove(&source_file);
                                                    state
                                                        .waveform_stereo_cache
                                                        .remove(&source_file);
                                                    state.waveform_raw_cache.remove(&source_file);
                                                    state
                                                        .audio_sample_invalidate
                                                        .push(source_file.clone());
                                                    if let Some((track_id, clip_idx)) =
                                                        state.selected_clip
                                                    {
                                                        if let Some(t) = state
                                                            .project
                                                            .tracks
                                                            .iter_mut()
                                                            .find(|t| t.id == track_id)
                                                        {
                                                            if let Some(
                                                                crate::models::Clip::Audio(ac),
                                                            ) = t.clips.get_mut(clip_idx)
                                                            {
                                                                let old_offset = ac.offset;
                                                                ac.offset =
                                                                    (old_offset - s).max(0.0);
                                                                let new_dur = e - s;
                                                                let max_len_beats =
                                                                    new_dur * bpm / 60.0;
                                                                if ac.length > max_len_beats {
                                                                    ac.length = max_len_beats;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    state.audio_editor_selection = None;
                                                    state.audio_editor_scroll = 0.0;
                                                    state.audio_editor_zoom = 1.0;
                                                    state.audio_editor_playhead = 0.0;
                                                    state.push_status("Audio trimmed to selection (file modified)");
                                                }
                                                Err(e) => {
                                                    state.push_status(format!("Trim failed: {}", e))
                                                }
                                            }
                                        }
                                        Err(e) => state.push_status(e),
                                    }
                                }
                            }
                        }
                    }
                }
                3 => {
                    // FIT — adjusts clip window (offset + length) to match selection (non-destructive, undoable)
                    if let Some((sel_s, sel_e)) = state.audio_editor_selection {
                        let s = sel_s.min(sel_e);
                        let e = sel_s.max(sel_e);
                        if (e - s) > 0.01 && bpm > 0.0 {
                            if let Some((track_id, clip_idx)) = state.selected_clip {
                                // Snapshot project state before modifying clip (undoable via Ctrl+Z)
                                let snapshot = state.project.clone();
                                state
                                    .commands
                                    .push_undo_snapshot(snapshot, "Fit clip to selection");
                                if let Some(t) =
                                    state.project.tracks.iter_mut().find(|t| t.id == track_id)
                                {
                                    if let Some(crate::models::Clip::Audio(ac)) =
                                        t.clips.get_mut(clip_idx)
                                    {
                                        let new_len_secs = e - s;
                                        ac.offset = s;
                                        ac.length = new_len_secs * bpm / 60.0;
                                    }
                                }
                                state.dirty = true;
                            }
                            state.audio_editor_selection = None;
                            state.push_status("Clip window fitted to selection");
                        }
                    } else if total_secs > 0.0 {
                        // No selection: zoom to fit view
                        state.audio_editor_zoom = (wave_w as f64 / total_secs).clamp(1.0, 4000.0);
                        state.audio_editor_scroll = 0.0;
                    }
                }
                4 => {
                    // CUT — destructive: removes selected audio from file, stores in clipboard
                    if let Some((sel_s, sel_e)) = state.audio_editor_selection {
                        let s = sel_s.min(sel_e);
                        let e = sel_s.max(sel_e);
                        if (e - s) > 0.001 {
                            let path = std::path::Path::new(&source_file);
                            if let Ok((raw, channels, sr)) =
                                crate::audio::load_audio_interleaved(path)
                            {
                                let ch = channels.max(1);
                                let total_frames = raw.len() / ch;
                                let start_frame_raw = ((s * sr as f64) as usize).min(total_frames);
                                let end_frame_raw = ((e * sr as f64) as usize).min(total_frames);
                                // Snap to zero crossings only when snap is off
                                let (start_frame, end_frame) = if !state.audio_editor_snap_enabled {
                                    let mono: Vec<f32> = raw.iter().step_by(ch).copied().collect();
                                    let zc_search = (sr as usize / 100).max(64);
                                    (
                                        nearest_zero_crossing(&mono, start_frame_raw, zc_search),
                                        nearest_zero_crossing(&mono, end_frame_raw, zc_search),
                                    )
                                } else {
                                    (start_frame_raw, end_frame_raw)
                                };
                                if end_frame > start_frame {
                                    match make_undo_backup(&source_file) {
                                        Ok(backup) => {
                                            state.audio_redo_stack.clear();
                                            state.audio_undo_stack.push((
                                                source_file.clone(),
                                                backup,
                                                "Cut".to_string(),
                                                Some(state.project.clone()),
                                            ));
                                            // Copy cut region to clipboard (mono mix)
                                            let cut_region: Vec<f32> = if ch >= 2 {
                                                raw[start_frame * ch..end_frame * ch]
                                                    .chunks(ch)
                                                    .map(|frame| {
                                                        frame.iter().sum::<f32>() / ch as f32
                                                    })
                                                    .collect()
                                            } else {
                                                raw[start_frame..end_frame].to_vec()
                                            };
                                            state.audio_clipboard = Some(cut_region);
                                            state.audio_clipboard_sr = sr;

                                            let mut remaining = Vec::with_capacity(
                                                raw.len() - (end_frame - start_frame) * ch,
                                            );
                                            remaining.extend_from_slice(&raw[..start_frame * ch]);
                                            remaining.extend_from_slice(&raw[end_frame * ch..]);

                                            let save_result = if ch >= 2 {
                                                crate::audio::save_wav_stereo(path, &remaining, sr)
                                            } else {
                                                crate::audio::save_wav_mono(path, &remaining, sr)
                                            };
                                            match save_result {
                                                Ok(()) => {
                                                    state.waveform_cache.remove(&source_file);
                                                    state
                                                        .waveform_stereo_cache
                                                        .remove(&source_file);
                                                    state.waveform_raw_cache.remove(&source_file);
                                                    state
                                                        .audio_sample_invalidate
                                                        .push(source_file.clone());
                                                    let cut_dur = e - s;
                                                    if let Some((track_id, clip_idx)) =
                                                        state.selected_clip
                                                    {
                                                        if let Some(t) = state
                                                            .project
                                                            .tracks
                                                            .iter_mut()
                                                            .find(|t| t.id == track_id)
                                                        {
                                                            if let Some(
                                                                crate::models::Clip::Audio(ac),
                                                            ) = t.clips.get_mut(clip_idx)
                                                            {
                                                                if ac.offset >= e {
                                                                    ac.offset -= cut_dur;
                                                                } else if ac.offset > s {
                                                                    ac.offset = s;
                                                                }
                                                            }
                                                        }
                                                    }
                                                    state.audio_editor_selection = None;
                                                    state.audio_editor_playhead = s;
                                                    state.push_status(
                                                        "Audio cut to clipboard (file modified)",
                                                    );
                                                }
                                                Err(err) => state
                                                    .push_status(format!("Cut failed: {}", err)),
                                            }
                                        }
                                        Err(e) => state.push_status(e),
                                    }
                                }
                            }
                        }
                    }
                }
                5 => {
                    // PASTE — inserts clipboard audio at playhead position
                    if let Some(ref clip_data) = state.audio_clipboard.clone() {
                        let paste_sec = state.audio_editor_playhead;
                        let path = std::path::Path::new(&source_file);
                        if let Ok((raw, channels, sr)) = crate::audio::load_audio_interleaved(path)
                        {
                            match make_undo_backup(&source_file) {
                                Ok(backup) => {
                                    state.audio_redo_stack.clear();
                                    state.audio_undo_stack.push((
                                        source_file.clone(),
                                        backup,
                                        "Paste".to_string(),
                                        Some(state.project.clone()),
                                    ));
                                    let ch = channels.max(1);
                                    let total_frames = raw.len() / ch;
                                    let insert_frame_raw =
                                        ((paste_sec * sr as f64) as usize).min(total_frames);
                                    // Snap insertion point to zero crossing only when snap is off
                                    let insert_frame = if !state.audio_editor_snap_enabled {
                                        let mono: Vec<f32> =
                                            raw.iter().step_by(ch).copied().collect();
                                        let zc_search = (sr as usize / 100).max(64);
                                        nearest_zero_crossing(&mono, insert_frame_raw, zc_search)
                                    } else {
                                        insert_frame_raw
                                    };
                                    let clip_sr = state.audio_clipboard_sr;

                                    let resampled: Vec<f32> = if clip_sr != sr {
                                        let ratio = sr as f64 / clip_sr as f64;
                                        let new_len = (clip_data.len() as f64 * ratio) as usize;
                                        (0..new_len)
                                            .map(|i| {
                                                let src_idx = ((i as f64 / ratio) as usize)
                                                    .min(clip_data.len().saturating_sub(1));
                                                clip_data[src_idx]
                                            })
                                            .collect()
                                    } else {
                                        clip_data.clone()
                                    };

                                    let interleaved_paste: Vec<f32> = if ch >= 2 {
                                        resampled.iter().flat_map(|&s| vec![s; ch]).collect()
                                    } else {
                                        resampled
                                    };

                                    let mut result =
                                        Vec::with_capacity(raw.len() + interleaved_paste.len());
                                    result.extend_from_slice(&raw[..insert_frame * ch]);
                                    result.extend_from_slice(&interleaved_paste);
                                    result.extend_from_slice(&raw[insert_frame * ch..]);

                                    let save_result = if ch >= 2 {
                                        crate::audio::save_wav_stereo(path, &result, sr)
                                    } else {
                                        crate::audio::save_wav_mono(path, &result, sr)
                                    };
                                    match save_result {
                                        Ok(()) => {
                                            state.waveform_cache.remove(&source_file);
                                            state.waveform_stereo_cache.remove(&source_file);
                                            state.waveform_raw_cache.remove(&source_file);
                                            state.audio_sample_invalidate.push(source_file.clone());
                                            let paste_dur = interleaved_paste.len() as f64
                                                / (sr as f64 * ch as f64);
                                            if let Some((track_id, clip_idx)) = state.selected_clip
                                            {
                                                if let Some(t) = state
                                                    .project
                                                    .tracks
                                                    .iter_mut()
                                                    .find(|t| t.id == track_id)
                                                {
                                                    if let Some(crate::models::Clip::Audio(ac)) =
                                                        t.clips.get_mut(clip_idx)
                                                    {
                                                        if ac.offset >= paste_sec {
                                                            ac.offset += paste_dur;
                                                        }
                                                    }
                                                }
                                            }
                                            state.audio_editor_playhead = paste_sec + paste_dur;
                                            state.push_status(
                                                "Audio pasted from clipboard (file modified)",
                                            );
                                        }
                                        Err(err) => {
                                            state.push_status(format!("Paste failed: {}", err))
                                        }
                                    }
                                }
                                Err(e) => state.push_status(e),
                            }
                        }
                    } else {
                        state.push_status("Nothing in audio clipboard");
                    }
                }
                _ => {}
            }
        }
        bx += bw + 4;
    }

    // ── Audio editor snap toggle ──────────────────────────────────────
    {
        let snap_id = input.next_id();
        let snap_clicked = toggle_button(
            canvas,
            input,
            &state.theme,
            bx,
            top + 4,
            20,
            state.theme.accent,
            state.audio_editor_snap_enabled,
            snap_id,
            "S",
            Some("Toggle audio editor snap"),
        );
        if snap_clicked {
            state.audio_editor_snap_enabled = !state.audio_editor_snap_enabled;
        }
        bx += 24;
    }

    // ── Audio editor snap resolution dropdown ─────────────────────────
    let snap_dropdown_x = bx;
    {
        let snap_labels: Vec<&str> = SNAP_RESOLUTIONS.iter().map(|(l, _)| *l).collect();
        let changed = dropdown(
            canvas,
            input,
            &state.theme,
            7073,
            snap_dropdown_x,
            top + 4,
            56,
            20,
            &snap_labels,
            &mut state.audio_editor_snap_idx,
            &mut state.dropdown_open_id,
        );
        let _ = changed;
        bx += 60;
    }

    // ── Gain slider in toolbar ────────────────────────────────────────
    {
        let slider_w = 70i32;
        let slider_h = 16i32;
        let slider_x = bx;
        let slider_y = top + 6;
        let mut gain_val = clip_gain.clamp(0.0, 4.0);
        draw_pixel_label(
            canvas,
            &state.theme,
            "GAIN",
            slider_x,
            slider_y - 1,
            28,
            Theme::c(state.theme.text_secondary),
        );
        let gain_changed = slider(
            canvas,
            input,
            &state.theme,
            &SliderParams {
                id: WidgetId::Auto(7070),
                x: slider_x + 30,
                y: slider_y,
                width: slider_w,
                height: slider_h,
                min: 0.0,
                max: 4.0,
                orientation: SliderOrientation::Horizontal,
                label: Some(gain_to_db_label(clip_gain)),
                default_value: Some(1.0),
            },
            &mut gain_val,
        );
        if gain_changed {
            if let Some((track_id, clip_idx)) = state.selected_clip {
                if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Audio(ac)) = t.clips.get_mut(clip_idx) {
                        ac.gain = gain_val;
                        state.dirty = true;
                    }
                }
            }
        }
        // Commit clip gain change on release
        if input.mouse_released && input.drag_widget == WidgetId::Auto(7070) {
            if let Some((track_id, clip_idx)) = state.selected_clip {
                let old_gain = input.drag_start_value as f32;
                let new_gain = gain_val;
                if (old_gain - new_gain).abs() > 1e-4 {
                    state.commands.execute(
                        Box::new(crate::commands::SetClipGain {
                            track_id,
                            clip_idx,
                            old_gain,
                            new_gain,
                        }),
                        &mut state.project,
                    );
                }
            }
        }
        bx += 30 + slider_w + 8;
    }

    // ── Fade In slider ────────────────────────────────────────────────
    {
        let slider_w = 50i32;
        let slider_h = 16i32;
        let slider_x = bx;
        let slider_y = top + 6;
        let max_fade = total_secs.min(10.0) as f32;
        let mut fade_val = (state.audio_editor_fade_in as f32).clamp(0.0, max_fade);
        draw_pixel_label(
            canvas,
            &state.theme,
            "FIN",
            slider_x,
            slider_y - 1,
            20,
            Theme::c(state.theme.text_secondary),
        );
        let fade_changed = slider(
            canvas,
            input,
            &state.theme,
            &SliderParams {
                id: WidgetId::Auto(7075),
                x: slider_x + 22,
                y: slider_y,
                width: slider_w,
                height: slider_h,
                min: 0.0,
                max: max_fade,
                orientation: SliderOrientation::Horizontal,
                label: Some(format!("{:.2}s", fade_val)),
                default_value: Some(0.0),
            },
            &mut fade_val,
        );
        if fade_changed {
            state.audio_editor_fade_in = fade_val as f64;
            // Write back to the clip model
            if let Some((track_id, clip_idx)) = state.selected_clip {
                if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Audio(ac)) = t.clips.get_mut(clip_idx) {
                        ac.fade_in = fade_val as f64;
                        state.dirty = true;
                    }
                }
            }
        }
        bx += 22 + slider_w + 6;
    }

    // ── Fade Out slider ───────────────────────────────────────────────
    {
        let slider_w = 50i32;
        let slider_h = 16i32;
        let slider_x = bx;
        let slider_y = top + 6;
        let max_fade = total_secs.min(10.0) as f32;
        let mut fade_val = (state.audio_editor_fade_out as f32).clamp(0.0, max_fade);
        draw_pixel_label(
            canvas,
            &state.theme,
            "FOUT",
            slider_x,
            slider_y - 1,
            28,
            Theme::c(state.theme.text_secondary),
        );
        let fade_changed = slider(
            canvas,
            input,
            &state.theme,
            &SliderParams {
                id: WidgetId::Auto(7076),
                x: slider_x + 30,
                y: slider_y,
                width: slider_w,
                height: slider_h,
                min: 0.0,
                max: max_fade,
                orientation: SliderOrientation::Horizontal,
                label: Some(format!("{:.2}s", fade_val)),
                default_value: Some(0.0),
            },
            &mut fade_val,
        );
        if fade_changed {
            state.audio_editor_fade_out = fade_val as f64;
            // Write back to the clip model
            if let Some((track_id, clip_idx)) = state.selected_clip {
                if let Some(t) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(crate::models::Clip::Audio(ac)) = t.clips.get_mut(clip_idx) {
                        ac.fade_out = fade_val as f64;
                        state.dirty = true;
                    }
                }
            }
        }
        bx += 30 + slider_w + 8;
    }

    // ── Export button ────────────────────────────────────────────────
    {
        let export_id = input.next_id();
        let export_w = 52i32;
        let export_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: export_id,
                x: bx,
                y: top + 4,
                width: export_w,
                height: 20,
                label: "EXP".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Export audio clip to WAV file".into()),
                ..Default::default()
            },
        );
        if export_clicked && !source_file.is_empty() {
            // Populate the export popup with a default filename
            let src_path = std::path::Path::new(&source_file);
            let stem = src_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("audio");
            state.audio_export_name = format!("{}_export.wav", stem);
            state.audio_export_source = source_file.clone();
            // Default export directory to source file's parent
            state.audio_export_dir = src_path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            state.audio_export_popup_open = true;
        }
        bx += export_w + 4;
    }

    // ── Effects dropdown + APPLY button (right side of toolbar) ──────
    let audio_fx_labels: Vec<&str> = vec![
        "Reverse",
        "Fade In",
        "Fade Out",
        "Silence",
        "Gain +6dB",
        "Gain -6dB",
        "Invert",
    ];
    let fx_dropdown_w = 80i32;
    let apply_w = 50i32;
    let fx_area_w = fx_dropdown_w + 4 + apply_w;
    let fx_x = w - fx_area_w - 8;
    {
        let _changed = dropdown(
            canvas,
            input,
            &state.theme,
            7074,
            fx_x,
            top + 4,
            fx_dropdown_w,
            20,
            &audio_fx_labels,
            &mut state.audio_editor_effect_idx,
            &mut state.dropdown_open_id,
        );
    }
    {
        let apply_id = input.next_id();
        // B key triggers apply when audio editor is visible (no text field active)
        let apply_key_triggered = state.text_field_active_id == 0
            && !input.shift()
            && !input.ctrl()
            && input.key_available(sdl2::keyboard::Keycode::B);
        if apply_key_triggered {
            input.consume_key(sdl2::keyboard::Keycode::B);
        }
        let apply_clicked = button(
            canvas,
            input,
            &state.theme,
            &ButtonParams {
                id: apply_id,
                x: fx_x + fx_dropdown_w + 4,
                y: top + 4,
                width: apply_w,
                height: 20,
                label: "APPLY".into(),
                toggled: false,
                icon: ButtonIcon::None,
                hint: Some("Apply selected effect to selection (B)".into()),
                ..Default::default()
            },
        );
        if apply_clicked || apply_key_triggered {
            let (sel_s, sel_e) = state
                .audio_editor_selection
                .map(|(a, b)| (a.min(b), a.max(b)))
                .unwrap_or((0.0, total_secs));
            if (sel_e - sel_s) > 0.001 {
                let path = std::path::Path::new(&source_file);
                if let Ok((raw, channels, sr)) = crate::audio::load_audio_interleaved(path) {
                    let total_frames = raw.len() / channels.max(1);
                    let start_frame = ((sel_s * sr as f64) as usize).min(total_frames);
                    let end_frame = ((sel_e * sr as f64) as usize).min(total_frames);
                    if end_frame > start_frame {
                        let make_undo_backup_fx = |src: &str| -> Result<String, String> {
                            let sp = std::path::Path::new(src);
                            let dir = sp.parent().unwrap_or(std::path::Path::new("."));
                            let stem = sp.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
                            let ext = sp.extension().and_then(|s| s.to_str()).unwrap_or("wav");
                            let ts = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0);
                            let backup_name = format!(".{}_undo_{}.{}", stem, ts, ext);
                            let backup_path = dir.join(backup_name);
                            std::fs::copy(src, &backup_path)
                                .map(|_| backup_path.to_string_lossy().to_string())
                                .map_err(|e| format!("Backup failed: {}", e))
                        };
                        let fx_idx = state.audio_editor_effect_idx;
                        let fx_name = audio_fx_labels.get(fx_idx).unwrap_or(&"Unknown");
                        match make_undo_backup_fx(&source_file) {
                            Ok(backup) => {
                                state.audio_redo_stack.clear();
                                state.audio_undo_stack.push((
                                    source_file.clone(),
                                    backup,
                                    format!("Apply {}", fx_name),
                                    None,
                                ));
                                let mut modified = raw.clone();
                                let region =
                                    &mut modified[start_frame * channels..end_frame * channels];
                                match fx_idx {
                                    0 => {
                                        // Reverse
                                        if channels >= 2 {
                                            let frame_count = region.len() / channels;
                                            for i in 0..frame_count / 2 {
                                                let j = frame_count - 1 - i;
                                                for ch in 0..channels {
                                                    region
                                                        .swap(i * channels + ch, j * channels + ch);
                                                }
                                            }
                                        } else {
                                            region.reverse();
                                        }
                                    }
                                    1 => {
                                        // Fade In
                                        let frame_count = region.len() / channels;
                                        for i in 0..frame_count {
                                            let gain = i as f32 / frame_count as f32;
                                            for ch in 0..channels {
                                                region[i * channels + ch] *= gain;
                                            }
                                        }
                                    }
                                    2 => {
                                        // Fade Out
                                        let frame_count = region.len() / channels;
                                        for i in 0..frame_count {
                                            let gain = 1.0 - (i as f32 / frame_count as f32);
                                            for ch in 0..channels {
                                                region[i * channels + ch] *= gain;
                                            }
                                        }
                                    }
                                    3 => {
                                        // Silence
                                        for s in region.iter_mut() {
                                            *s = 0.0;
                                        }
                                    }
                                    4 => {
                                        // Gain +6dB (~2x)
                                        let gain = 2.0f32;
                                        for s in region.iter_mut() {
                                            *s = (*s * gain).clamp(-1.0, 1.0);
                                        }
                                    }
                                    5 => {
                                        // Gain -6dB (~0.5x)
                                        let gain = 0.5f32;
                                        for s in region.iter_mut() {
                                            *s *= gain;
                                        }
                                    }
                                    6 => {
                                        // Invert (phase flip)
                                        for s in region.iter_mut() {
                                            *s = -*s;
                                        }
                                    }
                                    _ => {}
                                }
                                let save_result = if channels >= 2 {
                                    crate::audio::save_wav_stereo(path, &modified, sr)
                                } else {
                                    crate::audio::save_wav_mono(path, &modified, sr)
                                };
                                match save_result {
                                    Ok(()) => {
                                        state.waveform_cache.remove(&source_file);
                                        state.waveform_stereo_cache.remove(&source_file);
                                        state.waveform_raw_cache.remove(&source_file);
                                        state.audio_sample_invalidate.push(source_file.clone());
                                        state.push_status(format!(
                                            "{} applied to selection",
                                            fx_name
                                        ));
                                    }
                                    Err(e) => state.push_status(format!("Apply failed: {}", e)),
                                }
                            }
                            Err(e) => state.push_status(e),
                        }
                    }
                }
            } else {
                state.push_status("Select a region first to apply an effect");
            }
        }
    }

    // Clip name + info (between left controls and right FX area)
    let info_max_w = (fx_x - bx - 16).max(10);
    let info_str = format!(
        "{}   file:{:.1}s  window:{:.2}s",
        clip_name, total_secs, clip_len_secs,
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        &info_str,
        bx + 8,
        top + 8,
        info_max_w,
        Theme::c(state.theme.text_primary),
    );

    // ── Loop ruler bar ───────────────────────────────────────────────
    let loop_ruler_top = top + toolbar_h;
    {
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(28, 30, 38, 255));
        let _ = canvas.fill_rect(Rect::new(
            wave_left,
            loop_ruler_top,
            wave_w as u32,
            loop_ruler_h as u32,
        ));

        // Draw loop region if enabled
        let loop_enabled = state.audio_editor_loop_enabled;
        let loop_s = state.audio_editor_loop_start;
        let loop_e = state.audio_editor_loop_end;
        if loop_enabled && loop_e > loop_s {
            let lx1 = sec_to_x(loop_s);
            let lx2 = sec_to_x(loop_e);
            let lc = state.theme.loop_color;

            // Filled region between handles
            {
                let fill_x0 = lx1.max(wave_left);
                let fill_x1 = lx2.min(wave_left + wave_w);
                if fill_x1 > fill_x0 {
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], 45));
                    let _ = canvas.fill_rect(Rect::new(
                        fill_x0,
                        loop_ruler_top,
                        (fill_x1 - fill_x0) as u32,
                        loop_ruler_h as u32,
                    ));
                }
            }

            // Left edge line
            if lx1 >= wave_left && lx1 <= wave_left + wave_w {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], 200));
                let _ = canvas.fill_rect(Rect::new(lx1, loop_ruler_top, 2, loop_ruler_h as u32));
            }
            // Right edge line
            if lx2 >= wave_left && lx2 <= wave_left + wave_w {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], 200));
                let _ =
                    canvas.fill_rect(Rect::new(lx2 - 1, loop_ruler_top, 2, loop_ruler_h as u32));
            }
        }

        // "LOOP" label
        canvas.set_clip_rect(Some(Rect::new(
            wave_left,
            loop_ruler_top,
            wave_w as u32,
            loop_ruler_h as u32,
        )));
        if !loop_enabled || loop_e <= loop_s {
            draw_pixel_label(
                canvas,
                &state.theme,
                "LOOP",
                wave_left + 2,
                loop_ruler_top + 2,
                40,
                sdl2::pixels::Color::RGBA(60, 65, 80, 120),
            );
        }
        canvas.set_clip_rect(None);

        // Bottom border
        canvas.set_draw_color(Theme::c(state.theme.panel_border));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(wave_left, loop_ruler_top + loop_ruler_h - 1),
            sdl2::rect::Point::new(wave_left + wave_w, loop_ruler_top + loop_ruler_h - 1),
        );
    }

    // ── Time ruler (in beats) ───────────────────────────────────────
    let ruler_top = loop_ruler_top + loop_ruler_h;
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(35, 38, 48, 255));
    let _ = canvas.fill_rect(Rect::new(
        wave_left,
        ruler_top,
        wave_w as u32,
        ruler_h as u32,
    ));

    canvas.set_clip_rect(Some(Rect::new(
        wave_left,
        ruler_top,
        wave_w as u32,
        ruler_h as u32,
    )));
    if total_secs > 0.0 && bpm > 0.0 {
        let beat_dur = 60.0 / bpm;
        // Choose a beat subdivision for ruler ticks based on zoom
        // zoom is px/sec; beat_px = beat_dur * zoom
        let beat_px = beat_dur * zoom;
        // Pick subdivision: 4 beats (bar), 1 beat, 1/2, 1/4, 1/8
        let sub_beat = if beat_px < 8.0 {
            4.0 // show bars only
        } else if beat_px < 20.0 {
            1.0
        } else if beat_px < 50.0 {
            0.5
        } else if beat_px < 100.0 {
            0.25
        } else {
            0.125
        };
        let sub_dur = sub_beat * beat_dur;
        let first = (scroll / sub_dur).floor() * sub_dur;
        let mut t = first;
        while t <= scroll + visible_secs + sub_dur {
            if t >= 0.0 && t <= total_secs + 0.001 {
                let x = sec_to_x(t);
                let beat_num = t / beat_dur;
                let is_bar = (beat_num.round() as i64 % 4 == 0)
                    && (beat_num - beat_num.round()).abs() < 0.01;
                let is_beat = (beat_num - beat_num.round()).abs() < 0.01;
                let tick_h = if is_bar {
                    ruler_h - 4
                } else if is_beat {
                    ruler_h / 2 + 2
                } else {
                    ruler_h / 3
                };
                canvas.set_draw_color(if is_bar {
                    sdl2::pixels::Color::RGBA(160, 160, 180, 200)
                } else if is_beat {
                    sdl2::pixels::Color::RGBA(110, 115, 130, 180)
                } else {
                    sdl2::pixels::Color::RGBA(80, 85, 100, 140)
                });
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(x, ruler_top + ruler_h - tick_h),
                    sdl2::rect::Point::new(x, ruler_top + ruler_h - 1),
                );
                // Label at bar and beat boundaries
                if is_bar && x + 2 < wave_left + wave_w {
                    let bar = (beat_num.round() as i64 / 4) + 1;
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &format!("{}", bar),
                        x + 2,
                        ruler_top + 3,
                        30,
                        sdl2::pixels::Color::RGBA(160, 165, 180, 220),
                    );
                } else if is_beat && beat_px >= 20.0 && x + 2 < wave_left + wave_w {
                    let bar = (beat_num.round() as i64 / 4) + 1;
                    let beat_in_bar = (beat_num.round() as i64 % 4) + 1;
                    draw_pixel_label(
                        canvas,
                        &state.theme,
                        &format!("{}.{}", bar, beat_in_bar),
                        x + 2,
                        ruler_top + 3,
                        40,
                        sdl2::pixels::Color::RGBA(120, 125, 140, 180),
                    );
                }
            }
            t += sub_dur;
        }
    }
    canvas.set_clip_rect(None);
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(wave_left, ruler_top + ruler_h - 1),
        sdl2::rect::Point::new(wave_left + wave_w, ruler_top + ruler_h - 1),
    );

    // ── Selection range handles on ruler ────────────────────────────
    if let Some((sel_s, sel_e)) = state.audio_editor_selection {
        let s = sel_s.min(sel_e);
        let e = sel_s.max(sel_e);
        let sx = sec_to_x(s);
        let ex = sec_to_x(e);
        let ruler_left = wave_left;
        let ruler_right = wave_left + wave_w;
        canvas.set_clip_rect(Some(Rect::new(
            ruler_left,
            ruler_top,
            wave_w as u32,
            ruler_h as u32,
        )));

        let fill_x0 = sx.max(ruler_left);
        let fill_x1 = ex.min(ruler_right);
        if fill_x1 > fill_x0 {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 160, 40, 50));
            let _ = canvas.fill_rect(Rect::new(
                fill_x0,
                ruler_top,
                (fill_x1 - fill_x0) as u32,
                ruler_h as u32,
            ));
        }

        let accent = sdl2::pixels::Color::RGBA(255, 160, 40, 230);
        canvas.set_draw_color(accent);
        if sx >= ruler_left && sx < ruler_right {
            let _ = canvas.fill_rect(Rect::new(sx, ruler_top, 2, ruler_h as u32));
            let _ = canvas.fill_rect(Rect::new(sx, ruler_top, 6, 3));
        }
        if ex >= ruler_left && ex <= ruler_right && ex != sx {
            let _ = canvas.fill_rect(Rect::new(ex - 1, ruler_top, 2, ruler_h as u32));
            let _ = canvas.fill_rect(Rect::new(ex - 5, ruler_top, 6, 3));
        }
        canvas.set_clip_rect(None);
    }
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 20, 26, 255));
    let _ = canvas.fill_rect(Rect::new(wave_left, wave_top, wave_w as u32, wave_h as u32));

    // ── Clip window shading ──────────────────────────────────────────
    let win_x0 = sec_to_x(clip_win_start_secs).clamp(wave_left, wave_left + wave_w);
    let win_x1 = sec_to_x(clip_win_end_secs).clamp(wave_left, wave_left + wave_w);
    if win_x0 > wave_left {
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(
            wave_left,
            wave_top,
            (win_x0 - wave_left) as u32,
            wave_h as u32,
        ));
    }
    if win_x1 < wave_left + wave_w {
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(
            win_x1,
            wave_top,
            (wave_left + wave_w - win_x1) as u32,
            wave_h as u32,
        ));
    }
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, 200));
    if win_x0 > wave_left && win_x0 < wave_left + wave_w {
        let _ = canvas.fill_rect(Rect::new(win_x0, wave_top, 2, wave_h as u32));
    }
    if win_x1 > wave_left && win_x1 < wave_left + wave_w {
        let _ = canvas.fill_rect(Rect::new(win_x1 - 2, wave_top, 2, wave_h as u32));
    }

    // Channel separator + center lines
    let ch_sep_y = wave_top + ch_h;
    let ch0_center = wave_top + ch_h / 2;
    let ch1_center = wave_top + ch_h + ch_h / 2;
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 180));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(wave_left, ch_sep_y),
        sdl2::rect::Point::new(wave_left + wave_w, ch_sep_y),
    );
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
    for cy in [ch0_center, ch1_center] {
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(wave_left, cy),
            sdl2::rect::Point::new(wave_left + wave_w, cy),
        );
    }

    // ── dB grid lines ──
    {
        let half = (ch_h / 2 - 2).max(1) as f32;
        let db_levels = [("-6", 0.5f32), ("-12", 0.25f32)];
        for &(label, linear) in db_levels.iter() {
            let amp = (linear * half) as i32;
            for &center_y in &[ch0_center, ch1_center] {
                for offset in &[amp, -amp] {
                    let ly = center_y - offset;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(55, 60, 75, 60));
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(wave_left + 22, ly),
                        sdl2::rect::Point::new(wave_left + wave_w, ly),
                    );
                }
            }
            let ly = ch0_center - (linear * half) as i32;
            draw_pixel_label(
                canvas,
                &state.theme,
                label,
                wave_left + 2,
                ly - 4,
                18,
                sdl2::pixels::Color::RGBA(80, 90, 110, 120),
            );
        }
    }
    draw_pixel_label(
        canvas,
        &state.theme,
        "L",
        wave_left + 2,
        wave_top + 2,
        10,
        sdl2::pixels::Color::RGBA(80, 90, 110, 180),
    );
    draw_pixel_label(
        canvas,
        &state.theme,
        "R",
        wave_left + 2,
        ch_sep_y + 2,
        10,
        sdl2::pixels::Color::RGBA(80, 90, 110, 180),
    );

    // ── Selection highlight ──────────────────────────────────────────
    if let Some((sel_s, sel_e)) = state.audio_editor_selection {
        let s = sel_s.min(sel_e);
        let e = sel_s.max(sel_e);
        let sx = sec_to_x(s).max(wave_left);
        let ex = sec_to_x(e).min(wave_left + wave_w);
        let sw = (ex - sx).max(0);
        if sw > 0 {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 140, 255, 50));
            let _ = canvas.fill_rect(Rect::new(sx, wave_top, sw as u32, wave_h as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 180));
            let _ = canvas.fill_rect(Rect::new(sx, wave_top, 1, wave_h as u32));
            let _ = canvas.fill_rect(Rect::new(
                ex.min(wave_left + wave_w - 1),
                wave_top,
                1,
                wave_h as u32,
            ));
        }
    }

    // ── Draw waveform ────────────────────────────────────────────────
    canvas.set_clip_rect(Some(Rect::new(
        wave_left,
        wave_top,
        wave_w as u32,
        wave_h as u32,
    )));
    let raw_data = state.waveform_raw_cache.get(&source_file);
    let stereo_data = state.waveform_stereo_cache.get(&source_file);
    if let Some((ref left_raw, ref right_raw, raw_sr)) = raw_data {
        // High-resolution rendering from raw samples
        let num_samples = left_raw.len();
        if num_samples > 0 && wave_w > 2 && total_secs > 0.0 {
            let half0 = (ch_h / 2 - 2).max(1) as f32;
            let half1 = half0;
            let sr_f64 = *raw_sr as f64;
            for px_i in 0..wave_w as usize {
                let sec0 = x_to_sec(wave_left + px_i as i32);
                let sec1 = x_to_sec(wave_left + px_i as i32 + 1);
                if sec1 < 0.0 || sec0 > total_secs {
                    continue;
                }
                let s0 = ((sec0 * sr_f64) as usize).min(num_samples.saturating_sub(1));
                let s1 = ((sec1 * sr_f64) as usize).min(num_samples).max(s0 + 1);
                // Compute per-pixel min/max from raw samples
                let mut l_px_max = f32::NEG_INFINITY;
                let mut l_px_min = f32::INFINITY;
                let mut r_px_max = f32::NEG_INFINITY;
                let mut r_px_min = f32::INFINITY;
                for si in s0..s1 {
                    let ls = left_raw[si];
                    let rs = right_raw[si];
                    if ls > l_px_max {
                        l_px_max = ls;
                    }
                    if ls < l_px_min {
                        l_px_min = ls;
                    }
                    if rs > r_px_max {
                        r_px_max = rs;
                    }
                    if rs < r_px_min {
                        r_px_min = rs;
                    }
                }
                if l_px_max == f32::NEG_INFINITY {
                    continue;
                }
                let in_window = sec0 >= clip_win_start_secs && sec0 <= clip_win_end_secs;
                let (wave_r, wave_g, wave_b, alpha) = if in_window {
                    (70u8, 200u8, 130u8, 230u8)
                } else {
                    (50u8, 110u8, 80u8, 140u8)
                };
                let bx = wave_left + px_i as i32;
                let lmx = ((l_px_max * clip_gain).min(1.0) * half0) as i32;
                let lmn = ((l_px_min * clip_gain).max(-1.0) * half0) as i32;
                let l_top = ch0_center - lmx;
                let l_bot = ch0_center - lmn;
                let lh = (l_bot - l_top).max(1);
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(wave_r, wave_g, wave_b, alpha));
                let _ = canvas.fill_rect(Rect::new(bx, l_top, 1, lh as u32));
                let rmx = ((r_px_max * clip_gain).min(1.0) * half1) as i32;
                let rmn = ((r_px_min * clip_gain).max(-1.0) * half1) as i32;
                let r_top = ch1_center - rmx;
                let r_bot = ch1_center - rmn;
                let rh = (r_bot - r_top).max(1);
                let _ = canvas.fill_rect(Rect::new(bx, r_top, 1, rh as u32));
            }
        }
    } else if let Some((ref l_max, ref l_min, ref r_max, ref r_min)) = stereo_data {
        // Fallback: use cached peaks (lower resolution)
        let num_peaks = l_max.len();
        if num_peaks > 0 && wave_w > 2 && total_secs > 0.0 {
            let half0 = (ch_h / 2 - 2).max(1) as f32;
            let half1 = half0;
            for px_i in 0..wave_w as usize {
                let sec = x_to_sec(wave_left + px_i as i32);
                if sec < 0.0 || sec > total_secs {
                    continue;
                }
                let frac = sec / total_secs;
                let idx = ((frac * num_peaks as f64) as usize).min(num_peaks - 1);
                let in_window = sec >= clip_win_start_secs && sec <= clip_win_end_secs;
                let (wave_r, wave_g, wave_b, alpha) = if in_window {
                    (70u8, 200u8, 130u8, 230u8)
                } else {
                    (50u8, 110u8, 80u8, 140u8)
                };
                let bx = wave_left + px_i as i32;
                let lmx = ((l_max[idx] * clip_gain).min(1.0) * half0) as i32;
                let lmn = ((l_min[idx] * clip_gain).max(-1.0) * half0) as i32;
                let l_top = ch0_center - lmx;
                let l_bot = ch0_center - lmn;
                let lh = (l_bot - l_top).max(1);
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(wave_r, wave_g, wave_b, alpha));
                let _ = canvas.fill_rect(Rect::new(bx, l_top, 1, lh as u32));
                let rmx = ((r_max[idx] * clip_gain).min(1.0) * half1) as i32;
                let rmn = ((r_min[idx] * clip_gain).max(-1.0) * half1) as i32;
                let r_top = ch1_center - rmx;
                let r_bot = ch1_center - rmn;
                let rh = (r_bot - r_top).max(1);
                let _ = canvas.fill_rect(Rect::new(bx, r_top, 1, rh as u32));
            }
        }
    } else if !source_file.is_empty() {
        draw_pixel_label(
            canvas,
            &state.theme,
            "loading waveform...",
            wave_left + wave_w / 2 - 60,
            ch0_center - 5,
            140,
            sdl2::pixels::Color::RGBA(100, 180, 130, 150),
        );
    }
    canvas.set_clip_rect(None);

    // ── Grid lines (beat-based, matching ruler) ─────────────────────
    canvas.set_clip_rect(Some(Rect::new(
        wave_left,
        wave_top,
        wave_w as u32,
        wave_h as u32,
    )));
    if total_secs > 0.0 && bpm > 0.0 {
        let beat_dur = 60.0 / bpm;
        let beat_px = beat_dur * zoom;
        // Use the snap resolution for grid subdivision, but fall back to
        // beat-density–based stepping for overall readability
        let snap_div_beats_grid = SNAP_RESOLUTIONS[state.audio_editor_snap_idx].1;
        let grid_beat = if state.audio_editor_snap_enabled {
            snap_div_beats_grid
        } else if beat_px < 8.0 {
            4.0
        } else if beat_px < 20.0 {
            1.0
        } else if beat_px < 50.0 {
            0.5
        } else if beat_px < 100.0 {
            0.25
        } else {
            0.125
        };
        let grid_dur = grid_beat * beat_dur;
        let first = (scroll / grid_dur).floor() * grid_dur;
        let mut t = if first <= 0.0 { grid_dur } else { first };
        while t < total_secs && t <= scroll + visible_secs + grid_dur {
            let x = sec_to_x(t);
            let beat_num = t / beat_dur;
            let is_bar =
                (beat_num.round() as i64 % 4 == 0) && (beat_num - beat_num.round()).abs() < 0.01;
            let is_beat = (beat_num - beat_num.round()).abs() < 0.01;
            canvas.set_draw_color(if is_bar {
                sdl2::pixels::Color::RGBA(70, 75, 95, 70)
            } else if is_beat {
                sdl2::pixels::Color::RGBA(55, 60, 78, 45)
            } else {
                sdl2::pixels::Color::RGBA(45, 48, 60, 30)
            });
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(x, wave_top),
                sdl2::rect::Point::new(x, wave_top + wave_h),
            );
            t += grid_dur;
        }
    }
    canvas.set_clip_rect(None);

    // ── Playhead (independent audio editor playhead) ─────────────────
    {
        let ph_sec = state.audio_editor_playhead;
        if ph_sec >= 0.0 && ph_sec <= total_secs {
            let cx = sec_to_x(ph_sec);
            if cx >= wave_left && cx <= wave_left + wave_w {
                canvas.set_draw_color(Theme::c(state.theme.playhead));
                let _ = canvas.fill_rect(Rect::new(cx, ruler_top, 1, (ruler_h + wave_h) as u32));
                // Triangle indicator at top of time ruler
                let tri_sz = 4i32;
                for dy in 0..tri_sz {
                    let half = dy;
                    let _ = canvas.fill_rect(Rect::new(
                        cx - half,
                        ruler_top + dy,
                        (half * 2 + 1) as u32,
                        1,
                    ));
                }
            }
        }
    }

    // ── Also draw loop region highlight on waveform (edge lines only) ──
    if state.audio_editor_loop_enabled
        && state.audio_editor_loop_end > state.audio_editor_loop_start
    {
        let lx1 = sec_to_x(state.audio_editor_loop_start).max(wave_left);
        let lx2 = sec_to_x(state.audio_editor_loop_end).min(wave_left + wave_w);
        if lx2 > lx1 {
            let lc = state.theme.loop_color;
            // Left edge
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(lc[0], lc[1], lc[2], 120));
            let _ = canvas.fill_rect(Rect::new(lx1, wave_top, 2, wave_h as u32));
            // Right edge
            let _ = canvas.fill_rect(Rect::new(lx2 - 1, wave_top, 2, wave_h as u32));
        }
    }

    // ── Fade in/out visual overlays ─────────────────────────────────
    {
        let fade_in_secs = state.audio_editor_fade_in;
        let fade_out_secs = state.audio_editor_fade_out;

        // Fade in: thin ramp line from bottom-left to top at fade_in_secs
        if fade_in_secs > 0.0 {
            let x_start = sec_to_x(0.0).max(wave_left);
            let x_end = sec_to_x(fade_in_secs).min(wave_left + wave_w);
            if x_end > x_start {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 200, 80, 200));
                let steps = (x_end - x_start).max(1);
                for i in 0..steps {
                    let x = x_start + i;
                    let frac = i as f32 / steps as f32;
                    let y = wave_top + wave_h - (frac * wave_h as f32) as i32;
                    // Draw 2px wide for visibility
                    let _ = canvas.fill_rect(Rect::new(x, y, 1, 2));
                }
            }
        }

        // Fade out: thin ramp line from top at (total_secs - fade_out) to bottom-right
        if fade_out_secs > 0.0 {
            let fo_start = (total_secs - fade_out_secs).max(0.0);
            let x_start = sec_to_x(fo_start).max(wave_left);
            let x_end = sec_to_x(total_secs).min(wave_left + wave_w);
            if x_end > x_start {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 120, 80, 200));
                let steps = (x_end - x_start).max(1);
                for i in 0..steps {
                    let x = x_start + i;
                    let frac = i as f32 / steps as f32;
                    let y = wave_top + (frac * wave_h as f32) as i32;
                    // Draw 2px wide for visibility
                    let _ = canvas.fill_rect(Rect::new(x, y, 1, 2));
                }
            }
        }
    }

    // ── Waveform border ──────────────────────────────────────────────
    canvas.set_draw_color(Theme::c(state.theme.panel_border));
    let _ = canvas.draw_rect(Rect::new(wave_left, wave_top, wave_w as u32, wave_h as u32));

    // ── Info bar at bottom ───────────────────────────────────────────
    let info_y = wave_top + wave_h + scroll_bar_h + 2;
    let sec_to_bar_beat = |s: f64| -> String {
        if bpm > 0.0 {
            let beat_dur = 60.0 / bpm;
            let beat = s / beat_dur;
            let bar = (beat as i64 / 4) + 1;
            let b_in_bar = (beat as i64 % 4) + 1;
            format!("{}.{}", bar, b_in_bar)
        } else {
            format!("{:.3}s", s)
        }
    };
    let sel_info = if let Some((ss, se)) = state.audio_editor_selection {
        let s = ss.min(se);
        let e = ss.max(se);
        let dur = e - s;
        let dur_beats = if bpm > 0.0 { dur * bpm / 60.0 } else { 0.0 };
        format!(
            "Sel: {} – {}  ({:.2} beats)  |  Playhead: {}  |  Snap: {}",
            sec_to_bar_beat(s),
            sec_to_bar_beat(e),
            dur_beats,
            sec_to_bar_beat(state.audio_editor_playhead),
            SNAP_RESOLUTIONS[state.audio_editor_snap_idx].0,
        )
    } else {
        format!(
            "File: {:.2}s  |  Playhead: {}  |  BPM: {:.0}  |  Snap: {}",
            total_secs,
            sec_to_bar_beat(state.audio_editor_playhead),
            bpm,
            SNAP_RESOLUTIONS[state.audio_editor_snap_idx].0,
        )
    };
    draw_pixel_label(
        canvas,
        &state.theme,
        &sel_info,
        wave_left + 4,
        info_y + 3,
        wave_w - 8,
        Theme::c(state.theme.text_secondary),
    );

    // ── Horizontal scrollbar / scroomer ─────────────────────────────
    {
        let sb_y = wave_top + wave_h;
        if total_secs > 0.0 {
            let thumb_ratio = (visible_secs / total_secs).clamp(0.02, 1.0) as f32;
            let scroll_frac = if max_scroll_secs > 0.0 {
                (scroll / max_scroll_secs).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };

            let (new_frac, new_ratio) = scrollbar_with_squeeze(
                canvas,
                input,
                &state.theme,
                WidgetId::Auto(7060),
                WidgetId::Auto(7061),
                WidgetId::Auto(7062),
                wave_left,
                sb_y,
                wave_w,
                scroll_bar_h,
                ScrollbarDir::Horizontal,
                scroll_frac,
                thumb_ratio,
            );
            let ratio_changed = (new_ratio - thumb_ratio).abs() > 0.001;
            let frac_changed = (new_frac - scroll_frac).abs() > 0.001;
            if ratio_changed {
                let new_visible_secs = (new_ratio as f64 * total_secs).max(0.001);
                let new_zoom = (wave_w as f64 / new_visible_secs).clamp(1.0, 4000.0);
                state.audio_editor_zoom = new_zoom;
            }
            if ratio_changed || frac_changed {
                let cur_zoom = state.audio_editor_zoom;
                let new_max_scroll = (total_secs - wave_w as f64 / cur_zoom).max(0.0);
                state.audio_editor_scroll =
                    (new_frac as f64 * new_max_scroll).clamp(0.0, new_max_scroll);
            }
        }
    }

    // ── Mouse interaction ────────────────────────────────────────────
    let in_loop_ruler = input.mouse_in_rect(wave_left, loop_ruler_top, wave_w, loop_ruler_h);
    let in_ruler = input.mouse_in_rect(wave_left, ruler_top, wave_w, ruler_h);
    let in_wave = input.mouse_in_rect(wave_left, wave_top, wave_w, wave_h);

    // Snapping helper
    let snap_div_beats = SNAP_RESOLUTIONS[state.audio_editor_snap_idx].1;
    let snap_div_secs = snap_div_beats * 60.0 / bpm.max(1.0);
    let audio_snap_enabled = state.audio_editor_snap_enabled;
    let snap_sec = |sec: f64| -> f64 {
        if audio_snap_enabled {
            (sec / snap_div_secs).round() * snap_div_secs
        } else {
            sec
        }
    };

    // ── Loop ruler interaction ───────────────────────────────────────
    // Click-drag on loop ruler creates/adjusts loop region
    let handle_hit_loop = 6i32;
    if in_loop_ruler && input.mouse_pressed && input.drag_widget == WidgetId::None {
        let mx = input.mouse_x;
        let loop_s = state.audio_editor_loop_start;
        let loop_e = state.audio_editor_loop_end;
        let ls_x = sec_to_x(loop_s);
        let le_x = sec_to_x(loop_e);
        let near_start = state.audio_editor_loop_enabled
            && loop_e > loop_s
            && (mx - ls_x).abs() <= handle_hit_loop;
        let near_end = state.audio_editor_loop_enabled
            && loop_e > loop_s
            && (mx - le_x).abs() <= handle_hit_loop;

        if near_start && !near_end {
            input.drag_widget = WidgetId::Auto(7080);
            input.active_widget = WidgetId::Auto(7080);
        } else if near_end && !near_start {
            input.drag_widget = WidgetId::Auto(7081);
            input.active_widget = WidgetId::Auto(7081);
        } else {
            // Start new loop region
            let sec = snap_sec(x_to_sec(mx).clamp(0.0, total_secs));
            state.audio_editor_loop_start = sec;
            state.audio_editor_loop_end = sec;
            state.audio_editor_loop_enabled = true;
            input.drag_widget = WidgetId::Auto(7082);
            input.active_widget = WidgetId::Auto(7082);
            input.drag_start_value = sec;
        }
        state.focused_panel = crate::state::FocusedPanel::AudioEditor;
    }
    // Drag loop start handle
    if input.drag_widget == WidgetId::Auto(7080) && input.mouse_down {
        let sec = snap_sec(x_to_sec(input.mouse_x).clamp(0.0, total_secs));
        state.audio_editor_loop_start = sec.min(state.audio_editor_loop_end - 0.01);
    }
    // Drag loop end handle
    if input.drag_widget == WidgetId::Auto(7081) && input.mouse_down {
        let sec = snap_sec(x_to_sec(input.mouse_x).clamp(0.0, total_secs));
        state.audio_editor_loop_end = sec.max(state.audio_editor_loop_start + 0.01);
    }
    // Drag new loop region (from click)
    if input.drag_widget == WidgetId::Auto(7082) && input.mouse_down {
        let sec = snap_sec(x_to_sec(input.mouse_x).clamp(0.0, total_secs));
        let anchor = input.drag_start_value;
        let (lo, hi) = if sec < anchor {
            (sec, anchor)
        } else {
            (anchor, sec)
        };
        state.audio_editor_loop_start = lo.max(0.0);
        state.audio_editor_loop_end = hi.max(lo + 0.01);
    }
    // Right-click on loop ruler: disable loop
    if in_loop_ruler && input.right_mouse_pressed {
        state.audio_editor_loop_enabled = false;
    }

    // ── Selection handle dragging (on ruler) ─────────────────────────
    let handle_hit_px = 7i32;
    if in_ruler && input.mouse_pressed && input.drag_widget == WidgetId::None {
        if let Some((sel_s, sel_e)) = state.audio_editor_selection {
            let s = sel_s.min(sel_e);
            let e = sel_s.max(sel_e);
            let sx = sec_to_x(s);
            let ex = sec_to_x(e);
            let mx = input.mouse_x;
            if (mx - sx).abs() <= handle_hit_px {
                input.drag_widget = WidgetId::Auto(7051);
                input.active_widget = WidgetId::Auto(7051);
                input.drag_start_value = s;
                state.audio_editor_selection = Some((e, s));
            } else if (mx - ex).abs() <= handle_hit_px {
                input.drag_widget = WidgetId::Auto(7052);
                input.active_widget = WidgetId::Auto(7052);
                input.drag_start_value = e;
                state.audio_editor_selection = Some((s, e));
            }
        }
    }
    if input.drag_widget == WidgetId::Auto(7051) && input.mouse_down {
        let sec = snap_sec(x_to_sec(input.mouse_x).clamp(0.0, total_secs));
        if let Some(ref mut sel) = state.audio_editor_selection {
            sel.1 = sec;
        }
    }
    if input.drag_widget == WidgetId::Auto(7052) && input.mouse_down {
        let sec = snap_sec(x_to_sec(input.mouse_x).clamp(0.0, total_secs));
        if let Some(ref mut sel) = state.audio_editor_selection {
            sel.1 = sec;
        }
    }

    // Ruler click (not on handle): set audio editor playhead
    let on_handle = if let Some((sel_s, sel_e)) = state.audio_editor_selection {
        let s = sel_s.min(sel_e);
        let e = sel_s.max(sel_e);
        let mx = input.mouse_x;
        (mx - sec_to_x(s)).abs() <= handle_hit_px || (mx - sec_to_x(e)).abs() <= handle_hit_px
    } else {
        false
    };

    if in_ruler
        && input.mouse_pressed
        && !on_handle
        && input.drag_widget != WidgetId::Auto(7051)
        && input.drag_widget != WidgetId::Auto(7052)
    {
        let sec = x_to_sec(input.mouse_x).clamp(0.0, total_secs);
        state.audio_editor_playhead = sec;
        state.focused_panel = crate::state::FocusedPanel::AudioEditor;
        // Start a drag so subsequent mouse_down frames also update the playhead
        input.drag_widget = WidgetId::Auto(7095);
        input.active_widget = WidgetId::Auto(7095);
    }
    // Playhead drag: update position while mouse held on ruler
    if input.drag_widget == WidgetId::Auto(7095) && input.mouse_down {
        let sec = x_to_sec(input.mouse_x).clamp(0.0, total_secs);
        state.audio_editor_playhead = sec;
    }

    // Wave: Ctrl+click-drag with existing selection → drag region to arranger
    if in_wave && input.mouse_pressed && input.ctrl() && !source_file.is_empty() {
        if let Some((sel_s, sel_e)) = state.audio_editor_selection {
            let s = sel_s.min(sel_e);
            let e = sel_s.max(sel_e);
            if (e - s) > 0.001 {
                state.audio_drag_to_arranger = true;
                state.audio_drag_source = source_file.clone();
                state.audio_drag_offset = s;
                state.audio_drag_length_secs = e - s;
                input.drag_widget = WidgetId::Auto(7090);
                input.active_widget = WidgetId::Auto(7090);
            }
        }
    }

    // Wave: click-drag to select range (only when not Ctrl+dragging to arranger)
    if in_wave
        && input.mouse_pressed
        && !input.ctrl()
        && input.drag_widget != WidgetId::Auto(7051)
        && input.drag_widget != WidgetId::Auto(7052)
        && input.drag_widget != WidgetId::Auto(7090)
    {
        let sec = snap_sec(x_to_sec(input.mouse_x).clamp(0.0, total_secs));
        state.audio_editor_selection = Some((sec, sec));
        state.focused_panel = crate::state::FocusedPanel::AudioEditor;
        input.drag_widget = WidgetId::Auto(7050);
        input.active_widget = WidgetId::Auto(7050);
        input.drag_start_value = sec;
    }
    if input.drag_widget == WidgetId::Auto(7050) && input.mouse_down {
        let sec = snap_sec(x_to_sec(input.mouse_x).clamp(0.0, total_secs));
        if let Some(ref mut sel) = state.audio_editor_selection {
            sel.1 = sec;
        }
    }
    if in_wave && input.right_mouse_pressed {
        state.audio_editor_selection = None;
    }

    // ── Scroll / Zoom ────────────────────────────────────────────────
    if (in_wave || in_ruler || in_loop_ruler) && input.scroll_y != 0 && !input.scroll_consumed {
        if input.ctrl() {
            let factor = if input.scroll_y > 0 { 1.15 } else { 0.87 };
            let old_z = state.audio_editor_zoom;
            let new_z = (old_z * factor).clamp(1.0, 4000.0);
            let cpx = (input.mouse_x - wave_left) as f64;
            let sec_under = state.audio_editor_scroll + cpx / old_z;
            let new_max = (total_secs - wave_w as f64 / new_z).max(0.0);
            state.audio_editor_scroll = (sec_under - cpx / new_z).clamp(0.0, new_max);
            state.audio_editor_zoom = new_z;
        } else {
            let delta = input.scroll_y as f64 * (visible_secs * 0.1);
            state.audio_editor_scroll =
                (state.audio_editor_scroll - delta).clamp(0.0, max_scroll_secs);
        }
    }

    if input.middle_mouse_down
        && (in_wave || in_ruler || in_loop_ruler)
        && input.middle_drag_widget == WidgetId::None
    {
        input.middle_drag_widget = WidgetId::Auto(86100);
    }
    if input.middle_mouse_down && input.middle_drag_widget == WidgetId::Auto(86100) {
        let dx_secs = input.mouse_dx as f64 / zoom;
        state.audio_editor_scroll =
            (state.audio_editor_scroll - dx_secs).clamp(0.0, max_scroll_secs);
    }

    if (in_wave || in_ruler || in_loop_ruler) && input.mouse_pressed {
        state.focused_panel = crate::state::FocusedPanel::AudioEditor;
    }

    // ── Update playhead from preview position ────────────────────────
    // When playing, advance the audio editor playhead based on preview_pos
    if state.audio_editor_playing && state.sample_preview_path.is_none() {
        // Preview finished
        state.audio_editor_playing = false;
    }

    // ── Dropdown popup overlays (draw on top of everything) ──────────
    {
        let snap_labels: Vec<&str> = SNAP_RESOLUTIONS.iter().map(|(l, _)| *l).collect();
        dropdown_popup_overlay(
            canvas,
            &state.theme,
            7073,
            snap_dropdown_x,
            top + 4,
            56,
            20,
            56,
            &snap_labels,
            state.audio_editor_snap_idx,
            state.dropdown_open_id,
            input.mouse_x,
            input.mouse_y,
        );
    }
    {
        let fx_labels: Vec<&str> = vec![
            "Reverse",
            "Fade In",
            "Fade Out",
            "Silence",
            "Gain +6dB",
            "Gain -6dB",
            "Invert",
        ];
        let fx_dropdown_w = 80i32;
        let apply_w = 50i32;
        let fx_area_w = fx_dropdown_w + 4 + apply_w;
        let fx_x = w - fx_area_w - 8;
        dropdown_popup_overlay(
            canvas,
            &state.theme,
            7074,
            fx_x,
            top + 4,
            fx_dropdown_w,
            20,
            fx_dropdown_w,
            &fx_labels,
            state.audio_editor_effect_idx,
            state.dropdown_open_id,
            input.mouse_x,
            input.mouse_y,
        );
    }
}

fn draw_automation_editor(
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
                if let crate::models::Clip::Automation(ac) = c {
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
                        state.mode = crate::state::AppMode::Edit;
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
            if let Some(crate::models::Clip::Automation(auto)) = track.clips.get(clip_idx) {
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
            state.focused_panel = crate::state::FocusedPanel::AutomationEditor;
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
                        Box::new(crate::commands::DeleteAutomationPoint {
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
                        if let Some(crate::models::Clip::Automation(auto)) =
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
                        Box::new(crate::commands::DeleteAutomationPoint {
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
                    if let Some(crate::models::Clip::Automation(auto)) = track.clips.get(clip_idx) {
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
                    Box::new(crate::commands::DeleteAutomationPoint {
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
                    if let Some(crate::models::Clip::Automation(auto)) = track.clips.get(clip_idx) {
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
                        if let Some(crate::models::Clip::Automation(auto)) =
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
                        if let Some(crate::models::Clip::Automation(auto)) =
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
                        Box::new(crate::commands::AddAutomationPoint {
                            track_id,
                            clip_idx,
                            point: crate::models::AutomationPoint {
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
                if let Some(crate::models::Clip::Automation(auto)) = track.clips.get(clip_idx) {
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
            if let Some(crate::models::Clip::Automation(auto)) = track.clips.get_mut(clip_idx) {
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
                    if let Some(crate::models::Clip::Automation(auto)) =
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
                    if let Some(crate::models::Clip::Automation(auto)) =
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
                        if let Some(crate::models::Clip::Automation(auto)) =
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
                                        if let Some(crate::models::Clip::Automation(auto_m)) =
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
                                            if let Some(crate::models::Clip::Automation(auto)) =
                                                track.clips.get(clip_idx)
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
                                        Box::new(crate::commands::MoveAutomationPoint {
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
                        if let Some(crate::models::Clip::Automation(auto)) =
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
                if let Some(crate::models::Clip::Automation(auto)) = track.clips.get(clip_idx) {
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

/// Draw a semi-transparent help/shortcut overlay with tabbed sidebar
pub fn draw_help_screen(canvas: &mut Canvas<Window>, input: &mut InputState, state: &mut AppState) {
    let w = state.window_width as i32;
    let h = state.window_height as i32;

    // Semi-transparent backdrop
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 200));
    let _ = canvas.fill_rect(Rect::new(0, 0, w as u32, h as u32));

    let pw = (w - 40).min(980);
    let ph = (h - 60).min(h - 40);
    let px = (w - pw) / 2;
    let py = (h - ph) / 2;

    // Panel background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(24, 26, 32, 248));
    let _ = canvas.fill_rect(Rect::new(px, py, pw as u32, ph as u32));
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(80, 100, 140, 200));
    let _ = canvas.draw_rect(Rect::new(px, py, pw as u32, ph as u32));

    // Title bar
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 50, 70, 255));
    let _ = canvas.fill_rect(Rect::new(px, py, pw as u32, 20));
    draw_pixel_label(
        canvas,
        &state.theme,
        "Eden DAW  —  Help  (F1 to close)",
        px + 8,
        py + 5,
        pw - 16,
        sdl2::pixels::Color::RGBA(140, 190, 255, 255),
    );

    // ── Tab sidebar ──────────────────────────────────────────────────
    let tab_w = 120i32;
    let tab_labels = [
        "General",
        "Arrangement",
        "Piano Roll",
        "Audio Editor",
        "Automation",
        "Rack",
        "Mixer",
    ];
    let tab_top = py + 24;
    let tab_h = 22i32;

    // Sidebar background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 33, 42, 255));
    let _ = canvas.fill_rect(Rect::new(px, tab_top, tab_w as u32, (ph - 24) as u32));

    for (i, label) in tab_labels.iter().enumerate() {
        let ty = tab_top + i as i32 * tab_h;
        let is_active = state.help_screen_tab == i;
        let hover = input.mouse_in_rect(px, ty, tab_w, tab_h);

        if is_active {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 60, 80, 255));
            let _ = canvas.fill_rect(Rect::new(px, ty, tab_w as u32, tab_h as u32));
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(100, 160, 255, 255));
            let _ = canvas.fill_rect(Rect::new(px, ty, 3, tab_h as u32));
        } else if hover {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 46, 58, 255));
            let _ = canvas.fill_rect(Rect::new(px, ty, tab_w as u32, tab_h as u32));
        }

        let col = if is_active {
            sdl2::pixels::Color::RGBA(180, 210, 255, 255)
        } else {
            sdl2::pixels::Color::RGBA(150, 155, 170, 220)
        };
        draw_pixel_label(
            canvas,
            &state.theme,
            label,
            px + 10,
            ty + 6,
            tab_w - 14,
            col,
        );

        if hover && input.mouse_pressed {
            state.help_screen_tab = i;
            input.mouse_pressed = false;
        }
    }

    // Sidebar divider
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(70, 80, 100, 160));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(px + tab_w, tab_top),
        sdl2::rect::Point::new(px + tab_w, py + ph),
    );

    // ── Content area ─────────────────────────────────────────────────
    let content_x = px + tab_w + 8;
    let content_w = pw - tab_w - 16;
    let content_top = py + 28;
    let content_bot = py + ph - 6;
    let line_h = 13i32;

    let c_section = sdl2::pixels::Color::RGBA(200, 165, 80, 255);
    let c_key = sdl2::pixels::Color::RGBA(255, 220, 140, 230);
    let c_desc = sdl2::pixels::Color::RGBA(178, 184, 200, 220);
    let c_note = sdl2::pixels::Color::RGBA(130, 200, 130, 200);

    // Build content for the active tab
    type HelpEntry = (&'static str, &'static str, bool);
    let entries: Vec<HelpEntry> = match state.help_screen_tab {
        0 => vec![
            // General
            ("── Transport ──", "", true),
            ("Space", "Play / Stop", false),
            (
                "Space (preview playing)",
                "Stops sample preview instead",
                false,
            ),
            ("Enter", "Stop and rewind to start (or loop start)", false),
            ("L", "Toggle loop on / off", false),
            ("", "", false),
            ("── Views ──", "", true),
            ("1", "Arrangement view", false),
            ("2", "Mixer view", false),
            ("3", "Edit / piano-roll view", false),
            ("F1", "Toggle this help screen", false),
            ("Escape", "Deselect / close popup / close help", false),
            ("T", "Cycle colour theme", false),
            ("", "", false),
            ("── Global ──", "", true),
            ("Ctrl+S", "Save project", false),
            ("Right-click Save btn", "Open Save As dialog", false),
            ("Ctrl+Z", "Undo", false),
            ("Ctrl+Shift+Z / Ctrl+R", "Redo", false),
            ("S", "Toggle snap to grid on / off", false),
            ("", "", false),
            ("── Knobs ──", "", true),
            ("Left-drag up / down", "Adjust value", false),
            (
                "Middle-drag up / down",
                "Fine adjustment (5x slower)",
                false,
            ),
            ("Shift+Click", "Reset to default value", false),
            ("Hover", "Shows current value as tooltip", false),
            ("", "", false),
            ("── Sliders ──", "", true),
            ("Left-drag", "Adjust value", false),
            ("Shift+Click", "Reset to default value", false),
            ("", "", false),
            ("── Dropdowns ──", "", true),
            ("Left-click", "Open dropdown / cycle options", false),
            ("Scroll wheel (on dropdown)", "Cycle through options", false),
        ],
        1 => vec![
            // Arrangement
            ("── Navigation ──", "", true),
            ("Middle-drag", "Pan view left/right and up/down", false),
            ("Scroll wheel", "Scroll tracks up / down", false),
            ("Shift+Scroll", "Scroll timeline left / right", false),
            ("Ctrl+Scroll", "Zoom timeline (anchored to cursor)", false),
            ("+ / =  or  -", "Zoom in / out", false),
            ("", "", false),
            ("── Clip Selection ──", "", true),
            ("Left-click clip", "Select clip", false),
            (
                "Shift+Click clip",
                "Add / remove from multi-selection",
                false,
            ),
            ("Ctrl+A", "Select all clips", false),
            ("Ctrl+C / Ctrl+V", "Copy / Paste at playhead", false),
            ("Ctrl+D", "Duplicate selected clips", false),
            ("Delete / Backspace", "Delete selected clips", false),
            ("", "", false),
            ("── Clip Editing ──", "", true),
            ("Drag clip", "Move clip; hold Ctrl to copy", false),
            ("Drag clip up / down", "Move clip to another track", false),
            ("Drag clip edge", "Resize clip (trim start/end)", false),
            (
                "Double-click clip",
                "Open in Piano Roll / Audio Editor",
                false,
            ),
            ("Double-click empty lane", "Create new clip", false),
            (
                "Right-click clip",
                "Delete; hold+drag to erase range",
                false,
            ),
            ("", "", false),
            ("── Loop Region ──", "", true),
            ("Drag ruler", "Set loop region start / end", false),
            ("Right-click ruler", "Clear loop region", false),
            ("", "", false),
            ("── Tracks ──", "", true),
            ("Shift+Up / Down", "Reorder selected track", false),
            ("", "", false),
            ("── Join ──", "", true),
            ("J", "Join adjacent selected clips into one", false),
        ],
        2 => vec![
            // Piano Roll
            ("── Navigation ──", "", true),
            ("Middle-drag", "Pan view in any direction", false),
            ("Scroll wheel", "Scroll pitch up / down", false),
            ("Shift+Scroll", "Scroll timeline left / right", false),
            ("Ctrl+Scroll", "Zoom timeline (anchored to cursor)", false),
            ("", "", false),
            ("── Note Editing ──", "", true),
            ("Left-click (draw mode)", "Place new note", false),
            ("Left-drag (draw mode)", "Draw note and set length", false),
            (
                "Right-click note",
                "Delete note; drag to erase multiple",
                false,
            ),
            ("Ctrl+Drag (select mode)", "Rubber-band select notes", false),
            ("Ctrl+A", "Select all notes", false),
            ("Ctrl+D", "Duplicate selected notes", false),
            ("Delete / Backspace", "Delete selected notes", false),
            ("", "", false),
            ("── Note Movement ──", "", true),
            ("Arrow Up / Down", "Transpose +/- 1 semitone", false),
            ("Shift+Up / Down", "Transpose +/- 1 octave", false),
            ("Arrow Left / Right", "Nudge by snap unit", false),
            ("", "", false),
            ("── Keyboard Piano ──", "", true),
            (
                "Left-click piano key strip",
                "Audition / preview a note",
                false,
            ),
            ("", "", false),
            ("── Computer Keyboard ──", "", true),
            ("A  W  S  E  D  F  T", "C  C#  D  D#  E  F  F#", false),
            ("G  Y  H  U  J", "G  G#  A  A#  B", false),
            ("K  O  L", "C  C#  D (next octave)", false),
            ("Z / X", "Octave down / up", false),
            ("", "", false),
            (
                "  * Piano keyboard mode",
                "active when KBD icon is lit",
                false,
            ),
            ("", "", false),
            ("── MIDI Export ──", "", true),
            (
                "MID button (toolbar)",
                "Export current clip as .mid file",
                false,
            ),
        ],
        3 => vec![
            // Audio Editor
            ("── Overview ──", "", true),
            (
                "  The audio editor opens",
                "when you select an audio clip",
                false,
            ),
            (
                "  in the arrangement view.",
                "It shows the waveform for editing.",
                false,
            ),
            ("", "", false),
            ("── Navigation ──", "", true),
            ("Middle-drag", "Pan waveform left / right", false),
            ("Ctrl+Scroll", "Zoom in / out (horizontal)", false),
            ("Shift+Scroll", "Scroll left / right", false),
            ("", "", false),
            ("── Selection ──", "", true),
            (
                "Left-click + drag",
                "Select a time range on the waveform",
                false,
            ),
            ("A", "Select entire waveform", false),
            ("Escape", "Clear selection", false),
            ("", "", false),
            ("── Toolbar ──", "", true),
            ("UNIQUE", "Make a unique copy of cloned clip audio", false),
            ("SEL  (Q)", "Selection tool (click+drag to select)", false),
            ("NORM (W)", "Normalize selected region to 0dB", false),
            ("TRIM (E)", "Trim file to selection", false),
            ("FIT  (R)", "Fit clip length to new audio duration", false),
            ("CUT  (T)", "Cut selected region from file", false),
            ("PASTE(Y)", "Paste clipboard at playhead position", false),
            ("EXP", "Export audio clip to WAV file", false),
            ("", "", false),
            ("── Drag to Arranger ──", "", true),
            (
                "Ctrl+drag selection",
                "Drag selected region to arranger as clip",
                false,
            ),
            ("", "", false),
            ("── Effects (Apply) ──", "", true),
            (
                "Effects dropdown (top right)",
                "Choose effect: Reverse, Fade In/Out, etc.",
                false,
            ),
            (
                "APPLY button / B",
                "Apply chosen effect to selection",
                false,
            ),
            ("", "", false),
            ("── Undo / Redo ──", "", true),
            ("Ctrl+Z", "Undo last audio edit (when focused)", false),
            ("Ctrl+Shift+Z", "Redo last audio edit (when focused)", false),
            ("", "", false),
            ("── Playback ──", "", true),
            ("Space", "Play / stop from playhead (when focused)", false),
            ("Click ruler", "Set playhead position", false),
            ("", "", false),
            ("── Loop Region ──", "", true),
            ("Drag loop ruler handles", "Set loop start / end", false),
            (
                "Loop region highlighted",
                "Playback loops within region",
                false,
            ),
            ("Gain slider (toolbar)", "Adjust clip gain", false),
        ],
        4 => vec![
            // Automation Editor
            ("── Overview ──", "", true),
            (
                "  The automation editor opens",
                "when you select an automation clip",
                false,
            ),
            (
                "  in the arrangement view.",
                "Draw curves to modulate parameters.",
                false,
            ),
            ("", "", false),
            ("── Navigation ──", "", true),
            ("Middle-drag", "Pan view in any direction", false),
            ("Scroll wheel", "Scroll up / down", false),
            ("Shift+Scroll", "Scroll timeline left / right", false),
            ("Ctrl+Scroll", "Zoom timeline (anchored to cursor)", false),
            ("", "", false),
            ("── Point Editing ──", "", true),
            ("Left-click (empty area)", "Add a new control point", false),
            ("Left-drag point", "Move existing control point", false),
            ("Right-click point", "Delete control point", false),
            ("", "", false),
            ("── Curve Types ──", "", true),
            ("Linear (default)", "Straight line between points", false),
            ("Stepped", "Value jumps at each point (no interp)", false),
            ("", "", false),
            ("── Snap ──", "", true),
            ("Snap dropdown (toolbar)", "Set grid snap resolution", false),
            ("Snap toggle (S)", "Enable / disable snap to grid", false),
            ("", "", false),
            ("── Automation Targets ──", "", true),
            (
                "Right-click rack knob",
                "Assign knob to automation lane",
                false,
            ),
            (
                "Automation track",
                "Routes values to assigned parameter",
                false,
            ),
        ],
        5 => vec![
            // Rack
            ("── Overview ──", "", true),
            (
                "  The Instrument Rack shows",
                "modules loaded on the selected track.",
                false,
            ),
            ("  Open it by double-clicking", "a track header.", false),
            ("", "", false),
            ("── Module Browser ──", "", true),
            (
                "Left panel browser",
                "Lists available instruments & effects",
                false,
            ),
            (
                "Drag module to rack",
                "Add module to the signal chain",
                false,
            ),
            (
                "Drag module to lane",
                "Create new track with that module",
                false,
            ),
            ("", "", false),
            ("── Rack Layout ──", "", true),
            (
                "Right-drag module header",
                "Reorder modules in the chain",
                false,
            ),
            (
                "Middle-click module header",
                "Open sidechain source dropdown",
                false,
            ),
            (
                "Delete / right-click header",
                "Remove module from rack",
                false,
            ),
            ("", "", false),
            ("── Knob Controls ──", "", true),
            ("Left-drag up / down", "Adjust parameter value", false),
            (
                "Middle-drag up / down",
                "Fine adjustment (5x slower)",
                false,
            ),
            ("Shift+Click knob", "Reset to default value", false),
            ("Right-click knob", "Assign to automation lane", false),
            ("Hover over knob", "Shows parameter name & value", false),
            ("", "", false),
            ("── Presets ──", "", true),
            ("Preset dropdown (module)", "Load / switch presets", false),
            ("", "", false),
            ("── Effects Info ──", "", true),
            (
                "EQ",
                "3-band parametric: lo shelf, mid bell, hi shelf",
                false,
            ),
            ("  Lo / Hi Gain", "Shelving filter gain (±12 dB)", false),
            ("  Mid Gain", "Peaking bell filter gain (±12 dB)", false),
            (
                "  Mid Freq",
                "Mid band center frequency (100–10 kHz)",
                false,
            ),
            ("Delay", "Stereo delay with beat-synced L/R times", false),
            (
                "  Time L / Time R dropdowns",
                "Beat divisions incl. triplets",
                false,
            ),
            ("Compressor", "Real-time curve dot + GR / IN meters", false),
            (
                "Limiter",
                "Lookahead brickwall with per-sample GR ramp",
                false,
            ),
            (
                "  Ceiling",
                "Maximum output peak level (-12 to 0 dB)",
                false,
            ),
            ("  Release", "Gain recovery speed after limiting", false),
        ],
        6 => vec![
            // Mixer
            ("── Mixer Overview ──", "", true),
            ("  Press 2 to switch", "to the mixer view", false),
            ("", "", false),
            ("── Channel Strip ──", "", true),
            ("Volume fader", "Drag to set channel volume (dB)", false),
            ("Pan knob", "Drag to set stereo panning", false),
            (
                "Mute / Solo buttons",
                "Toggle mute or solo per track",
                false,
            ),
            ("", "", false),
            ("── Slim Track Mode ──", "", true),
            (
                "Slim / Expand button",
                "Toggle at bottom of each strip",
                false,
            ),
            (
                "  Slim mode shows",
                "Volume, pan, meter, mute/solo only",
                false,
            ),
            (
                "  Expand mode shows",
                "Full CStrip2 EQ, compressor, rack",
                false,
            ),
            ("", "", false),
            ("── VU Meters ──", "", true),
            (
                "Green/yellow/red bar",
                "Current RMS level with fast attack",
                false,
            ),
            (
                "Red peak needle",
                "Slow-decay peak indicator for easy reading",
                false,
            ),
            (
                "dB labels",
                "0, -10, -20, -30, -40, -50, +10 dB marks",
                false,
            ),
            ("", "", false),
            ("── CStrip2 (per-track) ──", "", true),
            (
                "CS / BYP button",
                "Toggle channel strip bypass for A/B comparison",
                false,
            ),
            ("Treble", "High-frequency EQ gain (0.5 = unity)", false),
            ("Mid", "Mid-frequency EQ gain (0.5 = unity)", false),
            ("Bass", "Low-frequency EQ gain (0.5 = unity)", false),
            ("TrebFreq", "Treble band crossover frequency", false),
            ("BassFreq", "Bass band crossover frequency", false),
            ("LoCap", "Hi-pass filter (0.0 = off, 1.0 = full cut)", false),
            ("HiCap", "Lo-pass filter (0.0 = off, 1.0 = full cut)", false),
            ("Compress", "Compressor amount (0.0 = off)", false),
            ("CompSpd", "Compressor speed / attack", false),
            (
                "Output",
                "Output gain + soft saturation (0.33 = unity)",
                false,
            ),
            ("  Shift+Click knob", "Reset to default (neutral)", false),
            ("", "", false),
            ("── Effect Rack ──", "", true),
            (
                "Drag from browser",
                "Add effect / instrument to rack",
                false,
            ),
            (
                "Right-drag module header",
                "Reorder modules in the rack",
                false,
            ),
            (
                "Middle-click effect slot",
                "Open sidechain source dropdown",
                false,
            ),
            ("Right-click knob", "Assign knob to automation lane", false),
            ("", "", false),
            ("── Audio Engine ──", "", true),
            (
                "Options > Reset Audio",
                "Kill all voices if audio freezes",
                false,
            ),
            ("", "", false),
            ("── Automation ──", "", true),
            ("Click automation lane", "Add control point", false),
            ("Drag point", "Move control point", false),
            ("Right-click point", "Delete control point", false),
            ("Snap dropdown (toolbar)", "Set grid snap resolution", false),
        ],
        _ => vec![],
    };

    // Render entries
    let key_w = (content_w as f32 * 0.45) as i32;
    let desc_x = content_x + key_w + 4;
    let desc_w = content_w - key_w - 4;
    let mut y = content_top;
    for (key, desc, is_section) in &entries {
        if y + line_h > content_bot {
            break;
        }
        if key.is_empty() && desc.is_empty() {
            y += 5;
            continue;
        }
        if *is_section {
            draw_pixel_label(
                canvas,
                &state.theme,
                key,
                content_x,
                y,
                content_w,
                c_section,
            );
        } else {
            let col = if key.starts_with("  *") || key.starts_with("  ") {
                c_note
            } else {
                c_key
            };
            draw_pixel_label(canvas, &state.theme, key, content_x, y, key_w, col);
            draw_pixel_label(canvas, &state.theme, desc, desc_x, y, desc_w, c_desc);
        }
        y += line_h;
    }

    // Click outside sidebar to dismiss (but not on sidebar tabs)
    if input.mouse_pressed
        && !input.mouse_in_rect(px, tab_top, tab_w, tab_labels.len() as i32 * tab_h)
    {
        state.help_screen_visible = false;
    }
}
