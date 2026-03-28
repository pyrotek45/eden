// Eden DAW — VU Meter widget

use sdl2::gfx::primitives::DrawRenderer;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::theme::Theme;
use crate::widgets::draw_pixel_label_scaled;

/// Draws an analog-style VU meter gauge.
#[allow(clippy::too_many_arguments)]
pub fn vu_meter(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    needle_pos: f32,
    peak_pos: f32,
) {
    if w < 30 || h < 20 {
        return;
    }

    let pad = 3;
    let inner_w = w - pad * 2;
    let inner_h = h - pad * 2;
    let inner_x = x + pad;
    let inner_y = y + pad;

    // Background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(26, 26, 30, 245));
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    // ── Geometry: wide shallow arc ──
    let arc_inset = 20i32;
    let arc_w = (inner_w - arc_inset * 2).max(20);
    let half_chord = arc_w as f64 / 2.0;
    let sagitta = (inner_h as f64 * 0.35).max(8.0);
    let arc_r = (half_chord * half_chord + sagitta * sagitta) / (2.0 * sagitta);

    let cx = (inner_x + inner_w / 2) as f64;
    let cy = (inner_y as f64) + sagitta + (arc_r - sagitta);

    let half_angle_rad = (half_chord / arc_r).asin();
    let half_angle_deg = half_angle_rad.to_degrees();
    let start_deg = 270.0 - half_angle_deg;
    let end_deg = 270.0 + half_angle_deg;
    let sweep = end_deg - start_deg;

    let label_zone_h = 9i32;
    let arc_cy = cy + label_zone_h as f64;
    let arc_r_i16 = arc_r as i16;
    let cx_i16 = cx as i16;
    let cy_i16 = arc_cy as i16;

    // Draw arc tracks
    let _ = canvas.arc(
        cx_i16,
        cy_i16,
        arc_r_i16,
        start_deg as i16,
        end_deg as i16,
        sdl2::pixels::Color::RGBA(65, 70, 80, 200),
    );
    if arc_r_i16 > 2 {
        let _ = canvas.arc(
            cx_i16,
            cy_i16,
            arc_r_i16 - 1,
            start_deg as i16,
            end_deg as i16,
            sdl2::pixels::Color::RGBA(50, 55, 62, 130),
        );
    }

    // ── Scale markings ──
    let marks: [(f64, &str); 7] = [
        (-20.0, "20"),
        (-10.0, "10"),
        (-7.0, "7"),
        (-5.0, "5"),
        (-3.0, "3"),
        (0.0, "0"),
        (3.0, "+3"),
    ];
    for &(db, label) in &marks {
        let t = ((db + 20.0) / 23.0).clamp(0.0, 1.0);
        let angle_deg = start_deg + t * sweep;
        let angle_rad = angle_deg.to_radians();
        let cos_a = angle_rad.cos();
        let sin_a = angle_rad.sin();

        let tick_outer = arc_r - 1.0;
        let tick_inner = arc_r - 5.0;
        let tx1 = cx + tick_outer * cos_a;
        let ty1 = arc_cy + tick_outer * sin_a;
        let tx2 = cx + tick_inner * cos_a;
        let ty2 = arc_cy + tick_inner * sin_a;
        let tick_col = if db >= 0.0 {
            sdl2::pixels::Color::RGBA(200, 70, 50, 220)
        } else {
            sdl2::pixels::Color::RGBA(140, 148, 158, 200)
        };
        canvas.set_draw_color(tick_col);
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(tx1 as i32, ty1 as i32),
            sdl2::rect::Point::new(tx2 as i32, ty2 as i32),
        );

        let label_r = arc_r + 3.0;
        let lx = cx + label_r * cos_a;
        let ly = arc_cy + label_r * sin_a;
        let lbl_col = if db >= 0.0 {
            sdl2::pixels::Color::RGBA(190, 70, 50, 200)
        } else {
            sdl2::pixels::Color::RGBA(120, 128, 140, 180)
        };
        let lbl_w = label.len() as i32 * 5;
        draw_pixel_label_scaled(
            canvas,
            theme,
            label,
            lx as i32 - lbl_w / 2,
            ly as i32 - 6,
            lbl_w + 4,
            lbl_col,
            1,
        );
    }

    // ── Red zone arc (0dB to +3dB) ──
    {
        let zero_frac = 20.0 / 23.0;
        let red_start = start_deg + zero_frac * sweep;
        let _ = canvas.arc(
            cx_i16,
            cy_i16,
            arc_r_i16,
            red_start as i16,
            end_deg as i16,
            sdl2::pixels::Color::RGBA(180, 50, 40, 160),
        );
        if arc_r_i16 > 3 {
            let _ = canvas.arc(
                cx_i16,
                cy_i16,
                arc_r_i16 - 1,
                red_start as i16,
                end_deg as i16,
                sdl2::pixels::Color::RGBA(160, 45, 35, 100),
            );
        }
    }

    // "VU" label
    let vu_label_y = inner_y + inner_h - 7;
    draw_pixel_label_scaled(
        canvas,
        theme,
        "VU",
        (cx as i32) - 5,
        vu_label_y,
        14,
        sdl2::pixels::Color::RGBA(90, 95, 105, 140),
        1,
    );

    // ── Needle ──
    let np = (needle_pos as f64).clamp(0.0, 1.0);
    let needle_angle_deg = start_deg + np * sweep;
    let needle_angle_rad = needle_angle_deg.to_radians();
    let needle_len = arc_r - 7.0;
    let nx = cx + needle_len * needle_angle_rad.cos();
    let ny = arc_cy + needle_len * needle_angle_rad.sin();

    let vis_pivot_y = (inner_y + inner_h - 3) as f64;
    let vis_pivot_x = cx;

    // ── Peak hold needle ──
    {
        let pp = (peak_pos as f64).clamp(0.0, 1.0);
        let peak_angle_deg = start_deg + pp * sweep;
        let peak_angle_rad = peak_angle_deg.to_radians();
        let peak_len = arc_r - 7.0;
        let pkx = cx + peak_len * peak_angle_rad.cos();
        let pky = arc_cy + peak_len * peak_angle_rad.sin();
        let pdx = pkx - vis_pivot_x;
        let pdy = pky - vis_pivot_y;
        let pdist = (pdx * pdx + pdy * pdy).sqrt();
        if pdist > 2.0 && pp > 0.01 {
            let _ = canvas.aa_line(
                vis_pivot_x as i16,
                vis_pivot_y as i16,
                pkx as i16,
                pky as i16,
                sdl2::pixels::Color::RGBA(230, 50, 35, 200),
            );
            let _ = canvas.aa_line(
                vis_pivot_x as i16 + 1,
                vis_pivot_y as i16,
                pkx as i16 + 1,
                pky as i16,
                sdl2::pixels::Color::RGBA(230, 50, 35, 130),
            );
        }
    }

    let dx = nx - vis_pivot_x;
    let dy = ny - vis_pivot_y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > 2.0 {
        let _ = canvas.aa_line(
            vis_pivot_x as i16,
            vis_pivot_y as i16 + 1,
            nx as i16 + 1,
            ny as i16 + 1,
            sdl2::pixels::Color::RGBA(0, 0, 0, 80),
        );
        let needle_col = if needle_pos > 0.87 {
            sdl2::pixels::Color::RGBA(240, 65, 40, 255)
        } else {
            sdl2::pixels::Color::RGBA(235, 240, 245, 255)
        };
        let needle_col2 = if needle_pos > 0.87 {
            sdl2::pixels::Color::RGBA(240, 65, 40, 160)
        } else {
            sdl2::pixels::Color::RGBA(235, 240, 245, 160)
        };
        let _ = canvas.aa_line(
            vis_pivot_x as i16,
            vis_pivot_y as i16,
            nx as i16,
            ny as i16,
            needle_col,
        );
        let _ = canvas.aa_line(
            vis_pivot_x as i16 + 1,
            vis_pivot_y as i16,
            nx as i16 + 1,
            ny as i16,
            needle_col2,
        );
    }

    // Pivot dot
    let _ = canvas.filled_circle(
        vis_pivot_x as i16,
        vis_pivot_y as i16,
        2,
        sdl2::pixels::Color::RGBA(160, 165, 170, 255),
    );

    // Subtle border
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 100));
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));
}
