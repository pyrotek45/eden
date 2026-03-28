// Eden DAW — Views: arrangement

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::bottom_panel::draw_bottom_panel;
use super::left_panel::draw_left_panel;
use super::overlays::draw_overlays;
use super::track_headers::draw_track_headers;
use super::track_lanes::draw_track_lanes;
use super::transport::{draw_loop_ruler, draw_mode_tabs, draw_timeline_ruler, draw_transport};
use crate::app::input::InputState;
use crate::app::state::*;
use crate::widgets::*;

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
            if layer > crate::app::state::UiLayer::Base {
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
            if layer > crate::app::state::UiLayer::Base || mouse_below_panel {
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
    if state.dropdown_open_id == 200 && layer == crate::app::state::UiLayer::Base {
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
    if layer == crate::app::state::UiLayer::Base {
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
                                crate::app::models::Clip::Midi(m) => m.length,
                                crate::app::models::Clip::Audio(a) => a.length,
                                crate::app::models::Clip::Automation(a) => a.length,
                            })
                            .unwrap_or(4.0);
                        // Check type compatibility
                        let clip_type_ok = clip_info
                            .map(|c| {
                                let ct = match c {
                                    crate::app::models::Clip::Midi(_) => {
                                        crate::app::models::TrackType::Midi
                                    }
                                    crate::app::models::Clip::Audio(_) => {
                                        crate::app::models::TrackType::Audio
                                    }
                                    crate::app::models::Clip::Automation(_) => {
                                        crate::app::models::TrackType::Automation
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
                                crate::app::models::Clip::Midi(_) => {
                                    crate::app::models::TrackType::Midi
                                }
                                crate::app::models::Clip::Audio(_) => {
                                    crate::app::models::TrackType::Audio
                                }
                                crate::app::models::Clip::Automation(_) => {
                                    crate::app::models::TrackType::Automation
                                }
                            };
                            if clip_track_type == state.project.tracks[row].track_type {
                                let mut new_clip = src_clip;
                                match &mut new_clip {
                                    crate::app::models::Clip::Midi(m) => m.start_time = snapped,
                                    crate::app::models::Clip::Audio(a) => a.start_time = snapped,
                                    crate::app::models::Clip::Automation(a) => {
                                        a.start_time = snapped
                                    }
                                }
                                let track_id = state.project.tracks[row].id;
                                state.commands.execute(
                                    Box::new(crate::app::commands::AddClips {
                                        clips: vec![(track_id, new_clip)],
                                        added_indices: vec![],
                                    }),
                                    &mut state.project,
                                );
                            } else {
                                state.push_status(format!(
                                    "Cannot drop {} clip on {} track",
                                    match clip_track_type {
                                        crate::app::models::TrackType::Midi => "MIDI",
                                        crate::app::models::TrackType::Audio => "Audio",
                                        crate::app::models::TrackType::Automation => "Auto",
                                    },
                                    match state.project.tracks[row].track_type {
                                        crate::app::models::TrackType::Midi => "MIDI",
                                        crate::app::models::TrackType::Audio => "Audio",
                                        crate::app::models::TrackType::Automation => "Auto",
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
                    crate::app::models::Clip::Midi(m) => m.length,
                    crate::app::models::Clip::Audio(a) => a.length,
                    crate::app::models::Clip::Automation(a) => a.length,
                };
                let ct = match c {
                    crate::app::models::Clip::Midi(_) => crate::app::models::TrackType::Midi,
                    crate::app::models::Clip::Audio(_) => crate::app::models::TrackType::Audio,
                    crate::app::models::Clip::Automation(_) => {
                        crate::app::models::TrackType::Automation
                    }
                };
                (len, ct)
            } else {
                (4.0, crate::app::models::TrackType::Midi)
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
                            crate::app::models::Clip::Midi(m) => m.start_time = snapped,
                            crate::app::models::Clip::Audio(a) => a.start_time = snapped,
                            crate::app::models::Clip::Automation(a) => a.start_time = snapped,
                        }

                        if let Some(row) = target_row {
                            if state.project.tracks[row].track_type == clip_type {
                                let tid = state.project.tracks[row].id;
                                state.commands.execute(
                                    Box::new(crate::app::commands::AddClips {
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
                                        crate::app::models::TrackType::Midi => "MIDI",
                                        crate::app::models::TrackType::Audio => "Audio",
                                        crate::app::models::TrackType::Automation => "Auto",
                                    },
                                    match state.project.tracks[row].track_type {
                                        crate::app::models::TrackType::Midi => "MIDI",
                                        crate::app::models::TrackType::Audio => "Audio",
                                        crate::app::models::TrackType::Automation => "Auto",
                                    },
                                ));
                            }
                        } else if below_all {
                            // Create a new track matching the clip type
                            let new_id =
                                state.project.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                            let track_name = match clip_type {
                                crate::app::models::TrackType::Midi => format!("MIDI {}", new_id),
                                crate::app::models::TrackType::Audio => format!("Audio {}", new_id),
                                crate::app::models::TrackType::Automation => {
                                    format!("Auto {}", new_id)
                                }
                            };
                            let new_track =
                                crate::app::models::Track::new(new_id, &track_name, clip_type);
                            state.commands.execute(
                                Box::new(crate::app::commands::AddTrack { track: new_track }),
                                &mut state.project,
                            );
                            state.commands.execute(
                                Box::new(crate::app::commands::AddClips {
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
            let mut track_rows: Vec<(i32, i32, u32, crate::app::models::TrackType)> = Vec::new();
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
                        let is_audio_track =
                            track_rows[row].3 == crate::app::models::TrackType::Audio;
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

                        if track_type == crate::app::models::TrackType::Audio {
                            // Drop as an audio clip on this audio track
                            let clip =
                                crate::app::models::Clip::Audio(crate::app::models::AudioClip {
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
                                Box::new(crate::app::commands::AddClips {
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
        use crate::app::state::FocusedPanel;
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
        state.hover_last_widget = crate::app::input::WidgetId::None;
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
                    crate::app::models::TrackType::Midi => true,
                    crate::app::models::TrackType::Audio => {
                        !crate::modules::is_midi_effect(module_name)
                            && !crate::modules::is_instrument(module_name)
                    }
                    crate::app::models::TrackType::Automation => false,
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
