// Eden DAW — Widget system
//
// Immediate-mode-style widgets: Knob, Slider, Button, etc.
// All rendering uses SDL2 primitives + the Theme.
//
// Each widget lives in its own sub-module for easy navigation.
// This mod.rs re-exports everything so `use crate::widgets::*` continues to work.

mod button;
mod clip_widgets;
mod comp_curve;
mod dropdown;
mod knob;
mod number_spinner;
mod pixel_label;
mod row_layout;
mod scrollbar;
mod slider;
mod text_field;
mod toggle_button;
mod track_icon;
mod value_bar;
mod view_layers;
mod vu_meter;

pub use button::*;
pub use clip_widgets::*;
pub use comp_curve::*;
pub use dropdown::*;
pub use knob::*;
pub use number_spinner::*;
pub use pixel_label::*;
pub use row_layout::*;
pub use scrollbar::*;
pub use slider::*;
pub use text_field::*;
pub use toggle_button::*;
pub use track_icon::*;
#[allow(unused_imports)]
pub use value_bar::*;
pub use view_layers::*;
pub use vu_meter::*;

// ── Global font (pixel-label) scale ──────────────────────────────────
use std::cell::Cell;
thread_local! {
    static FONT_SCALE: Cell<i32> = const { Cell::new(2) };
}

/// Set the global font scale used by `draw_pixel_label`. Call once per frame.
pub fn set_font_scale(scale: i32) {
    FONT_SCALE.with(|s| s.set(scale.clamp(1, 4)));
}

/// Get the current global font scale.
pub fn get_font_scale() -> i32 {
    FONT_SCALE.with(|s| s.get())
}

// ── Shared helpers (used by multiple widget files) ───────────────────

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(crate) fn inv_lerp(a: f32, b: f32, v: f32) -> f32 {
    if (b - a).abs() < 1e-9 {
        0.0
    } else {
        (v - a) / (b - a)
    }
}

/// Draw a thick arc — used by the Knob widget.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_thick_arc(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    cx: f64,
    cy: f64,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    thickness: i32,
    color: sdl2::pixels::Color,
) {
    use sdl2::gfx::primitives::DrawRenderer;
    let steps = ((end_deg - start_deg).abs() / 3.0).max(8.0) as i32;
    let step_a = (end_deg - start_deg) / steps as f64;
    for t in -(thickness / 2)..=(thickness / 2) {
        let r = radius + t as f64;
        for i in 0..steps {
            let a1 = (start_deg + i as f64 * step_a).to_radians();
            let a2 = (start_deg + (i + 1) as f64 * step_a).to_radians();
            let _ = canvas.aa_line(
                (cx + a1.cos() * r) as i16,
                (cy + a1.sin() * r) as i16,
                (cx + a2.cos() * r) as i16,
                (cy + a2.sin() * r) as i16,
                color,
            );
        }
    }
}
