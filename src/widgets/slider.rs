// Eden DAW — Slider widget

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::{InputState, WidgetId};
use crate::theme::Theme;
use crate::widgets::inv_lerp;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliderOrientation {
    Horizontal,
    Vertical,
}

pub struct SliderParams {
    pub id: WidgetId,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub min: f32,
    pub max: f32,
    pub orientation: SliderOrientation,
    pub label: Option<String>,
    /// Value to reset to on double-click.
    pub default_value: Option<f32>,
}

impl Default for SliderParams {
    fn default() -> Self {
        Self {
            id: WidgetId::None,
            x: 0,
            y: 0,
            width: 120,
            height: 20,
            min: 0.0,
            max: 1.0,
            orientation: SliderOrientation::Horizontal,
            label: None,
            default_value: None,
        }
    }
}

pub fn slider(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    params: &SliderParams,
    value: &mut f32,
) -> bool {
    let hover = input.mouse_in_rect(params.x, params.y, params.width, params.height);
    let mut changed = false;

    if hover && input.mouse_pressed {
        if input.shift() {
            let dflt = params
                .default_value
                .unwrap_or(crate::widgets::lerp(params.min, params.max, 0.5));
            *value = dflt.clamp(params.min, params.max);
            input.consume();
            return true;
        }
        input.active_widget = params.id;
        input.drag_widget = params.id;
        input.drag_start_value = *value as f64;
        input.consume();
    }

    if (input.active_widget == params.id || input.drag_widget == params.id) && input.mouse_down {
        let sensitivity = 1.0
            / match params.orientation {
                SliderOrientation::Horizontal => params.width as f32,
                SliderOrientation::Vertical => params.height as f32,
            };
        let delta = match params.orientation {
            SliderOrientation::Horizontal => input.mouse_dx as f32 * sensitivity,
            SliderOrientation::Vertical => -input.mouse_dy as f32 * sensitivity,
        };
        let new_val = (*value + delta * (params.max - params.min)).clamp(params.min, params.max);
        if (new_val - *value).abs() > 1e-6 {
            *value = new_val;
            changed = true;
        }
    }

    // Middle mouse button — ultra-fine adjustment
    if hover && input.middle_mouse_down && input.middle_drag_widget == WidgetId::None {
        input.middle_drag_widget = params.id;
    }
    if input.middle_drag_widget == params.id && input.middle_mouse_down {
        let base_sensitivity = 1.0
            / match params.orientation {
                SliderOrientation::Horizontal => params.width as f32,
                SliderOrientation::Vertical => params.height as f32,
            };
        let fine_sens = base_sensitivity * 0.3;
        let delta = match params.orientation {
            SliderOrientation::Horizontal => input.mouse_dx as f32 * fine_sens,
            SliderOrientation::Vertical => -input.mouse_dy as f32 * fine_sens,
        };
        let new_val = (*value + delta * (params.max - params.min)).clamp(params.min, params.max);
        if (new_val - *value).abs() > 1e-6 {
            *value = new_val;
            changed = true;
        }
    }

    if hover {
        input.hot_widget = params.id;
    }

    let is_active = input.active_widget == params.id;
    let is_hot = hover;

    // Background
    canvas.set_draw_color(if is_active {
        Theme::c(theme.bg_light)
    } else {
        Theme::c(theme.slider_bg)
    });
    let _ = canvas.fill_rect(Rect::new(
        params.x,
        params.y,
        params.width as u32,
        params.height as u32,
    ));

    // Fill
    let t = inv_lerp(params.min, params.max, *value);
    canvas.set_draw_color(Theme::c(theme.slider_fill));

    match params.orientation {
        SliderOrientation::Horizontal => {
            let fill_w = (t * params.width as f32) as u32;
            let _ = canvas.fill_rect(Rect::new(params.x, params.y, fill_w, params.height as u32));
            let thumb_x = (params.x + (t * params.width as f32) as i32 - 2)
                .clamp(params.x, params.x + params.width - 4);
            canvas.set_draw_color(if is_active || is_hot {
                Theme::c(theme.accent)
            } else {
                Theme::c(theme.slider_thumb)
            });
            let _ = canvas.fill_rect(Rect::new(
                thumb_x,
                params.y + 1,
                4,
                (params.height - 2).max(1) as u32,
            ));
        }
        SliderOrientation::Vertical => {
            let fill_h = (t * params.height as f32) as u32;
            let fill_y = params.y + params.height - fill_h as i32;
            let _ = canvas.fill_rect(Rect::new(params.x, fill_y, params.width as u32, fill_h));
            let thumb_y = (fill_y - 2).clamp(params.y, params.y + params.height - 4);
            canvas.set_draw_color(if is_active || is_hot {
                Theme::c(theme.accent)
            } else {
                Theme::c(theme.slider_thumb)
            });
            let _ = canvas.fill_rect(Rect::new(
                params.x + 1,
                thumb_y,
                (params.width - 2).max(1) as u32,
                4,
            ));
        }
    }

    // Border
    canvas.set_draw_color(if is_hot {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.panel_border)
    });
    let _ = canvas.draw_rect(Rect::new(
        params.x,
        params.y,
        params.width as u32,
        params.height as u32,
    ));

    changed
}
