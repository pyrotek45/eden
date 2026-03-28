// Eden DAW — Text field widget

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::InputState;
use crate::theme::Theme;
use crate::widgets::draw_pixel_label;

pub struct TextFieldParams {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub hint: Option<String>,
}

/// Returns (committed: bool, new_value: Option<String>).
#[allow(clippy::too_many_arguments)]
pub fn text_field(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    params: &TextFieldParams,
    value: &str,
    active_id: &mut u32,
    buffer: &mut String,
    cursor: &mut usize,
) -> (bool, Option<String>) {
    let x = params.x;
    let y = params.y;
    let w = params.width;
    let h = params.height;
    let id = params.id;

    let is_active = *active_id == id && id != 0;
    let hover = input.mouse_in_rect(x, y, w, h);

    if hover && input.mouse_pressed && !is_active {
        *active_id = id;
        *buffer = value.to_string();
        *cursor = buffer.len();
    }

    if is_active && input.mouse_pressed && !hover {
        let result = buffer.clone();
        *active_id = 0;
        return (true, Some(result));
    }

    let mut committed = false;
    let mut result_value: Option<String> = None;

    if is_active {
        use sdl2::keyboard::Keycode;

        for ch in &input.text_input_chars {
            if !ch.is_control() && *cursor <= buffer.len() {
                buffer.insert(*cursor, *ch);
                *cursor += 1;
            }
        }

        for key in &input.keys_pressed {
            match *key {
                Keycode::Return | Keycode::KpEnter => {
                    committed = true;
                    result_value = Some(buffer.clone());
                    *active_id = 0;
                }
                Keycode::Escape => {
                    *active_id = 0;
                }
                Keycode::Backspace => {
                    if *cursor > 0 {
                        *cursor -= 1;
                        buffer.remove(*cursor);
                    }
                }
                Keycode::Delete => {
                    if *cursor < buffer.len() {
                        buffer.remove(*cursor);
                    }
                }
                Keycode::Left => {
                    if *cursor > 0 {
                        *cursor -= 1;
                    }
                }
                Keycode::Right => {
                    if *cursor < buffer.len() {
                        *cursor += 1;
                    }
                }
                Keycode::Home => {
                    *cursor = 0;
                }
                Keycode::End => {
                    *cursor = buffer.len();
                }
                _ => {}
            }
        }

        input.keys_pressed.clear();
        input.text_input_chars.clear();
    }

    // ── Draw ──

    let bg = if is_active {
        sdl2::pixels::Color::RGBA(40, 42, 50, 255)
    } else if hover {
        sdl2::pixels::Color::RGBA(55, 58, 68, 255)
    } else {
        sdl2::pixels::Color::RGBA(45, 48, 56, 255)
    };
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    let border = if is_active {
        let a = theme.accent;
        sdl2::pixels::Color::RGBA(a[0], a[1], a[2], 220)
    } else {
        sdl2::pixels::Color::RGBA(70, 75, 85, 200)
    };
    canvas.set_draw_color(border);
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    let display_text = if is_active { buffer.as_str() } else { value };
    let text_x = x + 4;
    let text_y = y + (h - 10) / 2;
    let text_w = w - 8;

    if display_text.is_empty() && !is_active {
        if let Some(ref hint) = params.hint {
            draw_pixel_label(
                canvas,
                theme,
                hint,
                text_x,
                text_y,
                text_w,
                sdl2::pixels::Color::RGBA(100, 105, 120, 150),
            );
        }
    } else {
        let col = if is_active {
            Theme::c(theme.text_primary)
        } else {
            sdl2::pixels::Color::RGBA(190, 195, 210, 230)
        };
        draw_pixel_label(canvas, theme, display_text, text_x, text_y, text_w, col);
    }

    if is_active {
        let char_w = 9i32;
        let visible_chars = (text_w / char_w).max(1) as usize;
        let scroll_start = if *cursor > visible_chars {
            *cursor - visible_chars
        } else {
            0
        };
        if scroll_start > 0 {
            canvas.set_draw_color(if is_active {
                sdl2::pixels::Color::RGBA(40, 42, 50, 255)
            } else {
                sdl2::pixels::Color::RGBA(45, 48, 56, 255)
            });
            let _ = canvas.fill_rect(Rect::new(text_x, text_y, text_w as u32, 10));
            let scrolled = &buffer[scroll_start.min(buffer.len())..];
            let col = Theme::c(theme.text_primary);
            draw_pixel_label(canvas, theme, scrolled, text_x, text_y, text_w, col);
        }
        let cursor_x = text_x + ((*cursor - scroll_start) as i32) * char_w;
        let cursor_x = cursor_x.min(text_x + text_w - 1);
        let a = theme.accent;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(a[0], a[1], a[2], 255));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(cursor_x, y + 2),
            sdl2::rect::Point::new(cursor_x, y + h - 2),
        );
    }

    if hover {
        if let Some(ref hint) = params.hint {
            if !is_active {
                input.hover_hint_text = Some(hint.clone());
            }
        }
    }

    (committed, result_value)
}
