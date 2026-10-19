//! SDL2_gfx packed colors are RGBA bytes in native memory order.
//!
//! rust-sdl2 0.38's Color adapter instead constructs a big-endian integer, which
//! reverses channels/alpha on little-endian targets. Pass the native-byte integer
//! explicitly to its packed-color functions. Regular Canvas drawing still takes
//! Color directly; do not apply this conversion there.
//!
//! Native contract: SDL2_gfxPrimitives.c, filledCircleColor (and other *Color APIs):
//! https://github.com/ferzkopp/SDL2_gfx/blob/master/SDL2_gfxPrimitives.c
pub(crate) fn gfx_color(color: sdl2::pixels::Color) -> u32 {
    u32::from_ne_bytes([color.r, color.g, color.b, color.a])
}

/// SDL2_gfx changes SDL's drawing color and blend mode internally. Keep those
/// implementation details local so later translucent UI layers still blend.
pub(crate) trait GfxRenderer {
    fn gfx_arc(
        &mut self,
        x: i16,
        y: i16,
        radius: i16,
        start: i16,
        end: i16,
        color: sdl2::pixels::Color,
    ) -> Result<(), String>;
    fn gfx_filled_circle(
        &mut self,
        x: i16,
        y: i16,
        radius: i16,
        color: sdl2::pixels::Color,
    ) -> Result<(), String>;
    fn gfx_circle(
        &mut self,
        x: i16,
        y: i16,
        radius: i16,
        color: sdl2::pixels::Color,
    ) -> Result<(), String>;
    fn gfx_aa_circle(
        &mut self,
        x: i16,
        y: i16,
        radius: i16,
        color: sdl2::pixels::Color,
    ) -> Result<(), String>;
    fn gfx_aa_line(
        &mut self,
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
        color: sdl2::pixels::Color,
    ) -> Result<(), String>;
    fn gfx_thick_line(
        &mut self,
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
        width: u8,
        color: sdl2::pixels::Color,
    ) -> Result<(), String>;
    #[allow(clippy::too_many_arguments)]
    fn gfx_filled_trigon(
        &mut self,
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
        x3: i16,
        y3: i16,
        color: sdl2::pixels::Color,
    ) -> Result<(), String>;
}

/// Preserve renderer state once for a batch, such as the segments of a knob arc.
pub(crate) fn with_gfx_state<T: sdl2::render::RenderTarget, R>(
    canvas: &mut sdl2::render::Canvas<T>,
    draw: impl FnOnce(&mut sdl2::render::Canvas<T>) -> R,
) -> R {
    let previous_blend = canvas.blend_mode();
    let previous_color = canvas.draw_color();
    let result = draw(canvas);
    canvas.set_blend_mode(previous_blend);
    canvas.set_draw_color(previous_color);
    result
}

macro_rules! primitive {
    ($name:ident, $native:ident, $( $arg:ident: $kind:ty ),+) => {
        fn $name(&mut self, $( $arg: $kind, )+ color: sdl2::pixels::Color) -> Result<(), String> {
            with_gfx_state(self, |canvas| {
                sdl2::gfx::primitives::DrawRenderer::$native(canvas, $( $arg, )+ gfx_color(color))
            })
        }
    };
}

impl<T: sdl2::render::RenderTarget> GfxRenderer for sdl2::render::Canvas<T> {
    primitive!(gfx_arc, arc, x: i16, y: i16, radius: i16, start: i16, end: i16);
    primitive!(gfx_filled_circle, filled_circle, x: i16, y: i16, radius: i16);
    primitive!(gfx_circle, circle, x: i16, y: i16, radius: i16);
    primitive!(gfx_aa_circle, aa_circle, x: i16, y: i16, radius: i16);
    primitive!(gfx_aa_line, aa_line, x1: i16, y1: i16, x2: i16, y2: i16);
    primitive!(gfx_thick_line, thick_line, x1: i16, y1: i16, x2: i16, y2: i16, width: u8);
    primitive!(gfx_filled_trigon, filled_trigon, x1: i16, y1: i16, x2: i16, y2: i16, x3: i16, y3: i16);
}
