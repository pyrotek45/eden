//! Exercise SDL2_gfx's native pixel boundary, not just integer color arithmetic.
#[path = "../src/widgets/gfx.rs"]
mod gfx;
use gfx::GfxRenderer;
use sdl2::{
    pixels::{Color, PixelFormatEnum},
    surface::Surface,
};

#[test]
fn gfx_shapes_preserve_theme_channels_and_opacity() {
    let surface = Surface::new(48, 48, PixelFormatEnum::RGBA32).unwrap();
    let mut canvas = surface.into_canvas().unwrap();
    // Neutral knob face, cool/warm/green theme accents, and fully opaque white.
    for color in [
        Color::RGB(50, 50, 50),
        Color::RGB(38, 139, 210),
        Color::RGB(255, 165, 0),
        Color::RGB(166, 226, 46),
        Color::RGB(255, 255, 255),
    ] {
        for shape in 0..7 {
            canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
            canvas.set_draw_color(Color::RGB(0, 0, 0));
            canvas.clear();
            let at = match shape {
                0 => {
                    canvas.gfx_filled_circle(24, 24, 10, color).unwrap();
                    (24, 24)
                }
                1 => {
                    canvas.gfx_thick_line(14, 24, 34, 24, 3, color).unwrap();
                    (24, 24)
                }
                2 => {
                    canvas
                        .gfx_filled_trigon(14, 34, 24, 14, 34, 34, color)
                        .unwrap();
                    (24, 24)
                }
                3 => {
                    canvas.gfx_aa_line(14, 24, 34, 24, color).unwrap();
                    (24, 24)
                }
                4 => {
                    canvas.gfx_aa_circle(24, 24, 10, color).unwrap();
                    (24, 14)
                }
                5 => {
                    canvas.gfx_circle(24, 24, 10, color).unwrap();
                    (24, 14)
                }
                _ => {
                    canvas.gfx_arc(24, 24, 10, 0, 180, color).unwrap();
                    (24, 34)
                }
            };
            assert_eq!(
                canvas.blend_mode(),
                sdl2::render::BlendMode::Blend,
                "graphics primitives must not change blending for subsequent UI panels"
            );
            assert_eq!(
                canvas.draw_color(),
                Color::RGB(0, 0, 0),
                "graphics primitives must restore the canvas color"
            );
            let pixels = canvas.read_pixels(None, PixelFormatEnum::RGB24).unwrap();
            let i = (at.1 * 48 + at.0) * 3;
            assert_eq!(
                &pixels[i..i + 3],
                &[color.r, color.g, color.b],
                "shape {shape} must preserve theme channels"
            );
        }
    }
    // Alpha belongs in the alpha byte, not in red. Allow integer blend rounding.
    let background = Color::RGB(20, 40, 60);
    let foreground = Color::RGBA(30, 140, 210, 128);
    canvas.set_draw_color(background);
    canvas.clear();
    canvas.gfx_filled_circle(24, 24, 10, foreground).unwrap();
    let pixels = canvas.read_pixels(None, PixelFormatEnum::RGB24).unwrap();
    let i = (24 * 48 + 24) * 3;
    for (actual, (fg, bg)) in
        pixels[i..i + 3]
            .iter()
            .zip([foreground.r, foreground.g, foreground.b].into_iter().zip([
                background.r,
                background.g,
                background.b,
            ]))
    {
        let expected = (fg as i32 * 128 + bg as i32 * 127) / 255;
        assert!(
            (*actual as i32 - expected).abs() <= 2,
            "opacity/channel mismatch: {actual} vs {expected}"
        );
    }
}
