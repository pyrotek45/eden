// Eden DAW — Main entry point
// SDL2 window, event loop, audio engine, everything wired together.
#![allow(dead_code)]

mod audio;
mod commands;
mod config;
mod input;
mod models;
mod modules;
mod render;
mod state;
#[cfg(test)]
mod tests;
mod theme;
mod views;
mod widgets;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use crate::audio::start_audio_engine;
use crate::input::InputState;
use crate::state::*;

/// Map QWERTY keyboard keys to semitone offsets (relative to current octave).
/// Bottom row (ZSXDCVGBHNJM) = lower octave, top row (Q2W3ER5T6Y7U) = upper octave.
fn qwerty_to_semitone(key: Keycode) -> Option<i32> {
    match key {
        // Lower row: C3..B3 (relative semitones 0-12)
        Keycode::A => Some(0),  // C
        Keycode::W => Some(1),  // C#
        Keycode::S => Some(2),  // D
        Keycode::E => Some(3),  // D#
        Keycode::D => Some(4),  // E
        Keycode::F => Some(5),  // F
        Keycode::T => Some(6),  // F#
        Keycode::G => Some(7),  // G
        Keycode::Y => Some(8),  // G#
        Keycode::H => Some(9),  // A
        Keycode::U => Some(10), // A#
        Keycode::J => Some(11), // B
        Keycode::K => Some(12), // C (next octave)
        Keycode::O => Some(13), // C#
        Keycode::L => Some(14), // D
        _ => None,
    }
}

fn main() {
    let sdl = sdl2::init().expect("Failed to init SDL2");
    let video = sdl.video().expect("Failed to init SDL2 video");

    let window = video
        .window("Eden DAW", 1280, 800)
        .position_centered()
        .resizable()
        .build()
        .expect("Failed to create window");

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .expect("Failed to create canvas");

    // ── Set window icon (32×32 music note) ──
    {
        // Build a 32×32 RGBA pixel buffer — dark bg + white music note
        let size = 32usize;
        let mut pixels = vec![0u8; size * size * 4];
        // Helper: set pixel (x, y) to RGBA
        let mut set = |x: usize, y: usize, r: u8, g: u8, b: u8, a: u8| {
            if x < size && y < size {
                let off = (y * size + x) * 4;
                pixels[off] = r;
                pixels[off + 1] = g;
                pixels[off + 2] = b;
                pixels[off + 3] = a;
            }
        };
        // Background
        for y in 0..size {
            for x in 0..size {
                set(x, y, 18, 18, 24, 255);
            }
        }
        // Music note (8th note) shape — hand-crafted 32×32 pixel art
        // Stem (vertical bar, right side)
        for y in 4..22usize {
            set(20, y, 220, 225, 240, 255);
        }
        // Flag (top-right of stem)
        for (dx, dy) in &[(1, 4), (2, 5), (3, 6), (2, 7), (1, 8)] {
            set(20 + dx, 4 + dy, 220, 225, 240, 255);
        }
        // Note head (filled ellipse centered at ~(16, 22))
        let (hx, hy): (i32, i32) = (16, 22);
        for dy in -4i32..=3i32 {
            for dx in -5i32..=4i32 {
                // Ellipse check: (dx/5)^2 + (dy/3.5)^2 <= 1
                let ex = dx as f32 / 5.0;
                let ey = dy as f32 / 3.5;
                if ex * ex + ey * ey <= 1.0 {
                    let nx = (hx + dx) as usize;
                    let ny = (hy + dy) as usize;
                    set(nx, ny, 220, 225, 240, 255);
                }
            }
        }
        // Create SDL surface and set as icon
        let surface = sdl2::surface::Surface::from_data(
            &mut pixels,
            size as u32,
            size as u32,
            (size * 4) as u32,
            sdl2::pixels::PixelFormatEnum::RGBA32,
        );
        if let Ok(surf) = surface {
            canvas.window_mut().set_icon(&surf);
        }
    }

    let mut event_pump = sdl.event_pump().expect("Failed to get event pump");
    let timer = sdl.timer().expect("Failed to get timer");

    // Enable SDL2 text input so we receive TextInput events for text fields
    video.text_input().start();

    let mut state = AppState::new();
    let mut input = InputState::default();

    // Load user config
    let user_config = config::UserConfig::load();
    state.set_theme_by_name(&user_config.theme_name);
    state.auto_return = user_config.auto_return;
    state.ui_scale = user_config.ui_scale;
    state.ui_scale_pending = user_config.ui_scale;
    state.snap.enabled = user_config.snap_enabled;
    state.snap.resolution_idx = user_config
        .snap_resolution_idx
        .min(crate::state::SNAP_RESOLUTIONS.len() - 1);
    state.sample_browser_open = user_config.sample_browser_open;
    state.sample_browser_width = user_config.sample_browser_width;
    state.bottom_panel_open = user_config.bottom_panel_open;
    state.bottom_panel_height = user_config.bottom_panel_height;
    state.velocity_editor_visible = user_config.velocity_editor_visible;
    state.sample_auto_play = user_config.sample_auto_play;
    state.audio_device_idx = user_config.audio_device_idx;
    state.left_panel_tab = match user_config.left_panel_tab {
        1 => state::LeftPanelTab::Clips,
        2 => state::LeftPanelTab::Instruments,
        3 => state::LeftPanelTab::Themes,
        _ => state::LeftPanelTab::Files,
    };
    // Load favorite folders into sample browser
    for folder in &user_config.favorite_folders {
        let path = std::path::PathBuf::from(folder);
        if path.is_dir() {
            state.add_sample_folder(path);
        }
    }
    // Store favorite folder paths in state for UI display
    state.favorite_folders = user_config.favorite_folders.clone();
    // Load recent projects from config
    state.recent_projects = user_config.recent_projects.clone();
    state.follow_playhead = user_config.follow_playhead;
    state.autosave_enabled = user_config.autosave_enabled;
    state.autosave_interval_idx = user_config
        .autosave_interval_idx
        .min(crate::config::AUTOSAVE_INTERVALS.len() - 1);
    // Initialize autosave countdown based on config interval
    if state.autosave_enabled {
        let (_, secs) = crate::config::AUTOSAVE_INTERVALS[state.autosave_interval_idx];
        state.autosave_countdown = secs * 60; // frames (assuming ~60fps)
    }

    let audio_shared = match start_audio_engine() {
        Ok((shared, _pos_atomic)) => {
            println!("[audio] Engine started");
            Some(shared)
        }
        Err(e) => {
            eprintln!("[audio] Failed to start: {}", e);
            None
        }
    };

    // Default session file — named after the project if one exists, else fallback
    let default_save_fallback = "eden_session.eden.json";
    // Try to load any previously saved session (check fallback name first, then project-named)
    let default_save = default_save_fallback.to_string();
    if std::path::Path::new(&default_save).exists() {
        match state.load_project(&default_save) {
            Ok(()) => {
                println!("[session] Loaded {}", default_save);
                // Keep mode as ProjectManager so the startup screen shows
                state.mode = crate::state::AppMode::ProjectManager;
            }
            Err(e) => eprintln!("[session] Load error: {}", e),
        }
    }

    // Populate clip library from initial project clips
    state.sync_clip_library();

    // Cache of loaded audio clip samples: path → (Arc<mono_samples>, sample_rate)
    let mut audio_sample_cache: std::collections::HashMap<String, (std::sync::Arc<Vec<f32>>, u32)> =
        std::collections::HashMap::new();

    let mut last_tick = timer.ticks();

    while state.running {
        let now = timer.ticks();
        let dt = (now - last_tick) as f64 / 1000.0;
        last_tick = now;

        input.begin_frame();

        // Snapshot config-relevant state at frame start for change detection
        #[derive(PartialEq)]
        struct CfgSnap {
            theme: String,
            favs: Vec<String>,
            auto_return: bool,
            ui_scale: u32, // f32 as bits for PartialEq
            snap_enabled: bool,
            snap_res: usize,
            browser_open: bool,
            browser_w: i32,
            bottom_open: bool,
            bottom_h: i32,
            vel_vis: bool,
            win_w: u32,
            win_h: u32,
            left_tab: u8,
            sample_auto: bool,
            audio_dev: usize,
            follow: bool,
        }
        let cfg_snapshot = CfgSnap {
            theme: state.theme.name.clone(),
            favs: state.favorite_folders.clone(),
            auto_return: state.auto_return,
            ui_scale: state.ui_scale.to_bits(),
            snap_enabled: state.snap.enabled,
            snap_res: state.snap.resolution_idx,
            browser_open: state.sample_browser_open,
            browser_w: state.sample_browser_width,
            bottom_open: state.bottom_panel_open,
            bottom_h: state.bottom_panel_height,
            vel_vis: state.velocity_editor_visible,
            win_w: state.window_width,
            win_h: state.window_height,
            left_tab: match state.left_panel_tab {
                state::LeftPanelTab::Files => 0,
                state::LeftPanelTab::Clips => 1,
                state::LeftPanelTab::Instruments => 2,
                state::LeftPanelTab::Themes => 3,
            },
            sample_auto: state.sample_auto_play,
            audio_dev: state.audio_device_idx,
            follow: state.follow_playhead,
        };

        // Always poll current modifier state so Ctrl/Shift/Alt are
        // accurate even when no key event fires this frame.
        input.key_mod = sdl.keyboard().mod_state();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    state.running = false;
                }
                Event::Window {
                    win_event: sdl2::event::WindowEvent::Resized(_w, _h),
                    ..
                } => {
                    // Dimensions will be recalculated from output_size()/scale in the render loop.
                    // Don't set them here to avoid scale mismatch on HiDPI.
                }
                Event::Window { .. } => {}
                Event::MouseMotion { x, y, .. } => {
                    input.on_mouse_move(x, y);
                }
                Event::MouseButtonDown {
                    x, y, mouse_btn, ..
                } => {
                    input.on_mouse_down(x, y, mouse_btn, now as u64);
                }
                Event::MouseButtonUp {
                    x, y, mouse_btn, ..
                } => {
                    input.on_mouse_up(x, y, mouse_btn);
                }
                Event::MouseWheel { x, y, .. } => {
                    input.on_scroll(x, y);
                }
                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    ..
                } => {
                    input.key_mod = keymod;
                    input.on_key_down(key);

                    // When a text field is active, suppress ALL DAW keyboard shortcuts.
                    // The text_field widget reads keys_pressed directly and handles them.
                    if state.text_field_active_id != 0 {
                        // Ctrl+S saves even with a text field active
                        if matches!(key, Keycode::S) && input.ctrl() {
                            match state.quick_save() {
                                Ok(()) => println!("[save] Project saved"),
                                Err(e) => eprintln!("[save] Save error: {}", e),
                            }
                        }
                        // Only allow Escape to close popups even during text input
                        // (text_field itself handles Escape to cancel)
                    } else {
                        // When piano keyboard mode is active and this key maps to a note,
                        // skip non-modifier shortcuts so the key plays a note instead.
                        let piano_consumes = state.piano_keyboard_mode
                            && !input.ctrl()
                            && qwerty_to_semitone(key).is_some();

                        match key {
                            // Escape and F1 always available in any mode
                            Keycode::Escape => {
                                if state.help_screen_visible {
                                    state.help_screen_visible = false;
                                } else if state.mode == AppMode::ProjectManager {
                                    // Escape on project manager: go back to arrangement if a
                                    // project is already loaded
                                    if !state.project.tracks.is_empty()
                                        || state.last_save_path.is_some()
                                    {
                                        state.mode = AppMode::Arrangement;
                                    }
                                } else if state.focused_panel
                                    == crate::state::FocusedPanel::AudioEditor
                                    && state.audio_editor_selection.is_some()
                                {
                                    state.audio_editor_selection = None;
                                    state.push_status("Selection cleared");
                                } else {
                                    state.selected_clip = None;
                                    state.selected_clips.clear();
                                    // Cancel any in-progress clip drag
                                    state.clip_drag_ghost_positions.clear();
                                    state.drag_original_positions.clear();
                                    state.clip_drag_target_track = None;
                                    state.clip_drag_target_valid = false;
                                    state.clip_drag_is_copy = false;
                                    state.add_track_popup_open = false;
                                    state.project_popup_open = false;
                                }
                            }
                            Keycode::F1 => {
                                state.help_screen_visible = !state.help_screen_visible;
                            }
                            // All other shortcuts are suppressed on the Project Manager screen
                            _ if state.mode == AppMode::ProjectManager => {}
                            Keycode::Space => {
                                // If audio editor is focused, Space controls its own playback
                                if state.focused_panel == crate::state::FocusedPanel::AudioEditor {
                                    if state.audio_editor_playing {
                                        // Stop audio editor playback
                                        state.audio_editor_playing = false;
                                        state.sample_preview_path = None;
                                        state.sample_preview_trigger = false;
                                        state.sample_preview_start_sample = 0;
                                        state.sample_preview_end_sample = 0;
                                    } else {
                                        // Start audio editor playback from playhead or selection
                                        let source_file: Option<String> =
                                            state.selected_clip.and_then(|(tid, cidx)| {
                                                state
                                                    .project
                                                    .tracks
                                                    .iter()
                                                    .find(|t| t.id == tid)
                                                    .and_then(|t| t.clips.get(cidx))
                                                    .and_then(|c| {
                                                        if let crate::models::Clip::Audio(ac) = c {
                                                            Some(ac.source_file.clone())
                                                        } else {
                                                            None
                                                        }
                                                    })
                                            });
                                        if let Some(sf) = source_file {
                                            if !sf.is_empty() {
                                                let preview_sr = 44100usize;
                                                if let Some((sel_s, sel_e)) =
                                                    state.audio_editor_selection
                                                {
                                                    let s = sel_s.min(sel_e).max(0.0);
                                                    let e = sel_s.max(sel_e);
                                                    state.audio_editor_playhead = s;
                                                    state.sample_preview_start_sample =
                                                        (s * preview_sr as f64) as usize;
                                                    state.sample_preview_end_sample =
                                                        (e * preview_sr as f64) as usize;
                                                } else {
                                                    let start = state.audio_editor_playhead;
                                                    state.sample_preview_start_sample =
                                                        (start * preview_sr as f64) as usize;
                                                    if state.audio_editor_loop_enabled
                                                        && state.audio_editor_loop_end
                                                            > state.audio_editor_loop_start
                                                    {
                                                        state.sample_preview_end_sample = (state
                                                            .audio_editor_loop_end
                                                            * preview_sr as f64)
                                                            as usize;
                                                    } else {
                                                        state.sample_preview_end_sample = 0;
                                                    }
                                                }
                                                state.audio_editor_playing = true;
                                                state.sample_preview_path =
                                                    Some(std::path::PathBuf::from(&sf));
                                                state.sample_preview_trigger = true;
                                            }
                                        }
                                    }
                                } else if state.sample_preview_path.is_some()
                                    || state.sample_preview_trigger
                                {
                                    // If a sample preview is playing, stop it first
                                    state.sample_preview_path = None;
                                    state.sample_preview_trigger = false;
                                    state.audio_editor_playing = false;
                                } else {
                                    // Normal play/stop transport toggle
                                    if !state.project.transport.playing {
                                        // Starting playback: save current position for auto-return
                                        state.pre_play_position = state.project.transport.position;
                                        // Push current UI position to audio thread before starting
                                        state.seek_pending = true;
                                    } else {
                                        // Stopping playback — let effect tails (reverb, delay)
                                        // ring out naturally. Don't seek/reset effects.
                                        if state.auto_return {
                                            state.project.transport.position =
                                                state.pre_play_position;
                                        }
                                    }
                                    state.project.transport.playing =
                                        !state.project.transport.playing;
                                }
                            }
                            Keycode::Return => {
                                // Back/rewind: stop and go to beat 0 (or loop start if looping)
                                state.project.transport.playing = false;
                                if state.project.transport.loop_enabled {
                                    state.project.transport.position =
                                        state.project.transport.loop_region.start;
                                } else {
                                    state.project.transport.position = 0.0;
                                }
                                state.pre_play_position = state.project.transport.position;
                                state.seek_pending = true;
                            }
                            Keycode::L if !piano_consumes => {
                                state.project.transport.loop_enabled =
                                    !state.project.transport.loop_enabled;
                            }
                            Keycode::Num1 => state.mode = AppMode::Arrangement,
                            Keycode::Num2 => state.mode = AppMode::Mixer,
                            Keycode::Num3 => state.mode = AppMode::Edit,
                            Keycode::S if input.ctrl() => {
                                let save_path = state.last_save_path.clone().unwrap_or_else(|| {
                                    let name = state.project.name.trim().to_string();
                                    let safe = if name.is_empty() {
                                        "untitled".to_string()
                                    } else {
                                        name
                                    };
                                    format!("{}.eden.json", safe)
                                });
                                match state.save_project(&save_path) {
                                    Ok(()) => {
                                        state.push_status(format!("Saved to {}", save_path));
                                        println!("[session] Saved to {}", save_path);
                                    }
                                    Err(e) => {
                                        state.push_status(format!("Save error: {}", e));
                                        eprintln!("[session] Save error: {}", e);
                                    }
                                }
                            }
                            Keycode::Z
                                if input.ctrl()
                                    && input.shift()
                                    && state.focused_panel
                                        == crate::state::FocusedPanel::AudioEditor =>
                            {
                                // Audio editor redo
                                if let Some((src, backup, desc, proj_snapshot)) =
                                    state.audio_redo_stack.pop()
                                {
                                    // Current state becomes undo backup
                                    let sp = std::path::Path::new(&src);
                                    let dir = sp.parent().unwrap_or(std::path::Path::new("."));
                                    let stem =
                                        sp.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
                                    let ext =
                                        sp.extension().and_then(|s| s.to_str()).unwrap_or("wav");
                                    let ts = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis())
                                        .unwrap_or(0);
                                    let undo_backup =
                                        dir.join(format!(".{}_undo_{}.{}", stem, ts, ext));
                                    // Save current project state before redo overwrites it
                                    let current_proj = if proj_snapshot.is_some() {
                                        Some(state.project.clone())
                                    } else {
                                        None
                                    };
                                    if std::fs::copy(&src, &undo_backup).is_ok() {
                                        state.audio_undo_stack.push((
                                            src.clone(),
                                            undo_backup.to_string_lossy().to_string(),
                                            desc.clone(),
                                            current_proj,
                                        ));
                                    }
                                    if std::fs::copy(&backup, &src).is_ok() {
                                        let _ = std::fs::remove_file(&backup);
                                        state.waveform_cache.remove(&src);
                                        state.waveform_stereo_cache.remove(&src);
                                        state.waveform_raw_cache.remove(&src);
                                        state.audio_sample_invalidate.push(src.clone());
                                        // Restore project snapshot (clip metadata) if present
                                        if let Some(proj) = proj_snapshot {
                                            state.project = proj;
                                        }
                                        state.push_status(format!("Redo: {}", desc));
                                    }
                                } else {
                                    state.push_status("Nothing to redo");
                                }
                            }
                            Keycode::Z
                                if input.ctrl()
                                    && state.focused_panel
                                        == crate::state::FocusedPanel::AudioEditor =>
                            {
                                // Audio editor undo
                                if let Some((src, backup, desc, proj_snapshot)) =
                                    state.audio_undo_stack.pop()
                                {
                                    // Current state becomes redo backup
                                    let sp = std::path::Path::new(&src);
                                    let dir = sp.parent().unwrap_or(std::path::Path::new("."));
                                    let stem =
                                        sp.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
                                    let ext =
                                        sp.extension().and_then(|s| s.to_str()).unwrap_or("wav");
                                    let ts = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis())
                                        .unwrap_or(0);
                                    let redo_backup =
                                        dir.join(format!(".{}_redo_{}.{}", stem, ts, ext));
                                    // Save current project state before undo overwrites it
                                    let current_proj = if proj_snapshot.is_some() {
                                        Some(state.project.clone())
                                    } else {
                                        None
                                    };
                                    if std::fs::copy(&src, &redo_backup).is_ok() {
                                        state.audio_redo_stack.push((
                                            src.clone(),
                                            redo_backup.to_string_lossy().to_string(),
                                            desc.clone(),
                                            current_proj,
                                        ));
                                    }
                                    if std::fs::copy(&backup, &src).is_ok() {
                                        let _ = std::fs::remove_file(&backup);
                                        state.waveform_cache.remove(&src);
                                        state.waveform_stereo_cache.remove(&src);
                                        state.waveform_raw_cache.remove(&src);
                                        state.audio_sample_invalidate.push(src.clone());
                                        // Restore project snapshot (clip metadata) if present
                                        if let Some(proj) = proj_snapshot {
                                            state.project = proj;
                                        }
                                        state.push_status(format!("Undo: {}", desc));
                                    }
                                } else {
                                    state.push_status("Nothing to undo");
                                }
                            }
                            Keycode::Z if input.ctrl() && input.shift() => {
                                if let Some(desc) = state.commands.redo_description() {
                                    state.push_status(format!("Redo: {}", desc));
                                }
                                state.commands.redo(&mut state.project);
                            }
                            Keycode::Z if input.ctrl() => {
                                if let Some(desc) = state.commands.undo_description() {
                                    state.push_status(format!("Undo: {}", desc));
                                }
                                state.commands.undo(&mut state.project);
                            }
                            Keycode::R if input.ctrl() => {
                                if let Some(desc) = state.commands.redo_description() {
                                    state.push_status(format!("Redo: {}", desc));
                                }
                                state.commands.redo(&mut state.project);
                            }
                            // ── Audio editor tool shortcuts ──────────────────
                            Keycode::A
                                if !input.ctrl()
                                    && state.focused_panel
                                        == crate::state::FocusedPanel::AudioEditor =>
                            {
                                // Select All — select the entire waveform
                                let total = state.selected_clip.and_then(|(tid, cidx)| {
                                    state
                                        .project
                                        .tracks
                                        .iter()
                                        .find(|t| t.id == tid)
                                        .and_then(|t| t.clips.get(cidx))
                                        .and_then(|c| {
                                            if let crate::models::Clip::Audio(ac) = c {
                                                let path = std::path::Path::new(&ac.source_file);
                                                crate::audio::load_audio_interleaved(path).ok().map(
                                                    |(raw, ch, sr)| {
                                                        raw.len() as f64
                                                            / (ch.max(1) as f64 * sr as f64)
                                                    },
                                                )
                                            } else {
                                                None
                                            }
                                        })
                                });
                                if let Some(dur) = total {
                                    state.audio_editor_selection = Some((0.0, dur));
                                    state.push_status("Selected entire waveform");
                                }
                            }
                            Keycode::S
                                if !input.ctrl()
                                    && state.focused_panel
                                        == crate::state::FocusedPanel::AudioEditor =>
                            {
                                // Toggle audio editor snap
                                state.audio_editor_snap_enabled = !state.audio_editor_snap_enabled;
                                let label = if state.audio_editor_snap_enabled {
                                    "Snap ON"
                                } else {
                                    "Snap OFF"
                                };
                                state.push_status(label);
                            }
                            Keycode::T
                                if !input.ctrl()
                                    && !piano_consumes
                                    && state.focused_panel
                                        == crate::state::FocusedPanel::AudioEditor =>
                            {
                                state.push_status("Press TRIM button or use toolbar");
                            }
                            Keycode::N
                                if !input.ctrl()
                                    && state.focused_panel
                                        == crate::state::FocusedPanel::AudioEditor =>
                            {
                                state.push_status("Press NORM button or use toolbar");
                            }
                            Keycode::T if !input.ctrl() && !piano_consumes => {
                                state.next_theme();
                            }
                            Keycode::Up
                                if input.shift()
                                    && state.focused_panel
                                        != crate::state::FocusedPanel::PianoRoll =>
                            {
                                if let Some(id) = state.selected_track {
                                    if let Some(i) =
                                        state.project.tracks.iter().position(|t| t.id == id)
                                    {
                                        if i > 0 {
                                            state.project.tracks.swap(i, i - 1);
                                            state.dirty = true;
                                        }
                                    }
                                }
                                input.consume_key(Keycode::Up);
                            }
                            Keycode::Down
                                if input.shift()
                                    && state.focused_panel
                                        != crate::state::FocusedPanel::PianoRoll =>
                            {
                                if let Some(id) = state.selected_track {
                                    if let Some(i) =
                                        state.project.tracks.iter().position(|t| t.id == id)
                                    {
                                        if i + 1 < state.project.tracks.len() {
                                            state.project.tracks.swap(i, i + 1);
                                            state.dirty = true;
                                        }
                                    }
                                }
                                input.consume_key(Keycode::Down);
                            }
                            Keycode::Plus | Keycode::Equals => {
                                state.arrangement.zoom_x =
                                    (state.arrangement.zoom_x * 1.2).min(200.0);
                            }
                            Keycode::Minus => {
                                state.arrangement.zoom_x =
                                    (state.arrangement.zoom_x / 1.2).max(5.0);
                            }
                            Keycode::J if !piano_consumes && !input.ctrl() => {
                                // Join adjacent selected clips on same track
                                let mut to_join: Vec<(u32, usize)> =
                                    state.selected_clips.iter().cloned().collect();
                                if to_join.is_empty() {
                                    if let Some(sel) = state.selected_clip {
                                        to_join.push(sel);
                                    }
                                }
                                if to_join.len() >= 2 {
                                    // Group by track_id
                                    let mut groups: std::collections::HashMap<u32, Vec<usize>> =
                                        std::collections::HashMap::new();
                                    for (tid, ci) in &to_join {
                                        groups.entry(*tid).or_default().push(*ci);
                                    }
                                    // Only keep groups with 2+ clips
                                    let join_groups: Vec<(u32, Vec<usize>)> =
                                        groups.into_iter().filter(|(_, v)| v.len() >= 2).collect();
                                    if !join_groups.is_empty() {
                                        state.commands.execute(
                                            Box::new(crate::commands::JoinClips {
                                                groups: join_groups,
                                            }),
                                            &mut state.project,
                                        );
                                        state.selected_clip = None;
                                        state.selected_clips.clear();
                                        state.dirty = true;
                                        state.push_status("Joined clips");
                                    } else {
                                        state.push_status(
                                            "Select 2+ clips on the same track to join",
                                        );
                                    }
                                } else {
                                    state.push_status("Select 2+ adjacent clips to join");
                                }
                                input.consume_key(Keycode::J);
                            }
                            Keycode::Delete | Keycode::Backspace => {
                                use crate::state::FocusedPanel;
                                match state.focused_panel {
                                    FocusedPanel::PianoRoll => {
                                        // Delete selected MIDI notes in the piano roll
                                        if let Some((tid, ci)) = state.selected_clip {
                                            if !state.piano_roll_selected_notes.is_empty() {
                                                let notes_to_delete: Vec<(
                                                    usize,
                                                    crate::models::MidiNote,
                                                )> = state
                                                    .piano_roll_selected_notes
                                                    .iter()
                                                    .cloned()
                                                    .filter_map(|ni| {
                                                        state
                                                            .project
                                                            .tracks
                                                            .iter()
                                                            .find(|t| t.id == tid)
                                                            .and_then(|t| t.clips.get(ci))
                                                            .and_then(|c| {
                                                                if let models::Clip::Midi(m) = c {
                                                                    m.notes
                                                                        .get(ni)
                                                                        .cloned()
                                                                        .map(|n| (ni, n))
                                                                } else {
                                                                    None
                                                                }
                                                            })
                                                    })
                                                    .collect();
                                                if !notes_to_delete.is_empty() {
                                                    state.commands.execute(
                                                        Box::new(
                                                            crate::commands::DeleteMidiNotes {
                                                                track_id: tid,
                                                                clip_idx: ci,
                                                                notes: notes_to_delete,
                                                            },
                                                        ),
                                                        &mut state.project,
                                                    );
                                                    state.piano_roll_selected_notes.clear();
                                                    state.dirty = true;
                                                }
                                            }
                                        }
                                    }
                                    FocusedPanel::Arrangement => {
                                        // Delete all selected clips
                                        let mut to_remove: Vec<(u32, usize)> =
                                            state.selected_clips.iter().cloned().collect();
                                        if to_remove.is_empty() {
                                            if let Some(sel) = state.selected_clip {
                                                to_remove.push(sel);
                                            }
                                        }
                                        if !to_remove.is_empty() {
                                            let clips_placeholder: Vec<(u32, usize, models::Clip)> =
                                                to_remove
                                                    .iter()
                                                    .filter_map(|&(tid, ci)| {
                                                        state
                                                            .project
                                                            .tracks
                                                            .iter()
                                                            .find(|t| t.id == tid)
                                                            .and_then(|t| {
                                                                t.clips
                                                                    .get(ci)
                                                                    .cloned()
                                                                    .map(|c| (tid, ci, c))
                                                            })
                                                    })
                                                    .collect();
                                            state.commands.execute(
                                                Box::new(crate::commands::DeleteClips {
                                                    clips: clips_placeholder,
                                                }),
                                                &mut state.project,
                                            );
                                            state.selected_clip = None;
                                            state.selected_clips.clear();
                                            state.dirty = true;
                                        }
                                    }
                                    _ => {}
                                }
                                input.consume_key(Keycode::Delete);
                                input.consume_key(Keycode::Backspace);
                            }
                            Keycode::C if input.ctrl() => {
                                // Copy selected clips to clipboard
                                state.clipboard.clear();
                                let mut clips: Vec<(u32, crate::models::Clip)> = Vec::new();
                                for (tid, ci) in &state.selected_clips {
                                    if let Some(track) =
                                        state.project.tracks.iter().find(|t| t.id == *tid)
                                    {
                                        if let Some(clip) = track.clips.get(*ci) {
                                            clips.push((*tid, clip.clone()));
                                        }
                                    }
                                }
                                if clips.is_empty() {
                                    if let Some((tid, ci)) = state.selected_clip {
                                        if let Some(track) =
                                            state.project.tracks.iter().find(|t| t.id == tid)
                                        {
                                            if let Some(clip) = track.clips.get(ci) {
                                                clips.push((tid, clip.clone()));
                                            }
                                        }
                                    }
                                }
                                state.clipboard = clips;
                            }
                            Keycode::V if input.ctrl() => {
                                // Paste clipboard at playhead — same track type only
                                let paste_pos = state.project.transport.position;
                                if !state.clipboard.is_empty() {
                                    let min_start = state
                                        .clipboard
                                        .iter()
                                        .map(|(_, c)| c.start_time())
                                        .fold(f64::INFINITY, f64::min);
                                    let offset = paste_pos - min_start;
                                    let clipboard_copy = state.clipboard.clone();

                                    let mut new_clips = Vec::new();
                                    for (tid, clip) in clipboard_copy {
                                        // Determine the clip's required track type
                                        let required_type = match &clip {
                                            crate::models::Clip::Midi(_) => {
                                                crate::models::TrackType::Midi
                                            }
                                            crate::models::Clip::Audio(_) => {
                                                crate::models::TrackType::Audio
                                            }
                                            crate::models::Clip::Automation(_) => {
                                                crate::models::TrackType::Automation
                                            }
                                        };
                                        // Try original track first, then first track of same type
                                        let target_id =
                                            if state.project.tracks.iter().any(|t| {
                                                t.id == tid && t.track_type == required_type
                                            }) {
                                                Some(tid)
                                            } else {
                                                state
                                                    .project
                                                    .tracks
                                                    .iter()
                                                    .find(|t| t.track_type == required_type)
                                                    .map(|t| t.id)
                                            };
                                        if let Some(target_tid) = target_id {
                                            let mut new_clip = clip.clone();
                                            let new_start =
                                                (new_clip.start_time() + offset).max(0.0);
                                            match &mut new_clip {
                                                crate::models::Clip::Midi(c) => {
                                                    c.start_time = new_start
                                                }
                                                crate::models::Clip::Audio(c) => {
                                                    c.start_time = new_start
                                                }
                                                crate::models::Clip::Automation(c) => {
                                                    c.start_time = new_start
                                                }
                                            }
                                            new_clips.push((target_tid, new_clip));
                                        }
                                    }

                                    if !new_clips.is_empty() {
                                        state.selected_clips.clear();
                                        for (tid, _) in &new_clips {
                                            let track = state
                                                .project
                                                .tracks
                                                .iter()
                                                .find(|t| t.id == *tid)
                                                .unwrap();
                                            let idx = track.clips.len()
                                                + state
                                                    .selected_clips
                                                    .iter()
                                                    .filter(|(t, _)| t == tid)
                                                    .count();
                                            state.selected_clips.insert((*tid, idx));
                                        }

                                        state.commands.execute(
                                            Box::new(crate::commands::AddClips {
                                                clips: new_clips,
                                                added_indices: Vec::new(),
                                            }),
                                            &mut state.project,
                                        );
                                        state.dirty = true;
                                    }
                                }
                            }
                            Keycode::D if input.ctrl() => {
                                use crate::state::FocusedPanel;
                                match state.focused_panel {
                                    FocusedPanel::PianoRoll => {
                                        // Duplicate selected notes in the piano roll (undoable)
                                        if let Some((tid, ci)) = state.selected_clip {
                                            if !state.piano_roll_selected_notes.is_empty() {
                                                let notes: Vec<crate::models::MidiNote> = state
                                                    .piano_roll_selected_notes
                                                    .iter()
                                                    .filter_map(|&ni| {
                                                        state
                                                            .project
                                                            .tracks
                                                            .iter()
                                                            .find(|t| t.id == tid)
                                                            .and_then(|t| t.clips.get(ci))
                                                            .and_then(|c| {
                                                                if let crate::models::Clip::Midi(
                                                                    m,
                                                                ) = c
                                                                {
                                                                    m.notes.get(ni).cloned()
                                                                } else {
                                                                    None
                                                                }
                                                            })
                                                    })
                                                    .collect();
                                                if !notes.is_empty() {
                                                    // Compute span to offset duplicates
                                                    let min_start = notes
                                                        .iter()
                                                        .map(|n| n.start)
                                                        .fold(f64::MAX, f64::min);
                                                    let max_end = notes
                                                        .iter()
                                                        .map(|n| n.start + n.length)
                                                        .fold(0.0_f64, f64::max);
                                                    let offset = (max_end - min_start).max(0.25);
                                                    let new_notes: Vec<crate::models::MidiNote> =
                                                        notes
                                                            .iter()
                                                            .map(|note| {
                                                                let mut dup = note.clone();
                                                                dup.start += offset;
                                                                dup
                                                            })
                                                            .collect();
                                                    let count = new_notes.len();
                                                    // Get current note count for selecting new notes after command
                                                    let base = state
                                                        .project
                                                        .tracks
                                                        .iter()
                                                        .find(|t| t.id == tid)
                                                        .and_then(|t| t.clips.get(ci))
                                                        .and_then(|c| {
                                                            if let crate::models::Clip::Midi(m) = c
                                                            {
                                                                Some(m.notes.len())
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                        .unwrap_or(0);
                                                    state.commands.execute(
                                                        Box::new(crate::commands::DuplicateNotes {
                                                            track_id: tid,
                                                            clip_idx: ci,
                                                            new_notes,
                                                            count: 0,
                                                        }),
                                                        &mut state.project,
                                                    );
                                                    // Select the new notes
                                                    state.piano_roll_selected_notes.clear();
                                                    for i in 0..count {
                                                        state
                                                            .piano_roll_selected_notes
                                                            .insert(base + i);
                                                    }
                                                    state.dirty = true;
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        // Duplicate selected clips, placing them immediately after originals
                                        let mut to_dup: Vec<(u32, usize)> =
                                            state.selected_clips.iter().cloned().collect();
                                        if to_dup.is_empty() {
                                            if let Some(sel) = state.selected_clip {
                                                to_dup.push(sel);
                                            }
                                        }

                                        // Find the span of all selected clips (max end - min start)
                                        let mut span_min = f64::MAX;
                                        let mut span_max = 0.0_f64;
                                        for &(tid, ci) in &to_dup {
                                            if let Some(track) =
                                                state.project.tracks.iter().find(|t| t.id == tid)
                                            {
                                                if let Some(orig) = track.clips.get(ci) {
                                                    let s = orig.start_time();
                                                    let e = s + orig.length();
                                                    if s < span_min {
                                                        span_min = s;
                                                    }
                                                    if e > span_max {
                                                        span_max = e;
                                                    }
                                                }
                                            }
                                        }
                                        let dup_offset = if span_max > span_min {
                                            span_max - span_min
                                        } else {
                                            1.0 // fallback
                                        };

                                        let mut new_clips = Vec::new();
                                        for (tid, ci) in to_dup {
                                            if let Some(track) =
                                                state.project.tracks.iter().find(|t| t.id == tid)
                                            {
                                                if let Some(orig) = track.clips.get(ci).cloned() {
                                                    let new_start = orig.start_time() + dup_offset;
                                                    let mut dup = orig.clone();
                                                    match &mut dup {
                                                        crate::models::Clip::Midi(c) => {
                                                            c.start_time = new_start
                                                        }
                                                        crate::models::Clip::Audio(c) => {
                                                            c.start_time = new_start
                                                        }
                                                        crate::models::Clip::Automation(c) => {
                                                            c.start_time = new_start
                                                        }
                                                    }
                                                    new_clips.push((tid, dup));
                                                }
                                            }
                                        }

                                        if !new_clips.is_empty() {
                                            // Select newly added clips: they will be appended to tracks
                                            state.selected_clips.clear();
                                            for (tid, _) in &new_clips {
                                                let track = state
                                                    .project
                                                    .tracks
                                                    .iter()
                                                    .find(|t| t.id == *tid)
                                                    .unwrap();
                                                let idx = track.clips.len()
                                                    + state
                                                        .selected_clips
                                                        .iter()
                                                        .filter(|(t, _)| t == tid)
                                                        .count();
                                                state.selected_clips.insert((*tid, idx));
                                            }

                                            state.commands.execute(
                                                Box::new(crate::commands::AddClips {
                                                    clips: new_clips,
                                                    added_indices: Vec::new(),
                                                }),
                                                &mut state.project,
                                            );
                                            state.dirty = true;
                                        }
                                    } // end _ => (arrangement duplicate)
                                } // end match focused_panel
                            }
                            Keycode::A if input.ctrl() => {
                                use crate::state::FocusedPanel;
                                match state.focused_panel {
                                    FocusedPanel::Arrangement => {
                                        // Select all clips across all tracks
                                        state.selected_clips.clear();
                                        for track in &state.project.tracks {
                                            for (ci, _) in track.clips.iter().enumerate() {
                                                state.selected_clips.insert((track.id, ci));
                                            }
                                        }
                                    }
                                    _ => {
                                        // Piano roll Ctrl+A is handled in views.rs
                                    }
                                }
                            }
                            _ => {}
                        }

                        // ── Computer keyboard piano mode ──
                        if state.piano_keyboard_mode
                            && !input.ctrl()
                            && state.mode != AppMode::ProjectManager
                        {
                            if let Some(semitone) = qwerty_to_semitone(key) {
                                let pitch = ((state.piano_keyboard_octave * 12) + semitone)
                                    .clamp(0, 127)
                                    as u8;
                                if !state.piano_keyboard_held.contains(&pitch) {
                                    state.piano_keyboard_held.insert(pitch);
                                    // Find the selected track index for preview
                                    if let Some(tid) = state
                                        .selected_track
                                        .or_else(|| state.selected_clip.map(|(t, _)| t))
                                    {
                                        if let Some(ti) =
                                            state.project.tracks.iter().position(|t| t.id == tid)
                                        {
                                            state.preview_notes.push((ti, pitch, 100));
                                        }
                                    }
                                }
                            }
                            // Octave shift with Z/X
                            match key {
                                Keycode::Z if !input.ctrl() => {
                                    state.piano_keyboard_octave =
                                        (state.piano_keyboard_octave - 1).max(0);
                                }
                                Keycode::X if !input.ctrl() => {
                                    state.piano_keyboard_octave =
                                        (state.piano_keyboard_octave + 1).min(9);
                                }
                                _ => {}
                            }
                        }
                    } // end else (text_field_active_id == 0)
                }
                Event::KeyUp {
                    keycode: Some(key),
                    keymod,
                    ..
                } => {
                    input.key_mod = keymod;
                    input.on_key_up(key);
                    // Release held piano key
                    if state.piano_keyboard_mode {
                        if let Some(semitone) = qwerty_to_semitone(key) {
                            let pitch =
                                ((state.piano_keyboard_octave * 12) + semitone).clamp(0, 127) as u8;
                            state.piano_keyboard_held.remove(&pitch);
                            // Send note-off to audio thread
                            state.piano_note_off_queue.push(pitch);
                        }
                    }
                }
                Event::TextInput { text, .. } => {
                    for ch in text.chars() {
                        input.text_input_chars.push(ch);
                    }
                }
                Event::DropFile { filename, .. } => {
                    input.dropped_file = Some(filename);
                }
                _ => {}
            }
        }

        // DON'T call end_frame yet - widgets need active_widget to detect clicks!
        // input.end_frame(); // MOVED to after drawing

        // Advance playhead position (UI-side fallback when no audio engine)
        if state.project.transport.playing && audio_shared.is_none() {
            let bpm = state
                .project
                .tempo_map
                .bpm_at(state.project.transport.position);
            let beats_per_sec = bpm / 60.0;
            state.project.transport.position += beats_per_sec * dt;

            if state.project.transport.loop_enabled
                && state.project.transport.position >= state.project.transport.loop_region.end
            {
                state.project.transport.position = state.project.transport.loop_region.start;
            }
        }

        // ── Sample preview loading ──
        if state.sample_preview_trigger {
            state.sample_preview_trigger = false;
            if let Some(ref path) = state.sample_preview_path {
                // Detect MIDI files
                let is_midi = std::path::Path::new(path.as_os_str())
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| {
                        let el = e.to_lowercase();
                        el == "mid" || el == "midi"
                    })
                    .unwrap_or(false);

                if is_midi {
                    // Render MIDI through default Analog synth for preview
                    let path_str = path.to_string_lossy().to_string();
                    let bpm = state.project.tempo_map.bpm_at(0.0);
                    match crate::models::import_midi_file(&path_str, bpm) {
                        Ok(tracks_data) => {
                            // Build a temporary project with the imported MIDI
                            let mut preview_proj = crate::models::Project::default();
                            if let Some(entry) = preview_proj.tempo_map.changes.first_mut() {
                                entry.bpm = bpm;
                            }
                            for (i, (track_name, midi_clip)) in tracks_data.into_iter().enumerate()
                            {
                                let mut t = crate::models::Track::new(
                                    (i + 1) as u32,
                                    &track_name,
                                    crate::models::TrackType::Midi,
                                );
                                t.rack = vec![crate::models::RackSlot::subtractive_synth(1)];
                                t.clips.push(crate::models::Clip::Midi(midi_clip));
                                preview_proj.tracks.push(t);
                            }
                            // Render to a temporary WAV file
                            let tmp_path = std::env::temp_dir().join("eden_midi_preview.wav");
                            let tmp_str = tmp_path.to_string_lossy().to_string();
                            let settings = crate::render::RenderSettings::default();
                            match crate::render::render_to_wav(&preview_proj, &tmp_str, &settings) {
                                Ok(()) => {
                                    // Load the rendered WAV for preview playback
                                    match crate::audio::load_wav(&tmp_path) {
                                        Ok((samples, sr)) => {
                                            println!(
                                                "[preview] MIDI rendered {} samples at {}Hz",
                                                samples.len(),
                                                sr
                                            );
                                            if let Some(ref shared) = audio_shared {
                                                if let Ok(mut audio) = shared.try_lock() {
                                                    audio.preview_samples =
                                                        std::sync::Arc::new(samples);
                                                    audio.preview_sample_rate = sr;
                                                    audio.preview_pos = 0;
                                                    audio.preview_playing = true;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "[preview] Failed to load rendered MIDI: {}",
                                                e
                                            );
                                            state.sample_preview_path = None;
                                        }
                                    }
                                    let _ = std::fs::remove_file(&tmp_path);
                                }
                                Err(e) => {
                                    eprintln!("[preview] MIDI render failed: {}", e);
                                    state.sample_preview_path = None;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[preview] MIDI import failed: {}", e);
                            state.sample_preview_path = None;
                        }
                    }
                } else {
                    // Use the audio sample cache so we don't re-read from disk
                    let path_str = path.to_string_lossy().to_string();
                    if !audio_sample_cache.contains_key(&path_str) {
                        match crate::audio::load_audio(path) {
                            Ok((samples, sr)) => {
                                audio_sample_cache
                                    .insert(path_str.clone(), (std::sync::Arc::new(samples), sr));
                            }
                            Err(e) => {
                                eprintln!("[preview] Failed to load {:?}: {}", path, e);
                                state.sample_preview_path = None;
                            }
                        }
                    }
                    if let Some((samples, sr)) = audio_sample_cache.get(&path_str) {
                        if let Some(ref shared) = audio_shared {
                            if let Ok(mut audio) = shared.try_lock() {
                                audio.preview_samples = samples.clone();
                                audio.preview_sample_rate = *sr;
                                audio.preview_pos = state.sample_preview_start_sample;
                                audio.preview_end_sample = state.sample_preview_end_sample;
                                // Set loop state from audio editor
                                audio.preview_loop_enabled = state.audio_editor_loop_enabled
                                    && state.audio_editor_loop_end > state.audio_editor_loop_start
                                    && state.focused_panel
                                        == crate::state::FocusedPanel::AudioEditor;
                                audio.preview_loop_start = if audio.preview_loop_enabled {
                                    (state.audio_editor_loop_start * (*sr) as f64) as usize
                                } else {
                                    0
                                };
                                audio.preview_playing = true;
                            }
                        }
                    }
                }
            }
        }
        // ── Quick preview state sync (single try_lock to minimize contention) ──
        if let Some(ref shared) = audio_shared {
            if let Ok(mut audio) = shared.try_lock() {
                // Stop preview when path is cleared
                if state.sample_preview_path.is_none() && !state.sample_preview_trigger {
                    state.sample_preview_start_sample = 0;
                    state.sample_preview_end_sample = 0;
                    if audio.preview_playing {
                        audio.preview_playing = false;
                        audio.preview_end_sample = 0;
                        audio.preview_loop_enabled = false;
                    }
                }
                // Check if preview finished playing
                if state.sample_preview_path.is_some()
                    && !audio.preview_playing
                    && !state.sample_preview_trigger
                    && !audio.preview_loop_enabled
                {
                    state.sample_preview_path = None;
                    state.audio_editor_playing = false;
                }
                // Read back preview playhead position
                if state.audio_editor_playing {
                    let sr = audio.preview_sample_rate as f64;
                    if sr > 0.0 {
                        state.audio_editor_playhead = audio.preview_pos as f64 / sr;
                    }
                }
            }
        }

        if let Some(ref shared) = audio_shared {
            // ── Prepare track data OUTSIDE the lock to minimize mutex hold time ──
            // This prevents the audio callback from starving (its try_lock would fail
            // if we hold the mutex while doing expensive work like sample loading).

            // Apply automation clips to rack params (modifies state, no lock needed)
            let cur_pos = state.project.transport.position;
            let auto_values: Vec<(String, f32)> = state
                .project
                .tracks
                .iter()
                .filter(|t| t.track_type == models::TrackType::Automation && t.automation_enabled)
                .flat_map(|t| {
                    t.clips.iter().filter_map(|c| {
                        if let models::Clip::Automation(ac) = c {
                            let clip_end = ac.start_time + ac.length;
                            if cur_pos < ac.start_time || cur_pos > clip_end {
                                return None;
                            }
                            let clip_pos = cur_pos - ac.start_time;
                            if ac.points.is_empty() {
                                return None;
                            }
                            let value = if ac.points.len() == 1 {
                                ac.points[0].value
                            } else {
                                let mut before = &ac.points[0];
                                let mut after = &ac.points[ac.points.len() - 1];
                                for i in 0..ac.points.len().saturating_sub(1) {
                                    if ac.points[i].time <= clip_pos
                                        && ac.points[i + 1].time >= clip_pos
                                    {
                                        before = &ac.points[i];
                                        after = &ac.points[i + 1];
                                        break;
                                    }
                                }
                                let dt = after.time - before.time;
                                if dt <= 0.0 {
                                    before.value
                                } else {
                                    let t = ((clip_pos - before.time) / dt).clamp(0.0, 1.0) as f32;
                                    before.value + t * (after.value - before.value)
                                }
                            };
                            Some((ac.target_param.clone(), value))
                        } else {
                            None
                        }
                    })
                })
                .collect();

            // Write automation values to rack params
            for (target_key, auto_val) in &auto_values {
                let parts: Vec<&str> = target_key.split(':').collect();
                if parts.len() != 3 {
                    continue;
                }
                let (Ok(track_id), Ok(slot_id)) =
                    (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                else {
                    continue;
                };
                let param_id = parts[2];
                if let Some(track) = state.project.tracks.iter_mut().find(|t| t.id == track_id) {
                    if let Some(slot) = track.rack.iter_mut().find(|s| s.slot_id == slot_id) {
                        if let Some(param) = slot.params.iter_mut().find(|p| p.id == param_id) {
                            let mapped = param.min + auto_val * (param.max - param.min);
                            param.value = mapped.clamp(param.min, param.max);
                        }
                    }
                }
            }

            // Drain audio sample invalidation requests (modifies cache, no lock needed)
            for path in state.audio_sample_invalidate.drain(..) {
                audio_sample_cache.remove(&path);
            }

            // Build track data outside the lock
            let mut prepared_tracks: Vec<audio::AudioTrack> =
                Vec::with_capacity(state.project.tracks.len());
            for track in &state.project.tracks {
                let mut midi_clips = Vec::new();
                let mut audio_clips_vec = Vec::new();

                let instrument_module: Option<String> = track
                    .rack
                    .iter()
                    .filter(|slot| slot.enabled)
                    .find(|slot| modules::is_instrument(&slot.plugin_name))
                    .map(|slot| slot.plugin_name.clone());

                let instrument_params: Vec<(String, f32)> = track
                    .rack
                    .iter()
                    .filter(|slot| slot.enabled)
                    .find(|slot| modules::is_instrument(&slot.plugin_name))
                    .map(|slot| {
                        slot.params
                            .iter()
                            .map(|p| (p.id.clone(), p.value))
                            .collect()
                    })
                    .unwrap_or_default();

                let effect_slots: Vec<(String, Vec<(String, f32)>)> = track
                    .rack
                    .iter()
                    .filter(|slot| slot.enabled && modules::is_effect(&slot.plugin_name))
                    .map(|slot| {
                        let params: Vec<(String, f32)> = slot
                            .params
                            .iter()
                            .map(|p| (p.id.clone(), p.value))
                            .collect();
                        (slot.plugin_name.clone(), params)
                    })
                    .collect();

                let midi_effect_slots: Vec<(String, Vec<(String, f32)>)> = track
                    .rack
                    .iter()
                    .filter(|slot| slot.enabled && modules::is_midi_effect(&slot.plugin_name))
                    .map(|slot| {
                        let params: Vec<(String, f32)> = slot
                            .params
                            .iter()
                            .map(|p| (p.id.clone(), p.value))
                            .collect();
                        (slot.plugin_name.clone(), params)
                    })
                    .collect();

                let effect_sidechain_track: Vec<Option<usize>> = track
                    .rack
                    .iter()
                    .filter(|slot| slot.enabled && modules::is_effect(&slot.plugin_name))
                    .map(|slot| {
                        slot.sidechain_track_id.and_then(|sc_id| {
                            state.project.tracks.iter().position(|t| t.id == sc_id)
                        })
                    })
                    .collect();

                let extra = if instrument_module.as_deref() == Some("Sampler") {
                    if let Some(ref sample_path) = track.sampler_file {
                        if !sample_path.is_empty() {
                            if !audio_sample_cache.contains_key(sample_path) {
                                let path = std::path::Path::new(sample_path);
                                match audio::load_audio(path) {
                                    Ok((samples, sr)) => {
                                        audio_sample_cache.insert(
                                            sample_path.clone(),
                                            (std::sync::Arc::new(samples), sr),
                                        );
                                    }
                                    Err(_) => {
                                        audio_sample_cache.insert(
                                            sample_path.clone(),
                                            (std::sync::Arc::new(Vec::new()), 44100),
                                        );
                                    }
                                }
                            }
                            if let Some((samples, sr)) = audio_sample_cache.get(sample_path) {
                                modules::ModuleExtra {
                                    sample_data: Some(samples.clone()),
                                    sample_sr: *sr,
                                }
                            } else {
                                modules::ModuleExtra::default()
                            }
                        } else {
                            modules::ModuleExtra::default()
                        }
                    } else {
                        modules::ModuleExtra::default()
                    }
                } else {
                    modules::ModuleExtra::default()
                };

                if track.track_type == models::TrackType::Midi {
                    for clip in &track.clips {
                        if let models::Clip::Midi(mc) = clip {
                            let notes = mc
                                .notes
                                .iter()
                                .map(|n| audio::AudioNote {
                                    pitch: n.pitch,
                                    velocity: n.velocity,
                                    start_beats: n.start,
                                    length_beats: n.length,
                                })
                                .collect();
                            midi_clips.push(audio::AudioMidiClip {
                                start_beats: mc.start_time,
                                length_beats: mc.length,
                                notes,
                            });
                        }
                    }
                }
                for clip in &track.clips {
                    if let models::Clip::Audio(ac) = clip {
                        if ac.source_file.is_empty() {
                            continue;
                        }
                        if !audio_sample_cache.contains_key(&ac.source_file) {
                            let path = std::path::Path::new(&ac.source_file);
                            match audio::load_audio(path) {
                                Ok((samples, sr)) => {
                                    audio_sample_cache.insert(
                                        ac.source_file.clone(),
                                        (std::sync::Arc::new(samples), sr),
                                    );
                                }
                                Err(_) => {
                                    audio_sample_cache.insert(
                                        ac.source_file.clone(),
                                        (std::sync::Arc::new(Vec::new()), 44100),
                                    );
                                }
                            }
                        }
                        if let Some((samples, sr)) = audio_sample_cache.get(&ac.source_file) {
                            if !samples.is_empty() {
                                audio_clips_vec.push(audio::AudioSampleClip {
                                    start_beats: ac.start_time,
                                    length_beats: ac.length,
                                    gain: ac.gain,
                                    offset_secs: ac.offset,
                                    samples: samples.clone(),
                                    sample_rate: *sr,
                                    fade_in: ac.fade_in,
                                    fade_out: ac.fade_out,
                                });
                            }
                        }
                    }
                }

                prepared_tracks.push(audio::AudioTrack {
                    volume: track.volume,
                    pan: track.pan,
                    mute: track.mute,
                    solo: track.solo,
                    is_automation: track.track_type == models::TrackType::Automation,
                    midi_clips,
                    audio_clips: audio_clips_vec,
                    instrument_module,
                    instrument_params,
                    effect_slots,
                    midi_effect_slots,
                    effect_sidechain_track,
                    cstrip2_params: track.cstrip2_params.clone(),
                    cstrip2_bypass: track.cstrip2_bypass,
                    extra,
                });
            }

            // Prepare master rack effects outside the lock
            let prepared_master_effects: Vec<(String, Vec<(String, f32)>)> = state
                .project
                .master_rack
                .iter()
                .filter(|s| s.enabled && modules::is_effect(&s.plugin_name))
                .map(|s| {
                    let params: Vec<(String, f32)> =
                        s.params.iter().map(|p| (p.id.clone(), p.value)).collect();
                    (s.plugin_name.clone(), params)
                })
                .collect();

            // Prepare keyboard pitches outside the lock
            let prepared_held_pitches: Vec<(usize, Vec<u8>)> =
                if state.piano_keyboard_mode && !state.piano_keyboard_held.is_empty() {
                    if let Some(tid) = state
                        .selected_track
                        .or_else(|| state.selected_clip.map(|(t, _)| t))
                    {
                        if let Some(ti) = state.project.tracks.iter().position(|t| t.id == tid) {
                            let mut pitches: Vec<u8> =
                                state.piano_keyboard_held.iter().copied().collect();
                            pitches.sort_unstable();
                            vec![(ti, pitches)]
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

            // ── Now acquire the lock briefly just to swap data in/out ──
            if let Ok(mut audio) = shared.try_lock() {
                audio.playing = state.project.transport.playing;
                audio.bpm = state.project.tempo_map.bpm_at(0.0);

                if state.seek_pending {
                    audio.position_beats = state.project.transport.position;
                    audio.seek_pending = true;
                    state.seek_pending = false;
                } else if state.project.transport.playing {
                    state.project.transport.position = audio.position_beats;
                } else {
                    audio.position_beats = state.project.transport.position;
                }

                audio.loop_enabled = state.project.transport.loop_enabled;
                audio.loop_start = state.project.transport.loop_region.start;
                audio.loop_end = state.project.transport.loop_region.end;
                audio.master_volume = state.master_volume_ui;

                if !state.preview_notes.is_empty() {
                    audio.preview_notes.append(&mut state.preview_notes);
                }

                audio.preview_sustain = state.piano_keyboard_mode;
                audio.preview_held_pitches = prepared_held_pitches;

                if !state.piano_note_off_queue.is_empty() {
                    audio
                        .preview_note_off
                        .append(&mut state.piano_note_off_queue);
                }

                if state.panic_triggered {
                    audio.panic = true;
                    state.panic_triggered = false;
                }

                // Swap in pre-built track data (fast: just a Vec swap)
                audio.tracks = prepared_tracks;
                audio.master_effects = prepared_master_effects;

                // Read metering data back into UI state
                state.meters.track_rms = audio.track_rms.clone();
                state.meters.track_rms_pre_effect = audio.track_rms_pre_effect.clone();
                state.meters.track_rms_l = audio.track_rms_l.clone();
                state.meters.track_rms_r = audio.track_rms_r.clone();
                state.meters.oscilloscope = audio.oscilloscope.clone();
                state.meters.master_rms = audio.master_rms;
                state.meters.master_rms_pre = audio.master_rms_pre;
                state.meters.master_rms_l = audio.master_rms_l;
                state.meters.master_rms_r = audio.master_rms_r;
                state.meters.master_rms_post_l = audio.master_rms_post_l;
                state.meters.master_rms_post_r = audio.master_rms_post_r;
                state.meters.master_true_peak_post_l = audio.master_true_peak_post_l;
                state.meters.master_true_peak_post_r = audio.master_true_peak_post_r;
                state.meters.track_effect_gr = audio.track_effect_gr.clone();
                state.meters.master_effect_gr = audio.master_effect_gr.clone();
                state.meters.preview_rms_l = audio.preview_rms_l;
                state.meters.preview_rms_r = audio.preview_rms_r;

                // Update VU ballistic needles (GUI-side, ~300ms attack/decay)
                {
                    let n = state.project.tracks.len();
                    if state.meters.vu_needle.len() != n {
                        state.meters.vu_needle.resize(n, 0.0);
                    }
                    if state.meters.vu_peak_needle.len() != n {
                        state.meters.vu_peak_needle.resize(n, 0.0);
                    }
                    if state.meters.vu_peak_hold_frames.len() != n {
                        state.meters.vu_peak_hold_frames.resize(n, 0);
                    }
                    // VU ballistic: ~300ms integration (attack ≈ decay ≈ 0.1 per frame @ 60fps)
                    let vu_coeff = 0.10_f32;
                    // Peak needle: fast attack (~instant), very slow decay
                    let peak_attack = 0.6_f32; // fast rise
                    let peak_decay = 0.008_f32; // slow fall (~5s from 1→0 @ 60fps)
                    let peak_hold_time = 90u32; // ~1.5s hold at 60fps before decay starts
                    for i in 0..n {
                        let rms = state.meters.track_rms.get(i).copied().unwrap_or(0.0);
                        // Convert to VU scale: 0dBVU ≈ 0.3162 (~-10dBFS), needle 0–1
                        let vu_db = if rms > 1e-6 {
                            20.0 * rms.log10()
                        } else {
                            -60.0
                        };
                        // Map: -20dBFS → 0.0 (left), 0dBFS → ~0.77, +3dBFS → 1.0
                        let vu_pos = ((vu_db + 20.0) / 23.0).clamp(0.0, 1.0);
                        state.meters.vu_needle[i] +=
                            (vu_pos - state.meters.vu_needle[i]) * vu_coeff;
                        // Peak needle: jumps up quickly, holds, then decays very slowly
                        if vu_pos > state.meters.vu_peak_needle[i] {
                            state.meters.vu_peak_needle[i] +=
                                (vu_pos - state.meters.vu_peak_needle[i]) * peak_attack;
                            state.meters.vu_peak_hold_frames[i] = peak_hold_time;
                        // reset hold
                        } else if state.meters.vu_peak_hold_frames[i] > 0 {
                            // Holding at peak — don't decay yet
                            state.meters.vu_peak_hold_frames[i] -= 1;
                        } else {
                            // Hold expired, start slow decay
                            state.meters.vu_peak_needle[i] -= peak_decay;
                            if state.meters.vu_peak_needle[i] < 0.0 {
                                state.meters.vu_peak_needle[i] = 0.0;
                            }
                        }
                    }
                    // Master stereo peak-hold + clipping
                    let ml = state.meters.master_rms_l;
                    let mr = state.meters.master_rms_r;
                    state.meters.master_peak_hold_l = if ml > state.meters.master_peak_hold_l {
                        ml
                    } else {
                        (state.meters.master_peak_hold_l - 0.002).max(0.0)
                    };
                    state.meters.master_peak_hold_r = if mr > state.meters.master_peak_hold_r {
                        mr
                    } else {
                        (state.meters.master_peak_hold_r - 0.002).max(0.0)
                    };
                    state.meters.master_peak_l = ml.max(state.meters.master_peak_l * 0.995);
                    state.meters.master_peak_r = mr.max(state.meters.master_peak_r * 0.995);
                    if ml >= 0.98 {
                        state.meters.master_clipping_l = true;
                    }
                    if mr >= 0.98 {
                        state.meters.master_clipping_r = true;
                    }
                    // Post-output (Out pair) peak hold — driven from true instantaneous
                    // peak so the user can verify the limiter ceiling is being honoured.
                    // Uses the same hold-then-decay pattern as the track VU peak needles.
                    let pl = state.meters.master_true_peak_post_l;
                    let pr = state.meters.master_true_peak_post_r;
                    const OUT_PEAK_HOLD_FRAMES: u32 = 90;
                    const OUT_PEAK_DECAY: f32 = 0.002;
                    if pl > state.meters.master_peak_hold_post_l {
                        state.meters.master_peak_hold_post_l = pl;
                        state.meters.master_peak_hold_post_frames_l = OUT_PEAK_HOLD_FRAMES;
                    } else if state.meters.master_peak_hold_post_frames_l > 0 {
                        state.meters.master_peak_hold_post_frames_l -= 1;
                    } else {
                        state.meters.master_peak_hold_post_l =
                            (state.meters.master_peak_hold_post_l - OUT_PEAK_DECAY).max(0.0);
                    }
                    if pr > state.meters.master_peak_hold_post_r {
                        state.meters.master_peak_hold_post_r = pr;
                        state.meters.master_peak_hold_post_frames_r = OUT_PEAK_HOLD_FRAMES;
                    } else if state.meters.master_peak_hold_post_frames_r > 0 {
                        state.meters.master_peak_hold_post_frames_r -= 1;
                    } else {
                        state.meters.master_peak_hold_post_r =
                            (state.meters.master_peak_hold_post_r - OUT_PEAK_DECAY).max(0.0);
                    }
                    // Master VU ballistic needle (same logic as track VU)
                    {
                        let m_rms = state
                            .meters
                            .master_rms_post_l
                            .max(state.meters.master_rms_post_r);
                        let m_vu_db = if m_rms > 1e-6 {
                            20.0 * m_rms.log10()
                        } else {
                            -60.0_f32
                        };
                        let m_vu_pos = ((m_vu_db + 20.0) / 23.0).clamp(0.0, 1.0);
                        let vu_coeff = 0.10_f32;
                        let peak_attack = 0.6_f32;
                        let peak_decay = 0.008_f32;
                        let peak_hold_time = 90u32;
                        state.meters.master_vu_needle +=
                            (m_vu_pos - state.meters.master_vu_needle) * vu_coeff;
                        if m_vu_pos > state.meters.master_vu_peak_needle {
                            state.meters.master_vu_peak_needle +=
                                (m_vu_pos - state.meters.master_vu_peak_needle) * peak_attack;
                            state.meters.master_vu_peak_hold_frames = peak_hold_time;
                        } else if state.meters.master_vu_peak_hold_frames > 0 {
                            state.meters.master_vu_peak_hold_frames -= 1;
                        } else {
                            state.meters.master_vu_peak_needle =
                                (state.meters.master_vu_peak_needle - peak_decay).max(0.0);
                        }
                    }
                    // Stereo correlation: cos(angle between L and R vectors) ≈ (L·R) / (|L|·|R|)
                    // Use RMS as proxy: correlation ≈ 2*L*R / (L²+R²), range -1..+1
                    {
                        let l = state.meters.master_rms_post_l;
                        let r = state.meters.master_rms_post_r;
                        let denom = l * l + r * r;
                        let corr_raw = if denom > 1e-10 {
                            2.0 * l * r / denom
                        } else {
                            1.0_f32
                        };
                        // Smooth toward new value
                        state.meters.master_correlation +=
                            (corr_raw - state.meters.master_correlation) * 0.05;
                    }
                    // Per-track stereo peak hold + clipping
                    if state.meters.track_peak_hold_l.len() != n {
                        state.meters.track_peak_hold_l.resize(n, 0.0);
                    }
                    if state.meters.track_peak_hold_r.len() != n {
                        state.meters.track_peak_hold_r.resize(n, 0.0);
                    }
                    if state.meters.track_clipping_l.len() != n {
                        state.meters.track_clipping_l.resize(n, false);
                    }
                    if state.meters.track_clipping_r.len() != n {
                        state.meters.track_clipping_r.resize(n, false);
                    }
                    for i in 0..n {
                        let tl = state.meters.track_rms_l.get(i).copied().unwrap_or(0.0);
                        let tr = state.meters.track_rms_r.get(i).copied().unwrap_or(0.0);
                        state.meters.track_peak_hold_l[i] =
                            if tl > state.meters.track_peak_hold_l[i] {
                                tl
                            } else {
                                (state.meters.track_peak_hold_l[i] - 0.003).max(0.0)
                            };
                        state.meters.track_peak_hold_r[i] =
                            if tr > state.meters.track_peak_hold_r[i] {
                                tr
                            } else {
                                (state.meters.track_peak_hold_r[i] - 0.003).max(0.0)
                            };
                        if tl >= 0.98 {
                            state.meters.track_clipping_l[i] = true;
                        }
                        if tr >= 0.98 {
                            state.meters.track_clipping_r[i] = true;
                        }
                    }
                }
            }
        }

        // Lazy-load waveform peaks for audio clips (one per frame)
        state.load_pending_waveforms();

        canvas.set_draw_color(theme::Theme::c(state.theme.bg_dark));
        canvas.clear();

        // Apply global UI scale via SDL2 canvas transform.
        // apply_scale() converts raw physical coords → logical coords ONCE per frame.
        let sc = state.ui_scale;
        let _ = canvas.set_scale(sc, sc);
        let (out_w, out_h) = canvas.output_size().unwrap_or((1280, 800));
        state.window_width = (out_w as f32 / sc) as u32;
        state.window_height = (out_h as f32 / sc) as u32;
        input.apply_scale(sc);

        // Propagate font scale to widget renderer
        crate::widgets::set_font_scale(state.font_scale);

        match state.mode {
            AppMode::ProjectManager => {
                views::draw_project_manager(&mut canvas, &mut input, &mut state)
            }
            AppMode::Arrangement => views::draw_arrangement(&mut canvas, &mut input, &mut state),
            // Mixer and Edit now live in the bottom panel — just show arrangement
            AppMode::Mixer | AppMode::Edit => {
                views::draw_arrangement(&mut canvas, &mut input, &mut state)
            }
        }

        // Clear module drag if mouse was released (and it wasn't consumed by a drop handler)
        if input.mouse_released && state.module_drag.is_some() {
            state.module_drag = None;
        }

        // Global note-off: when mouse is released, queue note-offs for any active
        // preview notes to prevent stuck notes if the mouse was released outside
        // the piano roll area. Notes already forwarded to audio get released;
        // notes still pending are just cleared.
        if input.mouse_released && !state.preview_notes.is_empty() {
            for &(_, pitch, _) in &state.preview_notes {
                state.piano_note_off_queue.push(pitch);
            }
            state.preview_notes.clear();
        }

        // Draw help screen overlay on top of everything
        if state.help_screen_visible {
            views::draw_help_screen(&mut canvas, &mut input, &mut state);
        }

        // Update window title with project name
        {
            let dirty_marker = if state.dirty { " *" } else { "" };
            let title = format!("Eden DAW — {}{}", state.project.name, dirty_marker);
            let _ = canvas.window_mut().set_title(&title);
        }

        // ── Config auto-save (debounced: save 120 frames (~2s) after last change) ──
        // Compare current state against frame-start snapshot
        {
            let cfg_now = CfgSnap {
                theme: state.theme.name.clone(),
                favs: state.favorite_folders.clone(),
                auto_return: state.auto_return,
                ui_scale: state.ui_scale.to_bits(),
                snap_enabled: state.snap.enabled,
                snap_res: state.snap.resolution_idx,
                browser_open: state.sample_browser_open,
                browser_w: state.sample_browser_width,
                bottom_open: state.bottom_panel_open,
                bottom_h: state.bottom_panel_height,
                vel_vis: state.velocity_editor_visible,
                win_w: state.window_width,
                win_h: state.window_height,
                left_tab: match state.left_panel_tab {
                    state::LeftPanelTab::Files => 0,
                    state::LeftPanelTab::Clips => 1,
                    state::LeftPanelTab::Instruments => 2,
                    state::LeftPanelTab::Themes => 3,
                },
                sample_auto: state.sample_auto_play,
                audio_dev: state.audio_device_idx,
                follow: state.follow_playhead,
            };
            if cfg_now != cfg_snapshot {
                state.config_dirty = true;
                state.config_save_countdown = 120;
            }
        }
        if state.config_dirty {
            if state.config_save_countdown > 0 {
                state.config_save_countdown -= 1;
            }
            if state.config_save_countdown == 0 {
                let cfg = config::UserConfig {
                    theme_name: state.theme.name.clone(),
                    favorite_folders: state.favorite_folders.clone(),
                    auto_return: state.auto_return,
                    ui_scale: state.ui_scale,
                    snap_enabled: state.snap.enabled,
                    snap_resolution_idx: state.snap.resolution_idx,
                    sample_browser_open: state.sample_browser_open,
                    sample_browser_width: state.sample_browser_width,
                    bottom_panel_open: state.bottom_panel_open,
                    bottom_panel_height: state.bottom_panel_height,
                    velocity_editor_visible: state.velocity_editor_visible,
                    window_width: state.window_width,
                    window_height: state.window_height,
                    left_panel_tab: match state.left_panel_tab {
                        state::LeftPanelTab::Files => 0,
                        state::LeftPanelTab::Clips => 1,
                        state::LeftPanelTab::Instruments => 2,
                        state::LeftPanelTab::Themes => 3,
                    },
                    sample_auto_play: state.sample_auto_play,
                    audio_device_idx: state.audio_device_idx,
                    recent_projects: state.recent_projects.clone(),
                    follow_playhead: state.follow_playhead,
                    autosave_enabled: state.autosave_enabled,
                    autosave_interval_idx: state.autosave_interval_idx,
                };
                if cfg.save().is_ok() {
                    // config saved silently
                }
                state.config_dirty = false;
            }
        }

        // ── Project autosave ──
        if state.autosave_enabled && state.dirty {
            if state.autosave_countdown > 0 {
                state.autosave_countdown -= 1;
            }
            if state.autosave_countdown == 0 {
                // Generate an autosave filename with a timestamp
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let base_name = state
                    .last_save_path
                    .as_ref()
                    .and_then(|p| {
                        std::path::Path::new(p)
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| state.project.name.clone());
                // Strip any previous "_autosave_NNNN" suffix
                let clean_name = if let Some(idx) = base_name.find("_autosave_") {
                    base_name[..idx].to_string()
                } else {
                    base_name
                };
                let save_dir = state
                    .last_save_path
                    .as_ref()
                    .and_then(|p| {
                        std::path::Path::new(p)
                            .parent()
                            .map(|d| d.to_string_lossy().to_string())
                    })
                    .unwrap_or_else(|| ".".to_string());
                let autosave_path = format!(
                    "{}/{}_autosave_{}.eden.json",
                    save_dir, clean_name, timestamp
                );
                match state.autosave_project(&autosave_path) {
                    Ok(()) => state.push_status(format!("Autosaved: {}", autosave_path)),
                    Err(e) => state.push_status(format!("Autosave failed: {}", e)),
                }
                // Reset countdown
                let (_, secs) = crate::config::AUTOSAVE_INTERVALS[state.autosave_interval_idx];
                state.autosave_countdown = secs * 60;
            }
        }

        canvas.present();

        // Call end_frame AFTER drawing so widgets can properly detect clicks
        input.end_frame();
    }

    if state.dirty {
        let exit_path = state.last_save_path.clone().unwrap_or_else(|| {
            let name = state.project.name.trim().to_string();
            let safe = if name.is_empty() {
                "untitled".to_string()
            } else {
                name
            };
            format!("{}.eden.json", safe)
        });
        let _ = state.save_project(&exit_path);
        println!("[session] Auto-saved on exit to {}", exit_path);
    }

    // Save user config on exit
    {
        let cfg = config::UserConfig {
            theme_name: state.theme.name.clone(),
            favorite_folders: state.favorite_folders.clone(),
            auto_return: state.auto_return,
            ui_scale: state.ui_scale,
            snap_enabled: state.snap.enabled,
            snap_resolution_idx: state.snap.resolution_idx,
            sample_browser_open: state.sample_browser_open,
            sample_browser_width: state.sample_browser_width,
            bottom_panel_open: state.bottom_panel_open,
            bottom_panel_height: state.bottom_panel_height,
            velocity_editor_visible: state.velocity_editor_visible,
            window_width: state.window_width,
            window_height: state.window_height,
            left_panel_tab: match state.left_panel_tab {
                state::LeftPanelTab::Files => 0,
                state::LeftPanelTab::Clips => 1,
                state::LeftPanelTab::Instruments => 2,
                state::LeftPanelTab::Themes => 3,
            },
            sample_auto_play: state.sample_auto_play,
            audio_device_idx: state.audio_device_idx,
            recent_projects: state.recent_projects.clone(),
            follow_playhead: state.follow_playhead,
            autosave_enabled: state.autosave_enabled,
            autosave_interval_idx: state.autosave_interval_idx,
        };
        match cfg.save() {
            Ok(()) => {} // saved silently on shutdown
            Err(e) => eprintln!("[config] Save error: {}", e),
        }
    }
}
