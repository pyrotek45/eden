// Eden DAW — Number spinner widget

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::theme::Theme;
use crate::widgets::draw_pixel_label;

#[allow(clippy::too_many_arguments)]
pub fn number_spinner(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: WidgetId,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    min: f64,
    max: f64,
    step: f64,
    decimals: usize,
    value: &mut f64,
) -> bool {
    let hover = input.mouse_in_rect(x, y, w, h);
    let mut changed = false;

    if hover && input.scroll_y != 0 && !input.scroll_consumed {
        *value = (*value + step * input.scroll_y as f64).clamp(min, max);
        input.scroll_consumed = true;
        changed = true;
    }

    if hover && input.mouse_pressed {
        if input.shift() {
            *value = (min + max) / 2.0;
            return true;
        }
        input.active_widget = id;
        input.drag_widget = id;
        input.drag_start_value = *value;
    }
    if input.active_widget == id && input.mouse_down {
        let delta = -input.mouse_dy as f64 * step * 0.5;
        *value = (*value + delta).clamp(min, max);
        if delta.abs() > 0.0 {
            changed = true;
        }
    }
    if hover {
        input.hot_widget = id;
    }

    let is_active = input.active_widget == id;
    let is_hot = hover;

    canvas.set_draw_color(if is_active {
        Theme::c(theme.bg_light)
    } else if is_hot {
        Theme::c(theme.button_hover)
    } else {
        Theme::c(theme.button_bg)
    });
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));
    canvas.set_draw_color(if is_active {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.panel_border)
    });
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    let text = if decimals == 0 {
        format!("{:.0}", *value)
    } else {
        format!("{:.prec$}", *value, prec = decimals)
    };
    let col = if is_active {
        Theme::c(theme.accent)
    } else {
        sdl2::pixels::Color::RGBA(220, 220, 220, 255)
    };
    draw_pixel_label(canvas, theme, &text, x + 4, y + (h - 10) / 2, w - 8, col);

    // Up/down arrows
    canvas.set_draw_color(Theme::c(theme.text_dim));
    let ax = x + w - 8;
    let mid_y_up = y + h / 4;
    let _ = canvas.fill_rect(Rect::new(ax, mid_y_up + 1, 5, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 1, mid_y_up - 1, 3, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 2, mid_y_up - 2, 1, 1));
    let mid_y_dn = y + h - h / 4;
    let _ = canvas.fill_rect(Rect::new(ax, mid_y_dn - 2, 5, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 1, mid_y_dn, 3, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 2, mid_y_dn + 2, 1, 1));

    changed
}
