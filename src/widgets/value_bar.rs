// Eden DAW — Value bar display

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::theme::Theme;

pub fn value_bar(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    t: f32,
) {
    canvas.set_draw_color(Theme::c(theme.slider_bg));
    let _ = canvas.fill_rect(Rect::new(x, y, width as u32, height as u32));
    let fill_w = (t.clamp(0.0, 1.0) * width as f32) as u32;
    canvas.set_draw_color(Theme::c(theme.slider_fill));
    let _ = canvas.fill_rect(Rect::new(x, y, fill_w, height as u32));
}
