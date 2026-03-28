// Eden DAW — Compressor Curve widget

use sdl2::gfx::primitives::DrawRenderer;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::theme::Theme;
use crate::widgets::draw_pixel_label;

/// Draws a tiny compressor knee/curve + gain reduction bar.
#[allow(clippy::too_many_arguments)]
pub fn comp_curve_widget(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    compress: f32,
    gr_db: f32,
    input_rms: f32,
) {
    if w < 10 || h < 10 {
        return;
    }

    // Background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 20, 26, 220));
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    let curve_h = (h * 2 / 3).max(6);
    let bar_h = h - curve_h - 2;

    let cx0 = x + 1;
    let cy0 = y + curve_h;
    let cw = w - 2;
    let ch = curve_h - 1;

    // 1:1 reference diagonal
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 100));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(cx0, cy0),
        sdl2::rect::Point::new(cx0 + cw, cy0 - ch),
    );

    // Compressed curve
    let ratio = 1.0 + compress * 7.0;
    let thresh_frac = 1.0 - compress * 0.5;
    let curve_col = sdl2::pixels::Color::RGBA(200, 140, 60, 220);
    canvas.set_draw_color(curve_col);
    let mut prev_px = cx0;
    let mut prev_py = cy0;
    for px_i in 0..=cw {
        let in_frac = px_i as f32 / cw as f32;
        let out_frac = if in_frac < thresh_frac {
            in_frac
        } else {
            thresh_frac + (in_frac - thresh_frac) / ratio
        };
        let px = cx0 + px_i;
        let py = cy0 - (out_frac * ch as f32) as i32;
        if px_i > 0 {
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(prev_px, prev_py),
                sdl2::rect::Point::new(px, py),
            );
        }
        prev_px = px;
        prev_py = py;
    }

    // ── Riding dot ──
    if input_rms > 1e-6 {
        let in_db = 20.0 * input_rms.log10();
        let in_frac = ((in_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let out_frac = if in_frac < thresh_frac {
            in_frac
        } else {
            thresh_frac + (in_frac - thresh_frac) / ratio
        };
        let dot_px = cx0 + (in_frac * cw as f32) as i32;
        let dot_py = cy0 - (out_frac * ch as f32) as i32;
        let _ = canvas.filled_circle(
            dot_px as i16,
            dot_py as i16,
            3,
            sdl2::pixels::Color::RGBA(255, 200, 80, 255),
        );
        let _ = canvas.circle(
            dot_px as i16,
            dot_py as i16,
            3,
            sdl2::pixels::Color::RGBA(255, 160, 40, 180),
        );
    }

    // ── GR bar ──
    if bar_h > 2 {
        let bar_y = y + curve_h + 1;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 32, 38, 200));
        let _ = canvas.fill_rect(Rect::new(x, bar_y, w as u32, bar_h as u32));

        let gr_frac = (gr_db.abs() / 20.0).clamp(0.0, 1.0);
        if gr_frac > 0.001 {
            let fill_w = (gr_frac * (w - 2) as f32) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 100, 40, 200));
            let _ = canvas.fill_rect(Rect::new(x + 1, bar_y, fill_w as u32, bar_h as u32));
        }

        draw_pixel_label(
            canvas,
            theme,
            "GR",
            x + 2,
            bar_y,
            14,
            sdl2::pixels::Color::RGBA(200, 140, 60, 140),
        );

        if gr_db.abs() > 0.1 {
            let gr_str = format!("{:.1}", gr_db);
            draw_pixel_label(
                canvas,
                theme,
                &gr_str,
                x + w - 28,
                bar_y,
                26,
                sdl2::pixels::Color::RGBA(200, 160, 80, 180),
            );
        }
    }

    // Border
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 80));
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));
}
