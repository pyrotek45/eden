// Eden DAW — Track type icon

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

pub fn draw_track_type_icon(
    canvas: &mut Canvas<Window>,
    track_type: &crate::app::models::TrackType,
    x: i32,
    y: i32,
    size: i32,
    color: sdl2::pixels::Color,
) {
    match track_type {
        crate::app::models::TrackType::Midi => {
            let kw = (size / 4).max(2);
            let kh = size - 1;
            let bh = kh * 6 / 10;
            let bw = (kw * 6 / 10).max(1);
            canvas.set_draw_color(color);
            for k in 0..4 {
                let kx = x + k * kw;
                let _ = canvas.draw_rect(Rect::new(kx, y, kw as u32, kh as u32));
            }
            let dark = sdl2::pixels::Color::RGBA(color.r / 4, color.g / 4, color.b / 4, color.a);
            canvas.set_draw_color(dark);
            for k in 0..3 {
                let bx = x + k * kw + kw - bw / 2;
                let _ = canvas.fill_rect(Rect::new(bx, y, bw as u32, bh as u32));
            }
        }
        crate::app::models::TrackType::Audio => {
            canvas.set_draw_color(color);
            let mid = y + size / 2;
            let amp = size / 2 - 1;
            let steps = size / 2;
            for i in 0..steps {
                let t = i as f32 / steps as f32;
                let wave_y =
                    (mid as f32 - (t * std::f32::consts::PI * 2.0).sin() * amp as f32) as i32;
                let sx = x + i * 2;
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(sx, mid),
                    sdl2::rect::Point::new(sx, wave_y),
                );
            }
        }
        crate::app::models::TrackType::Automation => {
            canvas.set_draw_color(color);
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(x, y + size - 2),
                sdl2::rect::Point::new(x + size - 2, y + 1),
            );
            for i in 0..3 {
                let t = i as f32 / 2.0;
                let dx = x + (t * (size - 2) as f32) as i32;
                let dy = y + ((1.0 - t) * (size - 2) as f32) as i32;
                let _ = canvas.fill_rect(Rect::new(dx - 1, dy - 1, 3, 3));
            }
        }
    }
}
