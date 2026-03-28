// Eden DAW — Views: audio_editor

use sdl2::render::Canvas;
use sdl2::video::Window;

use super::gain_to_db_label;
use crate::input::{InputState, WidgetId};
use crate::state::*;
use crate::theme::Theme;
use crate::widgets::*;

/// Find the nearest zero crossing to `idx` in `samples`, searching up to `max_search`
/// samples in each direction. A zero crossing is where the sample value crosses zero
/// (sign change) or is exactly zero. Returns the adjusted index.
pub(super) fn nearest_zero_crossing(samples: &[f32], idx: usize, max_search: usize) -> usize {
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

pub(super) fn draw_audio_editor(
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
                                crate::engine::load_audio_interleaved(path)
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
                                                    crate::engine::save_wav_stereo(
                                                        path, &modified, sr,
                                                    )
                                                } else {
                                                    crate::engine::save_wav_mono(
                                                        path, &modified, sr,
                                                    )
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
                                crate::engine::load_audio_interleaved(path)
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
                                                crate::engine::save_wav_stereo(path, &trimmed, sr)
                                            } else {
                                                crate::engine::save_wav_mono(path, &trimmed, sr)
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
                                crate::engine::load_audio_interleaved(path)
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
                                                crate::engine::save_wav_stereo(path, &remaining, sr)
                                            } else {
                                                crate::engine::save_wav_mono(path, &remaining, sr)
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
                        if let Ok((raw, channels, sr)) = crate::engine::load_audio_interleaved(path)
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
                                        crate::engine::save_wav_stereo(path, &result, sr)
                                    } else {
                                        crate::engine::save_wav_mono(path, &result, sr)
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
                if let Ok((raw, channels, sr)) = crate::engine::load_audio_interleaved(path) {
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
                                    crate::engine::save_wav_stereo(path, &modified, sr)
                                } else {
                                    crate::engine::save_wav_mono(path, &modified, sr)
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
