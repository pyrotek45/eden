// Eden DAW — Views: help

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::InputState;
use crate::app::state::*;
use crate::widgets::*;

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
        "Eden DAW  —  Help  (F1 / Esc to close)",
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
            ("H", "Toggle this help screen (alternate)", false),
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
