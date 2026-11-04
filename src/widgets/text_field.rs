// Eden DAW — Text field widget

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::InputState;
use crate::theme::Theme;
use crate::widgets::{draw_pixel_label_scaled, get_font_scale};

fn previous_boundary(text: &str, cursor: usize) -> usize {
    text.char_indices()
        .map(|(i, _)| i)
        .take_while(|i| *i < cursor)
        .last()
        .unwrap_or(0)
}
fn next_boundary(text: &str, cursor: usize) -> usize {
    text.char_indices()
        .map(|(i, _)| i)
        .find(|i| *i > cursor)
        .unwrap_or(text.len())
}

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
        *cursor = (*cursor).min(buffer.len());
        while !buffer.is_char_boundary(*cursor) {
            *cursor -= 1;
        }

        for ch in &input.text_input_chars {
            if !ch.is_control() && *cursor <= buffer.len() {
                buffer.insert(*cursor, *ch);
                *cursor += ch.len_utf8();
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
                        *cursor = previous_boundary(buffer, *cursor);
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
                        *cursor = previous_boundary(buffer, *cursor);
                    }
                }
                Keycode::Right => {
                    if *cursor < buffer.len() {
                        *cursor = next_boundary(buffer, *cursor);
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

    let bg = Theme::c(if is_active || hover {
        theme.bg_light
    } else {
        theme.bg_medium
    });
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    let border = if is_active {
        let a = theme.accent;
        sdl2::pixels::Color::RGBA(a[0], a[1], a[2], 220)
    } else {
        Theme::c(theme.panel_border)
    };
    canvas.set_draw_color(border);
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    let scale = get_font_scale().min(((h - 4) / 5).max(1));
    let char_w = 4 * scale + scale / 2;
    let text_x = x + 4;
    let text_y = y + (h - 5 * scale) / 2;
    let text_w = (w - 8).max(0);
    let display_text = if is_active { buffer.as_str() } else { value };
    let caret_chars = if is_active {
        buffer[..*cursor].chars().count()
    } else {
        0
    };
    let visible_chars = (text_w / char_w).max(1) as usize;
    let scroll_chars = caret_chars.saturating_sub(visible_chars.saturating_sub(1));
    let scroll_byte = display_text
        .char_indices()
        .nth(scroll_chars)
        .map(|(i, _)| i)
        .unwrap_or(display_text.len());
    let shown = &display_text[scroll_byte..];
    if shown.is_empty() && !is_active {
        if let Some(hint) = &params.hint {
            draw_pixel_label_scaled(
                canvas,
                theme,
                hint,
                text_x,
                text_y,
                text_w,
                Theme::c(theme.text_secondary),
                scale,
            );
        }
    } else {
        draw_pixel_label_scaled(
            canvas,
            theme,
            shown,
            text_x,
            text_y,
            text_w,
            Theme::c(theme.text_primary),
            scale,
        );
    }
    if is_active {
        let cursor_x =
            (text_x + (caret_chars - scroll_chars) as i32 * char_w).min(text_x + text_w - 1);
        canvas.set_draw_color(Theme::c(theme.accent));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_moves_and_backspaces_whole_utf8_characters() {
        let mut text = String::from("aé🎹");
        let mut cursor = text.len();
        cursor = previous_boundary(&text, cursor);
        assert_eq!(cursor, 3);
        text.remove(cursor);
        assert_eq!(text, "aé");
        cursor = previous_boundary(&text, cursor);
        assert_eq!(cursor, 1);
        assert_eq!(next_boundary(&text, cursor), 3);
        text.remove(cursor);
        assert_eq!(text, "a");
    }
}
