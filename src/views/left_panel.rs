// Eden DAW — Views: left_panel

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::clip_manager::draw_clip_manager;
use crate::app::input::{InputState, WidgetId};
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

/// Draw the left-side sample browser panel.
/// Width = state.sample_browser_width; spans from transport bar bottom to bottom panel.
/// Tabbed left panel: Files, Clips, Instruments
pub(super) fn draw_left_panel(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    state: &mut AppState,
) {
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
        input.scroll_consumed = true;
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
                if input.click_type == Some(crate::app::input::ClickType::Double) {
                    let file_path = row.path.to_string_lossy().to_string();
                    let file_name = row.name.clone();
                    // Create an AudioClip for the clip library
                    let new_clip = crate::app::models::Clip::Audio(crate::app::models::AudioClip {
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
                        if let crate::app::models::Clip::Audio(ac) = lc {
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
                            crate::engine::load_audio(std::path::Path::new(&file_str))
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
                            match crate::app::models::import_midi_file(&file_str, bpm) {
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
                                        let mut new_track = crate::app::models::Track::new(
                                            new_id,
                                            &track_name,
                                            crate::app::models::TrackType::Midi,
                                        );
                                        // Give the track an Analog instrument
                                        new_track.rack =
                                            vec![crate::app::models::create_rack_slot_for_module(
                                                "Analog", 1,
                                            )];
                                        midi_clip.color = new_track.color;
                                        new_track
                                            .clips
                                            .push(crate::app::models::Clip::Midi(midi_clip));
                                        state.commands.execute(
                                            Box::new(crate::app::commands::AddTrack {
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
                                    if track.track_type == crate::app::models::TrackType::Audio {
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
                                    if let crate::app::models::Clip::Audio(ref ac) = track.clips[ci]
                                    {
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
                                            if let crate::app::models::Clip::Audio(ref mut ac_mut) =
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
                                    crate::engine::load_audio(std::path::Path::new(&file_str))
                                {
                                    let duration_secs = samples.len() as f64 / sr as f64;
                                    (duration_secs * beats_per_sec).max(0.01)
                                } else {
                                    4.0
                                };
                                let mut audio_clip = crate::app::models::Clip::Audio(
                                    crate::app::models::AudioClip {
                                        source_file: file_str,
                                        start_time: beat,
                                        offset: 0.0,
                                        length: clip_len_beats,
                                        gain: 1.0,
                                        name: stem.clone(),
                                        color: [100, 160, 255, 255],
                                        fade_in: 0.0,
                                        fade_out: 0.0,
                                    },
                                );

                                if let Some(row) = target_row {
                                    // Drop onto existing audio track (empty area)
                                    let track_id = state.project.tracks[row].id;
                                    let track_color = state.project.tracks[row].color;
                                    // Use track color for the new clip
                                    if let crate::app::models::Clip::Audio(ref mut ac) = audio_clip
                                    {
                                        ac.color = track_color;
                                    }
                                    let new_ci = state.project.tracks[row].clips.len();
                                    state.commands.execute(
                                        Box::new(crate::app::commands::AddClips {
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
                                    let mut new_track = crate::app::models::Track::new(
                                        new_id,
                                        &stem,
                                        crate::app::models::TrackType::Audio,
                                    );
                                    let mut clip_with_color = audio_clip;
                                    if let crate::app::models::Clip::Audio(ac) =
                                        &mut clip_with_color
                                    {
                                        ac.color = new_track.color
                                    }
                                    new_track.clips.push(clip_with_color);
                                    state.commands.execute(
                                        Box::new(crate::app::commands::AddTrack {
                                            track: new_track,
                                        }),
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
                            let mut new_track = crate::app::models::Track::new(
                                new_id,
                                &stem,
                                crate::app::models::TrackType::Midi,
                            );
                            // Replace the default rack with a Sampler
                            new_track.rack = vec![crate::app::models::RackSlot::sampler(1)];
                            new_track.sampler_file = Some(file_str);
                            state.commands.execute(
                                Box::new(crate::app::commands::AddTrack { track: new_track }),
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
    node: &crate::app::state::SampleTreeNode,
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
fn toggle_tree_node(tree: &mut [crate::app::state::SampleTreeNode], addr: &[usize]) {
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
        input.scroll_consumed = true;
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
            items: crate::modules::MIDI_EFFECT_NAMES
                .iter()
                .map(|&name| ModuleEntry { icon: "♪", name })
                .collect(),
        },
        ModuleCategory {
            icon: "~",
            label: "GENERATORS",
            color: [100, 220, 130, 220],
            items: crate::modules::INSTRUMENT_NAMES
                .iter()
                .map(|&name| ModuleEntry { icon: "~", name })
                .collect(),
        },
        ModuleCategory {
            icon: "≈",
            label: "FX",
            color: [220, 160, 80, 220],
            items: crate::modules::EFFECT_NAMES
                .iter()
                .map(|&name| ModuleEntry { icon: "≈", name })
                .collect(),
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

    // Scroll with mouse wheel (check mouse_in_rect so arranger scroll doesn't leak in)
    if input.mouse_in_rect(0, top, w, h) && input.scroll_y != 0 && !input.scroll_consumed {
        state.instruments_scroll = (state.instruments_scroll - input.scroll_y * 20)
            .max(0)
            .min(max_scroll);
        input.scroll_consumed = true;
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
        input.scroll_consumed = true;
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
