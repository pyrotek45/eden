// Eden DAW — Views: project_manager

use sdl2::render::Canvas;
use sdl2::video::Window;

use super::overlays::{draw_file_browser_popup, draw_new_project_popup};
use crate::app::input::{InputState, WidgetId};
use crate::app::state::*;
use crate::theme::Theme;
use crate::widgets::*;

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
            state.mode = crate::app::state::AppMode::Arrangement;
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
                        state.mode = crate::app::state::AppMode::Arrangement;
                    }
                    Err(e) => {
                        state.push_status(format!("Failed to load: {}", e));
                        state.mode = crate::app::state::AppMode::Arrangement;
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
                crate::app::state::FileBrowserCaller::OpenProject,
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
                        state.mode = crate::app::state::AppMode::Arrangement;
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
