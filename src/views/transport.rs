// Eden DAW — Views: transport

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::state::*;
use crate::theme::Theme;
use crate::widgets::*;

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
