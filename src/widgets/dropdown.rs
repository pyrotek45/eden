// Eden DAW — Dropdown widget

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::InputState;
use crate::theme::Theme;
use crate::widgets::draw_pixel_label;

#[allow(clippy::too_many_arguments)]
pub fn dropdown(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    options: &[&str],
    selected: &mut usize,
    open_id: &mut u32,
) -> bool {
    if options.is_empty() {
        return false;
    }
    let mut changed = false;
    let is_open = *open_id == id;
    let hover = input.mouse_in_rect(x, y, w, h);

    if hover && input.scroll_y != 0 && !is_open && !input.scroll_consumed {
        let n = options.len();
        if input.scroll_y > 0 {
            *selected = selected.saturating_sub(1);
        } else {
            *selected = (*selected + 1).min(n - 1);
        }
        input.scroll_consumed = true;
        changed = true;
    }

    if hover && input.mouse_pressed && !input.consumed {
        if is_open {
            *open_id = 0;
        } else {
            *open_id = id;
        }
        input.mouse_pressed = false;
    }

    let bg = if is_open || hover {
        Theme::c(theme.button_hover)
    } else {
        Theme::c(theme.button_bg)
    };
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));
    canvas.set_draw_color(if is_open {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.panel_border)
    });
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    let label = options[*selected];
    draw_pixel_label(
        canvas,
        theme,
        label,
        x + 4,
        y + (h - 10) / 2,
        w - 18,
        sdl2::pixels::Color::RGBA(220, 220, 220, 255),
    );

    // Arrow chevron
    let ax = x + w - 10;
    let ay = y + h / 2;
    canvas.set_draw_color(Theme::c(theme.text_secondary));
    let _ = canvas.fill_rect(Rect::new(ax, ay - 1, 7, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 1, ay + 1, 5, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 2, ay + 3, 3, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 3, ay + 5, 1, 2));

    if is_open {
        let item_h = h;
        let popup_h = options.len() as i32 * item_h;
        let popup_y = y + h;

        let over_dropdown = input.mouse_in_rect(x, y, w, h + popup_h);
        if over_dropdown && (input.mouse_pressed || input.mouse_down) {
            input.consumed = true;
        }

        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(x + 2, popup_y + 2, w as u32, popup_h as u32));

        canvas.set_draw_color(Theme::c(theme.panel_bg));
        let _ = canvas.fill_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));
        canvas.set_draw_color(Theme::c(theme.accent));
        let _ = canvas.draw_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));

        for (i, opt) in options.iter().enumerate() {
            let iy = popup_y + i as i32 * item_h;
            let item_hover = input.mouse_in_rect(x, iy, w, item_h);

            if item_hover {
                canvas.set_draw_color(Theme::c(theme.button_hover));
                let _ = canvas.fill_rect(Rect::new(x, iy, w as u32, item_h as u32));
            }
            if i == *selected {
                canvas.set_draw_color(Theme::c(theme.accent_active));
                let _ = canvas.fill_rect(Rect::new(x, iy, 3, item_h as u32));
            }

            draw_pixel_label(
                canvas,
                theme,
                opt,
                x + 6,
                iy + (item_h - 10) / 2,
                w - 10,
                if i == *selected {
                    Theme::c(theme.accent)
                } else {
                    sdl2::pixels::Color::RGBA(210, 210, 210, 255)
                },
            );

            if item_hover && input.mouse_pressed {
                *selected = i;
                *open_id = 0;
                changed = true;
                input.mouse_pressed = false;
            }
        }

        let outside = !input.mouse_in_rect(x, y, w, h + popup_h);
        if input.mouse_pressed && outside {
            *open_id = 0;
            input.mouse_pressed = false;
        }
    }

    changed
}

/// Redraw just the popup overlay for an open dropdown (renders on top of everything).
#[allow(clippy::too_many_arguments)]
pub fn dropdown_popup_overlay(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    id: u32,
    x: i32,
    y: i32,
    _w_closed: i32,
    h: i32,
    w: i32,
    options: &[&str],
    selected: usize,
    open_id: u32,
    mouse_x: i32,
    mouse_y: i32,
) {
    if open_id != id || options.is_empty() {
        return;
    }
    canvas.set_clip_rect(None);
    let item_h = h;
    let popup_h = options.len() as i32 * item_h;
    let popup_y = y + h;

    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
    let _ = canvas.fill_rect(Rect::new(x + 2, popup_y + 2, w as u32, popup_h as u32));

    canvas.set_draw_color(Theme::c(theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));
    canvas.set_draw_color(Theme::c(theme.accent));
    let _ = canvas.draw_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));

    for (i, opt) in options.iter().enumerate() {
        let iy = popup_y + i as i32 * item_h;
        let item_hover = mouse_x >= x && mouse_x < x + w && mouse_y >= iy && mouse_y < iy + item_h;

        if item_hover {
            canvas.set_draw_color(Theme::c(theme.button_hover));
            let _ = canvas.fill_rect(Rect::new(x, iy, w as u32, item_h as u32));
        }
        if i == selected {
            canvas.set_draw_color(Theme::c(theme.accent_active));
            let _ = canvas.fill_rect(Rect::new(x, iy, 3, item_h as u32));
        }

        draw_pixel_label(
            canvas,
            theme,
            opt,
            x + 6,
            iy + (item_h - 10) / 2,
            w - 10,
            if i == selected {
                Theme::c(theme.accent)
            } else {
                sdl2::pixels::Color::RGBA(210, 210, 210, 255)
            },
        );
    }
}
