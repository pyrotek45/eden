// Eden DAW — Toggle button (Mute / Solo style)

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::theme::Theme;
use crate::widgets::draw_pixel_label;

#[allow(clippy::too_many_arguments)]
pub fn toggle_button(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    x: i32,
    y: i32,
    size: i32,
    on_color: [u8; 4],
    toggled: bool,
    id: WidgetId,
    label: &str,
    hint: Option<&str>,
) -> bool {
    let hover = input.mouse_in_rect(x, y, size, size);
    let mut clicked = false;
    if hover && input.mouse_pressed && !input.consumed {
        input.active_widget = id;
        input.consume();
    }
    if hover && input.mouse_released && input.active_widget == id {
        clicked = true;
    }
    if hover {
        input.hot_widget = id;
        if let Some(h) = hint {
            input.hover_hint_text = Some(h.to_string());
        }
    }

    let bg = if toggled {
        Theme::c(on_color)
    } else if hover {
        Theme::c(theme.button_hover)
    } else {
        Theme::c(theme.button_bg)
    };

    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, size as u32, size as u32));
    canvas.set_draw_color(Theme::c(theme.panel_border));
    let _ = canvas.draw_rect(Rect::new(x, y, size as u32, size as u32));

    if !label.is_empty() {
        let fg = if toggled {
            sdl2::pixels::Color::RGBA(10, 10, 10, 255)
        } else {
            Theme::c(theme.button_text)
        };
        let lw = (label.len() as i32 * 9).min(size - 4);
        let lx = x + (size - lw) / 2;
        let ly = y + (size - 10) / 2;
        draw_pixel_label(canvas, theme, label, lx, ly, lw + 2, fg);
    }

    clicked
}
