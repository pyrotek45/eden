// Eden DAW — Button widget

use sdl2::gfx::primitives::DrawRenderer;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::{InputState, WidgetId};
use crate::theme::Theme;
use crate::widgets::{draw_pixel_label, get_font_scale};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonIcon {
    None,
    Play,
    Stop,
    Record,
    Loop,
    Rewind,
    AutoReturn,
}

pub struct ButtonParams {
    pub id: WidgetId,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub label: String,
    pub toggled: bool,
    pub icon: ButtonIcon,
    /// Hover tooltip text
    pub hint: Option<String>,
    /// Allow a RowLayout to resize this button's width to fill available space
    pub can_resize: bool,
    /// Minimum width when can_resize is true (ignored otherwise)
    pub min_width: i32,
}

impl Default for ButtonParams {
    fn default() -> Self {
        Self {
            id: WidgetId::None,
            x: 0,
            y: 0,
            width: 60,
            height: 24,
            label: String::new(),
            toggled: false,
            icon: ButtonIcon::None,
            hint: None,
            can_resize: false,
            min_width: 20,
        }
    }
}

pub fn button(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    params: &ButtonParams,
) -> bool {
    let hover = input.mouse_in_rect(params.x, params.y, params.width, params.height);
    let mut clicked = false;

    if hover && input.mouse_pressed && !input.consumed {
        input.active_widget = params.id;
        input.consume();
    }
    if hover && input.mouse_released && input.active_widget == params.id {
        clicked = true;
    }
    if hover {
        input.hot_widget = params.id;
        if let Some(ref hint) = params.hint {
            input.hover_hint_text = Some(hint.clone());
            input.hover_hint_widget = params.id;
        }
    }

    let is_active = input.active_widget == params.id && input.mouse_down && hover;
    let is_hot = hover;

    let bg = if params.toggled {
        Theme::c(theme.accent_active)
    } else if is_active {
        Theme::c(theme.button_active)
    } else if is_hot {
        Theme::c(theme.button_hover)
    } else {
        Theme::c(theme.button_bg)
    };

    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(
        params.x,
        params.y,
        params.width as u32,
        params.height as u32,
    ));

    let border = if params.toggled || is_active {
        Theme::c(theme.accent)
    } else if is_hot {
        Theme::c(theme.accent_hover)
    } else {
        Theme::c(theme.panel_border)
    };
    canvas.set_draw_color(border);
    let _ = canvas.draw_rect(Rect::new(
        params.x,
        params.y,
        params.width as u32,
        params.height as u32,
    ));

    let fg = if params.toggled {
        Theme::c(theme.bg_dark)
    } else {
        Theme::c(theme.button_text)
    };
    let icx = params.x + params.width / 2;
    let icy = params.y + params.height / 2;

    match params.icon {
        ButtonIcon::Play => {
            let c = if params.toggled {
                Theme::c(theme.bg_dark)
            } else {
                Theme::c(theme.play_color)
            };
            let sz = (params.height as f32 * 0.3) as i16;
            let x1 = (icx - sz as i32 / 2) as i16;
            let _ = canvas.filled_trigon(
                x1,
                (icy - sz as i32) as i16,
                (icx + sz as i32) as i16,
                icy as i16,
                x1,
                (icy + sz as i32) as i16,
                c,
            );
        }
        ButtonIcon::Stop => {
            let c = if params.toggled {
                Theme::c(theme.bg_dark)
            } else {
                Theme::c(theme.stop_color)
            };
            let sz = (params.height as f32 * 0.25) as i32;
            canvas.set_draw_color(c);
            let _ = canvas.fill_rect(Rect::new(
                icx - sz,
                icy - sz,
                (sz * 2) as u32,
                (sz * 2) as u32,
            ));
        }
        ButtonIcon::Record => {
            let c = if params.toggled {
                sdl2::pixels::Color::RGBA(255, 80, 80, 255)
            } else {
                Theme::c(theme.record_color)
            };
            let sz = (params.height as f32 * 0.28) as i16;
            let _ = canvas.filled_circle(icx as i16, icy as i16, sz, c);
        }
        ButtonIcon::Loop => {
            let c = if params.toggled {
                Theme::c(theme.bg_dark)
            } else {
                Theme::c(theme.loop_color)
            };
            let sz = (params.height as f32 * 0.26) as i32;
            let hw = sz;
            let hh = (sz as f32 * 0.6) as i32;
            canvas.set_draw_color(c);
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw + 3, icy - hh),
                sdl2::rect::Point::new(icx + hw - 2, icy - hh),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw + 3, icy - hh + 1),
                sdl2::rect::Point::new(icx + hw - 2, icy - hh + 1),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw, icy - hh + 2),
                sdl2::rect::Point::new(icx + hw, icy),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw - 1, icy - hh + 2),
                sdl2::rect::Point::new(icx + hw - 1, icy),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw - 3, icy + hh),
                sdl2::rect::Point::new(icx - hw + 2, icy + hh),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw - 3, icy + hh - 1),
                sdl2::rect::Point::new(icx - hw + 2, icy + hh - 1),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw, icy + hh - 2),
                sdl2::rect::Point::new(icx - hw, icy),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw - 1, icy + hh - 2),
                sdl2::rect::Point::new(icx - hw - 1, icy),
            );
            let ax = icx + hw - 1;
            let ay = icy - hh;
            let _ = canvas.filled_trigon(
                ax as i16,
                (ay - 3) as i16,
                (ax + 4) as i16,
                ay as i16,
                ax as i16,
                (ay + 3) as i16,
                c,
            );
            let bx = icx - hw + 1;
            let by = icy + hh;
            let _ = canvas.filled_trigon(
                bx as i16,
                (by - 3) as i16,
                (bx - 4) as i16,
                by as i16,
                bx as i16,
                (by + 3) as i16,
                c,
            );
        }
        ButtonIcon::Rewind => {
            let c = fg;
            let sz = (params.height as f32 * 0.25) as i16;
            let _ = canvas.filled_trigon(
                (icx + 2) as i16,
                (icy - sz as i32) as i16,
                (icx - sz as i32 + 2) as i16,
                icy as i16,
                (icx + 2) as i16,
                (icy + sz as i32) as i16,
                c,
            );
            canvas.set_draw_color(c);
            let _ = canvas.fill_rect(Rect::new(
                icx - sz as i32 - 1,
                icy - sz as i32,
                2,
                (sz * 2) as u32,
            ));
        }
        ButtonIcon::AutoReturn => {
            let c = fg;
            let sz = (params.height as f32 * 0.25) as i16;
            let _ = canvas.filled_trigon(
                icx as i16,
                (icy - sz as i32 + 2) as i16,
                (icx - sz as i32) as i16,
                icy as i16,
                icx as i16,
                (icy + sz as i32 - 2) as i16,
                c,
            );
            canvas.set_draw_color(c);
            let _ = canvas.fill_rect(Rect::new(
                icx - sz as i32,
                icy + sz as i32 - 1,
                (sz * 2) as u32,
                2,
            ));
            let _ = canvas.fill_rect(Rect::new(
                icx - sz as i32 - 2,
                icy - sz as i32,
                2,
                (sz * 2) as u32,
            ));
        }
        ButtonIcon::None => {
            if !params.label.is_empty() {
                let scale = get_font_scale();
                let glyph_w = 4 * scale + 1;
                let glyph_h = 5 * scale;
                let tw = (params.label.len() as i32 * glyph_w).min(params.width - 4);
                let tx = params.x + (params.width - tw) / 2;
                let ty = params.y + (params.height - glyph_h) / 2;
                draw_pixel_label(canvas, theme, &params.label, tx, ty, tw + 2, fg);
            }
        }
    }

    clicked
}
