// Eden DAW — Widget system
// Immediate-mode-style widgets: Knob, Slider, Button
// All rendering uses SDL2 primitives + the Theme.

use sdl2::gfx::primitives::DrawRenderer;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::theme::Theme;

// ── Global font (pixel-label) scale ──────────────────────────────────
// Set once per frame via `set_font_scale` before any draw_pixel_label calls.
// draw_pixel_label reads it automatically; no call-site changes needed.

use std::cell::Cell;
thread_local! {
    static FONT_SCALE: Cell<i32> = const { Cell::new(2) };
}

/// Set the global font scale used by `draw_pixel_label`. Call once per frame
/// with `state.font_scale` before drawing the UI.
pub fn set_font_scale(scale: i32) {
    FONT_SCALE.with(|s| s.set(scale.clamp(1, 4)));
}

/// Get the current global font scale.
pub fn get_font_scale() -> i32 {
    FONT_SCALE.with(|s| s.get())
}

// ── Helper ───────────────────────────────────────────────────────────

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn inv_lerp(a: f32, b: f32, v: f32) -> f32 {
    if (b - a).abs() < 1e-9 {
        0.0
    } else {
        (v - a) / (b - a)
    }
}

// ── Knob ─────────────────────────────────────────────────────────────

pub struct KnobParams {
    pub id: WidgetId,
    pub x: i32,
    pub y: i32,
    pub radius: i32,
    pub min: f32,
    pub max: f32,
    pub sensitivity: f32,
    pub label: Option<String>,
    pub bipolar: bool,
    /// Value to snap to on double-click (default = midpoint of min..max, or 0 for bipolar).
    pub default_value: Option<f32>,
    /// Hover tooltip text
    pub hint: Option<String>,
    /// Snap points: if the knob value comes within a small threshold of any
    /// of these values, it will snap to that value.  Leave empty for no snapping.
    pub snap_points: Vec<f32>,
}

impl Default for KnobParams {
    fn default() -> Self {
        Self {
            id: WidgetId::None,
            x: 0,
            y: 0,
            radius: 20,
            min: 0.0,
            max: 1.0,
            sensitivity: 0.005,
            label: None,
            bipolar: false,
            default_value: None,
            hint: None,
            snap_points: Vec::new(),
        }
    }
}

/// Draw a knob and handle interaction. Returns true if value changed.
pub fn knob(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    params: &KnobParams,
    value: &mut f32,
) -> bool {
    let cx = params.x as i16;
    let cy = params.y as i16;
    let r = params.radius as i16;
    let hover = input.mouse_in_rect(
        params.x - params.radius - 2,
        params.y - params.radius - 2,
        params.radius * 2 + 4,
        params.radius * 2 + 4,
    );

    let mut changed = false;

    // Interaction (guard: only start drag if no other widget is active)
    if hover && input.mouse_pressed && input.active_widget == WidgetId::None {
        // Shift+click → reset to default
        if input.shift() {
            let dflt = params.default_value.unwrap_or(if params.bipolar {
                0.0
            } else {
                lerp(params.min, params.max, 0.5)
            });
            *value = dflt.clamp(params.min, params.max);
            input.consume();
            return true;
        }
        input.active_widget = params.id;
        input.drag_widget = params.id;
        input.drag_start_value = *value as f64;
        input.consume();
    }

    // Use drag_widget for continuous drag (survives mouse leaving knob area)
    if (input.active_widget == params.id || input.drag_widget == params.id) && input.mouse_down {
        let delta = -input.mouse_dy as f32 * params.sensitivity;
        *value = (*value + delta * (params.max - params.min)).clamp(params.min, params.max);
        if delta.abs() > 0.0 {
            // Apply snap points (gravity threshold = 0.3% of range — subtle catch)
            if !params.snap_points.is_empty() {
                let range = params.max - params.min;
                let threshold = range * 0.003;
                for &sp in &params.snap_points {
                    if (*value - sp).abs() < threshold {
                        *value = sp;
                        break;
                    }
                }
            }
            changed = true;
        }
    }

    // Middle mouse button — ultra-fine adjustment
    // Capture the widget on initial middle-click so dragging off still works
    if hover && input.middle_mouse_down && input.middle_drag_widget == WidgetId::None {
        input.middle_drag_widget = params.id;
        input.drag_start_value = *value as f64;
    }
    if input.middle_drag_widget == params.id && input.middle_mouse_down {
        let fine_sens = params.sensitivity * 0.2; // 0.2x normal — fine but perceptible
        let delta = -input.mouse_dy as f32 * fine_sens;
        *value = (*value + delta * (params.max - params.min)).clamp(params.min, params.max);
        if delta.abs() > 0.0 {
            // Apply snap points (tighter threshold for fine adjustment)
            if !params.snap_points.is_empty() {
                let range = params.max - params.min;
                let threshold = range * 0.0015;
                for &sp in &params.snap_points {
                    if (*value - sp).abs() < threshold {
                        *value = sp;
                        break;
                    }
                }
            }
            changed = true;
        }
    }

    if hover {
        input.hot_widget = params.id;
        // Set hover hint text (for tooltip display)
        if let Some(ref hint) = params.hint {
            input.hover_hint_text = Some(hint.clone());
            input.hover_hint_widget = params.id;
        } else if let Some(ref label) = params.label {
            input.hover_hint_text = Some(format!("{}: {:.2}", label, *value));
            input.hover_hint_widget = params.id;
        }
    }

    let is_middle_dragging = input.middle_drag_widget == params.id;
    let is_active = input.active_widget == params.id || is_middle_dragging;
    let is_hot = hover;

    // Keep hint visible while middle-dragging (mouse may move off the knob)
    if is_middle_dragging {
        if let Some(ref hint) = params.hint {
            input.hover_hint_text = Some(hint.clone());
            input.hover_hint_widget = params.id;
        } else if let Some(ref label) = params.label {
            input.hover_hint_text = Some(format!("{}: {:.2}", label, *value));
            input.hover_hint_widget = params.id;
        }
    }

    // Shadow
    let _ = canvas.filled_circle(cx, cy + 1, r + 1, sdl2::pixels::Color::RGBA(0, 0, 0, 50));

    // Outer ring
    let outer_color = if is_active {
        Theme::c(theme.accent)
    } else if is_hot {
        Theme::c(theme.accent_hover)
    } else {
        Theme::c(theme.panel_border)
    };
    let _ = canvas.filled_circle(cx, cy, r, outer_color);

    // Main body
    let body_r = r - 2;
    let body_color = if is_active {
        sdl2::pixels::Color::RGBA(
            theme.knob_bg[0].saturating_add(20),
            theme.knob_bg[1].saturating_add(20),
            theme.knob_bg[2].saturating_add(20),
            255,
        )
    } else {
        Theme::c(theme.knob_bg)
    };
    let _ = canvas.filled_circle(cx, cy, body_r, body_color);

    // Value arc
    let t = inv_lerp(params.min, params.max, *value);
    let start_deg = 135.0_f64;
    let total_sweep = 270.0_f64;
    let arc_r = (body_r as f64) - 1.0;

    // Background track
    let track_col = sdl2::pixels::Color::RGBA(
        theme.knob_bg[0].saturating_sub(12),
        theme.knob_bg[1].saturating_sub(12),
        theme.knob_bg[2].saturating_sub(12),
        255,
    );
    draw_thick_arc(
        canvas,
        cx as f64,
        cy as f64,
        arc_r,
        start_deg,
        start_deg + total_sweep,
        3,
        track_col,
    );

    // Filled arc
    let ind_col = if is_active {
        sdl2::pixels::Color::RGBA(
            theme.knob_indicator[0].saturating_add(30),
            theme.knob_indicator[1].saturating_add(30),
            theme.knob_indicator[2].saturating_add(30),
            255,
        )
    } else {
        Theme::c(theme.knob_indicator)
    };

    if params.bipolar {
        let center_a = start_deg + total_sweep * 0.5;
        let val_a = start_deg + (t as f64) * total_sweep;
        let (from, to) = if val_a < center_a {
            (val_a, center_a)
        } else {
            (center_a, val_a)
        };
        draw_thick_arc(canvas, cx as f64, cy as f64, arc_r, from, to, 3, ind_col);
    } else {
        let end_a = start_deg + (t as f64) * total_sweep;
        draw_thick_arc(
            canvas, cx as f64, cy as f64, arc_r, start_deg, end_a, 3, ind_col,
        );
    }

    // Pointer line
    let ptr_a = (start_deg + t as f64 * total_sweep).to_radians();
    let r_start = body_r as f64 * 0.3;
    let r_end = body_r as f64 * 0.85;
    let px1 = cx as f64 + ptr_a.cos() * r_start;
    let py1 = cy as f64 + ptr_a.sin() * r_start;
    let px2 = cx as f64 + ptr_a.cos() * r_end;
    let py2 = cy as f64 + ptr_a.sin() * r_end;
    let ptr_col = if is_active {
        sdl2::pixels::Color::RGBA(255, 255, 255, 255)
    } else {
        Theme::c(theme.knob_indicator)
    };
    let _ = canvas.thick_line(px1 as i16, py1 as i16, px2 as i16, py2 as i16, 2, ptr_col);

    // Center dot
    let dot_col = if is_active {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.knob_fg)
    };
    let _ = canvas.filled_circle(cx, cy, 2, dot_col);

    // Active glow
    if is_active {
        let glow = sdl2::pixels::Color::RGBA(theme.accent[0], theme.accent[1], theme.accent[2], 35);
        let _ = canvas.aa_circle(cx, cy, r + 3, glow);
        let _ = canvas.aa_circle(cx, cy, r + 4, glow);
    }

    // Value display below knob (small, shows current numeric value)
    let below_y = params.y + params.radius + 3;
    if is_active || hover {
        // Show live value when interacting
        let val_str = if (params.max - params.min).abs() > 10.0 {
            format!("{:.1}", *value)
        } else {
            format!("{:.2}", *value)
        };
        let val_w = val_str.len() as i32 * 9 + 2;
        let val_x = params.x - val_w / 2;
        let val_col = Theme::c(theme.accent);
        draw_pixel_label(canvas, theme, &val_str, val_x, below_y, val_w + 4, val_col);
    }

    // Label below the knob (always visible)
    if let Some(ref label) = params.label {
        let label_scale = 1i32; // use 1x scale (glyph is 4×5 base)
        let glyph_w = 4 * label_scale * 2 + 1; // 2x render scale in draw_pixel_label
        let label_px_w = label.len() as i32 * glyph_w;
        let label_x = params.x - label_px_w / 2;
        let label_y = if is_active || hover {
            below_y + 12 // push down to make room for value
        } else {
            below_y
        };
        let label_col = if is_active {
            Theme::c(theme.accent)
        } else if hover {
            Theme::c(theme.text_primary)
        } else {
            Theme::c(theme.text_secondary)
        };
        draw_pixel_label(
            canvas,
            theme,
            label,
            label_x,
            label_y,
            label_px_w + 8,
            label_col,
        );
    }

    changed
}

#[allow(clippy::too_many_arguments)]
fn draw_thick_arc(
    canvas: &mut Canvas<Window>,
    cx: f64,
    cy: f64,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    thickness: i32,
    color: sdl2::pixels::Color,
) {
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

// ── Slider ───────────────────────────────────────────────────────────

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
        // Shift+click → reset to default
        if input.shift() {
            let dflt = params
                .default_value
                .unwrap_or(lerp(params.min, params.max, 0.5));
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
        // Drag-relative: use delta from drag start, like knobs
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

    // Middle mouse button — ultra-fine adjustment for sliders
    if hover && input.middle_mouse_down && input.middle_drag_widget == WidgetId::None {
        input.middle_drag_widget = params.id;
    }
    if input.middle_drag_widget == params.id && input.middle_mouse_down {
        let base_sensitivity = 1.0
            / match params.orientation {
                SliderOrientation::Horizontal => params.width as f32,
                SliderOrientation::Vertical => params.height as f32,
            };
        let fine_sens = base_sensitivity * 0.3; // fine-tune but still perceptible
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

// ── Scrollbar ─────────────────────────────────────────────────────────
// A thin scrollbar track + draggable thumb.
// `scroll`: current scroll position (0.0 = start, 1.0 = end).
// `thumb_ratio`: fraction of total content that is visible (0.0–1.0 thumb size).
// Returns the new scroll value (0.0–1.0). Call it every frame.
//
// Horizontal:  x,y = top-left corner, length = track length, thickness = track height.
// Vertical:    x,y = top-left corner, length = track height, thickness = track width.

#[derive(Clone, Copy, PartialEq)]
pub enum ScrollbarDir {
    Horizontal,
    Vertical,
}

#[allow(clippy::too_many_arguments)]
pub fn scrollbar(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: WidgetId,
    x: i32,
    y: i32,
    length: i32,
    thickness: i32,
    dir: ScrollbarDir,
    scroll: f32,      // 0.0 – 1.0 current scroll fraction
    thumb_ratio: f32, // 0.0 – 1.0 fraction of content visible
) -> f32 {
    if length <= 0 || thickness <= 0 {
        return scroll;
    }
    let thumb_ratio = thumb_ratio.clamp(0.05, 1.0);
    let thumb_len = ((length as f32 * thumb_ratio) as i32).max(12);
    let travel = (length - thumb_len).max(1);
    let thumb_offset = (scroll * travel as f32) as i32;

    let (track_rect, thumb_rect) = match dir {
        ScrollbarDir::Horizontal => (
            Rect::new(x, y, length as u32, thickness as u32),
            Rect::new(
                x + thumb_offset,
                y + 1,
                thumb_len as u32,
                (thickness - 2).max(1) as u32,
            ),
        ),
        ScrollbarDir::Vertical => (
            Rect::new(x, y, thickness as u32, length as u32),
            Rect::new(
                x + 1,
                y + thumb_offset,
                (thickness - 2).max(1) as u32,
                thumb_len as u32,
            ),
        ),
    };

    // Draw track
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 24, 200));
    let _ = canvas.fill_rect(track_rect);
    canvas.set_draw_color(Theme::c(theme.panel_border));
    let _ = canvas.draw_rect(track_rect);

    let thumb_hover = input.mouse_in_rect(
        thumb_rect.x(),
        thumb_rect.y(),
        thumb_rect.width() as i32,
        thumb_rect.height() as i32,
    );
    let track_hover = input.mouse_in_rect(
        track_rect.x(),
        track_rect.y(),
        track_rect.width() as i32,
        track_rect.height() as i32,
    );

    // Interaction: start drag on thumb press (only if not already consumed — e.g. by a squeeze handle)
    if thumb_hover && input.mouse_pressed && !input.consumed {
        input.drag_widget = id;
        input.active_widget = id;
        input.drag_start_value = scroll as f64;
        input.drag_start_x = input.mouse_x;
        input.drag_start_y = input.mouse_y;
        input.consume();
    }

    // Click on track (outside thumb) — jump
    if track_hover && !thumb_hover && input.mouse_pressed && !input.consumed {
        let raw = match dir {
            ScrollbarDir::Horizontal => (input.mouse_x - x - thumb_len / 2) as f32 / travel as f32,
            ScrollbarDir::Vertical => (input.mouse_y - y - thumb_len / 2) as f32 / travel as f32,
        };
        input.drag_widget = id;
        input.active_widget = id;
        input.drag_start_value = raw.clamp(0.0, 1.0) as f64;
        input.drag_start_x = input.mouse_x;
        input.drag_start_y = input.mouse_y;
        input.consume();
        return raw.clamp(0.0, 1.0);
    }

    let mut new_scroll = scroll;
    if input.drag_widget == id && input.mouse_down {
        let delta_px = match dir {
            ScrollbarDir::Horizontal => (input.mouse_x - input.drag_start_x) as f32,
            ScrollbarDir::Vertical => (input.mouse_y - input.drag_start_y) as f32,
        };
        let raw = input.drag_start_value as f32 + delta_px / travel as f32;
        new_scroll = raw.clamp(0.0, 1.0);
    }

    // Draw thumb
    let thumb_col = if input.drag_widget == id {
        Theme::c(theme.accent)
    } else if thumb_hover {
        Theme::c(theme.accent_hover)
    } else {
        Theme::c(theme.slider_thumb)
    };
    canvas.set_draw_color(thumb_col);
    let _ = canvas.fill_rect(thumb_rect);

    new_scroll
}

/// Like `scrollbar` but also draws squeeze handles at the left/right (or top/bottom)
/// edges of the thumb. Dragging a handle changes the `thumb_ratio` (zooming in/out).
/// Returns `(new_scroll, new_thumb_ratio)`.
///
/// `squeeze_id_lo` / `squeeze_id_hi` are WidgetIds for the two handles.
#[allow(clippy::too_many_arguments)]
pub fn scrollbar_with_squeeze(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: WidgetId,
    squeeze_id_lo: WidgetId,
    squeeze_id_hi: WidgetId,
    x: i32,
    y: i32,
    length: i32,
    thickness: i32,
    dir: ScrollbarDir,
    scroll: f32,      // current scroll (0..1)
    thumb_ratio: f32, // visible fraction (0..1)
) -> (f32, f32) {
    // (new_scroll, new_thumb_ratio)
    let thumb_ratio_clamped = thumb_ratio.clamp(0.05, 1.0);
    // Visual size of squeeze handle nubs
    const HANDLE_SIZE: i32 = 14;
    // Extra pixels beyond the visual rect that still count as a hit (makes handles easy to grab)
    const HIT_PAD: i32 = 4;
    let thumb_len = ((length as f32 * thumb_ratio_clamped) as i32).max(HANDLE_SIZE * 2 + 2);
    let travel = (length - thumb_len).max(1);
    let thumb_offset = (scroll.clamp(0.0, 1.0) * travel as f32) as i32;

    // Compute handle rects based on CURRENT thumb position (before any drag update)
    let (lo_rect, hi_rect) = match dir {
        ScrollbarDir::Horizontal => (
            Rect::new(x + thumb_offset, y, HANDLE_SIZE as u32, thickness as u32),
            Rect::new(
                x + thumb_offset + thumb_len - HANDLE_SIZE,
                y,
                HANDLE_SIZE as u32,
                thickness as u32,
            ),
        ),
        ScrollbarDir::Vertical => (
            Rect::new(x, y + thumb_offset, thickness as u32, HANDLE_SIZE as u32),
            Rect::new(
                x,
                y + thumb_offset + thumb_len - HANDLE_SIZE,
                thickness as u32,
                HANDLE_SIZE as u32,
            ),
        ),
    };

    // Use padded hit rects so the handles are easy to grab even at small scrollbar heights
    let lo_hit = match dir {
        ScrollbarDir::Horizontal => Rect::new(
            lo_rect.x() - HIT_PAD,
            lo_rect.y() - HIT_PAD,
            (lo_rect.width() as i32 + HIT_PAD * 2) as u32,
            (lo_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
        ScrollbarDir::Vertical => Rect::new(
            lo_rect.x() - HIT_PAD,
            lo_rect.y() - HIT_PAD,
            (lo_rect.width() as i32 + HIT_PAD * 2) as u32,
            (lo_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
    };
    let hi_hit = match dir {
        ScrollbarDir::Horizontal => Rect::new(
            hi_rect.x() - HIT_PAD,
            hi_rect.y() - HIT_PAD,
            (hi_rect.width() as i32 + HIT_PAD * 2) as u32,
            (hi_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
        ScrollbarDir::Vertical => Rect::new(
            hi_rect.x() - HIT_PAD,
            hi_rect.y() - HIT_PAD,
            (hi_rect.width() as i32 + HIT_PAD * 2) as u32,
            (hi_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
    };

    let lo_hover = input.mouse_in_rect(
        lo_hit.x(),
        lo_hit.y(),
        lo_hit.width() as i32,
        lo_hit.height() as i32,
    );
    let hi_hover = input.mouse_in_rect(
        hi_hit.x(),
        hi_hit.y(),
        hi_hit.width() as i32,
        hi_hit.height() as i32,
    );

    // Claim squeeze handle drag BEFORE the main scrollbar sees the press.
    // We guard on drag_widget == None rather than !consumed: this ensures no
    // other drag is already in progress, while not being blocked by consumed
    // flags set by non-drag interactions (e.g. piano roll note clicks that call
    // input.consume() even though they don't set drag_widget).
    if input.mouse_pressed && lo_hover && input.drag_widget == WidgetId::None {
        input.drag_widget = squeeze_id_lo;
        input.active_widget = squeeze_id_lo;
        input.drag_start_value = thumb_ratio as f64;
        input.drag_start_value2 = scroll as f64;
        input.drag_start_x = match dir {
            ScrollbarDir::Horizontal => input.mouse_x,
            ScrollbarDir::Vertical => input.mouse_y,
        };
        input.consumed = true;
    } else if input.mouse_pressed && hi_hover && input.drag_widget == WidgetId::None {
        input.drag_widget = squeeze_id_hi;
        input.active_widget = squeeze_id_hi;
        input.drag_start_value = thumb_ratio as f64;
        input.drag_start_value2 = scroll as f64;
        input.drag_start_x = match dir {
            ScrollbarDir::Horizontal => input.mouse_x,
            ScrollbarDir::Vertical => input.mouse_y,
        };
        input.consumed = true;
    }

    // Main scrollbar (only processes the click if no squeeze handle is active)
    let new_scroll = if input.active_widget == squeeze_id_lo
        || input.active_widget == squeeze_id_hi
        || input.drag_widget == squeeze_id_lo
        || input.drag_widget == squeeze_id_hi
    {
        // Squeeze dragging — don't move the scroll thumb
        scroll
    } else {
        scrollbar(
            canvas,
            input,
            theme,
            id,
            x,
            y,
            length,
            thickness,
            dir,
            scroll,
            thumb_ratio,
        )
    };

    // If a squeeze handle IS active, still draw the scrollbar track (without re-running interaction)
    if input.active_widget == squeeze_id_lo
        || input.active_widget == squeeze_id_hi
        || input.drag_widget == squeeze_id_lo
        || input.drag_widget == squeeze_id_hi
    {
        // Draw track background
        canvas.set_draw_color(Theme::c(theme.scrollbar_bg));
        let _ = match dir {
            ScrollbarDir::Horizontal => {
                canvas.fill_rect(Rect::new(x, y, length as u32, thickness as u32))
            }
            ScrollbarDir::Vertical => {
                canvas.fill_rect(Rect::new(x, y, thickness as u32, length as u32))
            }
        };
        // Draw thumb
        canvas.set_draw_color(Theme::c(theme.scrollbar_thumb));
        let _ = match dir {
            ScrollbarDir::Horizontal => canvas.fill_rect(Rect::new(
                x + thumb_offset,
                y + 1,
                thumb_len as u32,
                (thickness - 2).max(1) as u32,
            )),
            ScrollbarDir::Vertical => canvas.fill_rect(Rect::new(
                x + 1,
                y + thumb_offset,
                (thickness - 2).max(1) as u32,
                thumb_len as u32,
            )),
        };
    }

    // Process squeeze drag
    let mut new_thumb_ratio = thumb_ratio;
    let mut new_scroll = new_scroll; // allow mutation
    if input.mouse_down
        && (input.drag_widget == squeeze_id_lo || input.drag_widget == squeeze_id_hi)
    {
        let cur_pos = match dir {
            ScrollbarDir::Horizontal => input.mouse_x,
            ScrollbarDir::Vertical => input.mouse_y,
        };
        let drag_delta_px = (cur_pos - input.drag_start_x) as f64;
        let ratio_delta = (drag_delta_px / length as f64) as f32;

        // Use the ORIGINAL values from drag start (not current frame values)
        let orig_ratio = (input.drag_start_value as f32).clamp(0.05, 1.0);
        let orig_scroll = (input.drag_start_value2 as f32).clamp(0.0, 1.0);
        let old_left = orig_scroll * (1.0 - orig_ratio); // left edge as content fraction
        let old_right = old_left + orig_ratio; // right edge as content fraction

        if input.drag_widget == squeeze_id_lo {
            // Lo handle: drag right → move left edge right (zoom in from left)
            let new_left = (old_left + ratio_delta).clamp(0.0, old_right - 0.05);
            new_thumb_ratio = (old_right - new_left).clamp(0.05, 1.0);
            // Recompute scroll so the left edge is at new_left
            if new_thumb_ratio < 1.0 {
                new_scroll = (new_left / (1.0 - new_thumb_ratio)).clamp(0.0, 1.0);
            } else {
                new_scroll = 0.0;
            }
        } else {
            // Hi handle: drag right → move right edge right (zoom out from right)
            let new_right = (old_right + ratio_delta).clamp(old_left + 0.05, 1.0);
            new_thumb_ratio = (new_right - old_left).clamp(0.05, 1.0);
            // Keep left edge pinned
            if new_thumb_ratio < 1.0 {
                new_scroll = (old_left / (1.0 - new_thumb_ratio)).clamp(0.0, 1.0);
            } else {
                new_scroll = 0.0;
            }
        }
    }

    // Recompute thumb position for drawing handles (use new_scroll for updated pos)
    let new_thumb_ratio_clamped = new_thumb_ratio.clamp(0.05, 1.0);
    let new_thumb_len = ((length as f32 * new_thumb_ratio_clamped) as i32).max(HANDLE_SIZE * 2);
    let new_travel = (length - new_thumb_len).max(1);
    let new_thumb_offset = (new_scroll.clamp(0.0, 1.0) * new_travel as f32) as i32;

    let (lo_draw_rect, hi_draw_rect) = match dir {
        ScrollbarDir::Horizontal => (
            Rect::new(
                x + new_thumb_offset,
                y,
                HANDLE_SIZE as u32,
                thickness as u32,
            ),
            Rect::new(
                x + new_thumb_offset + new_thumb_len - HANDLE_SIZE,
                y,
                HANDLE_SIZE as u32,
                thickness as u32,
            ),
        ),
        ScrollbarDir::Vertical => (
            Rect::new(
                x,
                y + new_thumb_offset,
                thickness as u32,
                HANDLE_SIZE as u32,
            ),
            Rect::new(
                x,
                y + new_thumb_offset + new_thumb_len - HANDLE_SIZE,
                thickness as u32,
                HANDLE_SIZE as u32,
            ),
        ),
    };

    // Draw squeeze handles
    let is_lo_active = input.drag_widget == squeeze_id_lo;
    let is_hi_active = input.drag_widget == squeeze_id_hi;
    let col_lo = if is_lo_active || lo_hover {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.accent_hover)
    };
    let col_hi = if is_hi_active || hi_hover {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.accent_hover)
    };
    canvas.set_draw_color(col_lo);
    let _ = canvas.fill_rect(lo_draw_rect);
    canvas.set_draw_color(col_hi);
    let _ = canvas.fill_rect(hi_draw_rect);

    // Grip lines
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 80));
    match dir {
        ScrollbarDir::Horizontal => {
            let mid_y = y + thickness / 2;
            for dx in [2i32, 4] {
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(lo_draw_rect.x() + dx, mid_y - 2),
                    sdl2::rect::Point::new(lo_draw_rect.x() + dx, mid_y + 2),
                );
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(hi_draw_rect.x() + dx, mid_y - 2),
                    sdl2::rect::Point::new(hi_draw_rect.x() + dx, mid_y + 2),
                );
            }
        }
        ScrollbarDir::Vertical => {
            let mid_x = x + thickness / 2;
            for dy in [2i32, 4] {
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(mid_x - 2, lo_draw_rect.y() + dy),
                    sdl2::rect::Point::new(mid_x + 2, lo_draw_rect.y() + dy),
                );
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(mid_x - 2, hi_draw_rect.y() + dy),
                    sdl2::rect::Point::new(mid_x + 2, hi_draw_rect.y() + dy),
                );
            }
        }
    }

    (new_scroll, new_thumb_ratio)
}

// ── Row Layout Container ──────────────────────────────────────────────
//
// Positions a list of buttons (or other fixed-width items) in a horizontal row.
// Fixed-width items keep their specified width; resizable items share leftover space.
//
// Usage:
//   let row = RowLayout { x: 10, y: 8, total_width: w - 20, height: 32, gap: 4 };
//   let mut items = vec![
//       RowItem { width: 36, can_resize: false },
//       RowItem { width: 56, can_resize: true, min_width: 40 },
//   ];
//   let sizes = row.layout(&items);    // returns (x, computed_width) per item
//   // Apply sizes[i].0 → button.x,  sizes[i].1 → button.width

pub struct RowLayout {
    /// Left edge of the row
    pub x: i32,
    /// Top edge of the row
    pub y: i32,
    /// Total horizontal space the row may use
    pub total_width: i32,
    /// Height of every item in the row (uniform)
    pub height: i32,
    /// Gap between items
    pub gap: i32,
}

/// Describes one slot in the row.
pub struct RowItem {
    /// Preferred / base width of this item
    pub width: i32,
    /// If true the item may grow or shrink to absorb leftover space
    pub can_resize: bool,
    /// Minimum width for resizable items
    pub min_width: i32,
}

impl RowLayout {
    /// Compute `(x_pos, actual_width)` for every item and return the list.
    pub fn layout(&self, items: &[RowItem]) -> Vec<(i32, i32)> {
        let n = items.len();
        if n == 0 {
            return Vec::new();
        }
        let total_gap = self.gap * (n as i32 - 1);

        // Sum of fixed-width items
        let fixed_total: i32 = items
            .iter()
            .filter(|it| !it.can_resize)
            .map(|it| it.width)
            .sum();

        // How many resizable items are there?
        let resize_count = items.iter().filter(|it| it.can_resize).count() as i32;

        // Available space for resizable items
        let leftover = (self.total_width - fixed_total - total_gap).max(0);
        let resize_each = if resize_count > 0 {
            (leftover / resize_count).max(0)
        } else {
            0
        };

        let mut out = Vec::with_capacity(n);
        let mut cursor = self.x;
        for item in items {
            let w = if item.can_resize {
                resize_each.max(item.min_width)
            } else {
                item.width
            };
            out.push((cursor, w));
            cursor += w + self.gap;
        }
        out
    }

    /// Like `layout` but fills from the RIGHT edge (for right-aligned rows).
    /// Items are laid out right-to-left in the slice order (index 0 = rightmost).
    pub fn layout_right(&self, items: &[RowItem]) -> Vec<(i32, i32)> {
        let n = items.len();
        if n == 0 {
            return Vec::new();
        }
        let total_gap = self.gap * (n as i32 - 1);

        let fixed_total: i32 = items
            .iter()
            .filter(|it| !it.can_resize)
            .map(|it| it.width)
            .sum();
        let resize_count = items.iter().filter(|it| it.can_resize).count() as i32;
        let leftover = (self.total_width - fixed_total - total_gap).max(0);
        let resize_each = if resize_count > 0 {
            (leftover / resize_count).max(0)
        } else {
            0
        };

        let mut out = vec![(0i32, 0i32); n];
        let mut cursor = self.x + self.total_width; // right edge
        for (i, item) in items.iter().enumerate() {
            let w = if item.can_resize {
                resize_each.max(item.min_width)
            } else {
                item.width
            };
            cursor -= w;
            out[i] = (cursor, w);
            cursor -= self.gap;
        }
        out
    }
}

// ── View-level Input Layer Manager ────────────────────────────────────
//
// Within a single view there may be multiple overlapping sub-regions (e.g. an
// open dropdown, a floating panel, the background track area). Widgets in
// higher-z regions must consume input before widgets in lower-z regions see it.
//
// Usage:
//   let mut layers = ViewLayers::new(input);
//   // Draw / process highest-z layer first, then lower layers.
//   // Widgets in each layer call layers.input(layer_index) to get an InputState
//   // reference. If a higher-indexed layer already consumed the event at that
//   // mouse position, the lower layer receives a zeroed InputState (no clicks,
//   // no scroll) but retains mouse position so hover-based visuals still work.
//
// The "consumed" flag on the real InputState already propagates within a single
// layer. ViewLayers adds inter-layer blocking: once input.consumed is set, all
// subsequent calls to layers.input_if_top() return a dead snapshot.

pub struct ViewLayers<'a> {
    real: &'a mut InputState,
    dead: InputState,
}

impl<'a> ViewLayers<'a> {
    pub fn new(input: &'a mut InputState) -> Self {
        let dead = InputState {
            mouse_x: input.mouse_x,
            mouse_y: input.mouse_y,
            ..Default::default()
        };
        ViewLayers { real: input, dead }
    }

    /// Return the real InputState — use this for the top-most (highest-z) region
    /// that should be processed first.
    pub fn top(&mut self) -> &mut InputState {
        self.real
    }

    /// Return the real InputState only if input has not yet been consumed;
    /// otherwise return a dead InputState (mouse position preserved, no events).
    /// Call this for lower-z regions after the higher-z region has been processed.
    pub fn below(&mut self) -> &mut InputState {
        if self.real.consumed {
            // Sync mouse position in case it changed
            self.dead.mouse_x = self.real.mouse_x;
            self.dead.mouse_y = self.real.mouse_y;
            &mut self.dead
        } else {
            self.real
        }
    }
}

// ── Button ───────────────────────────────────────────────────────────

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
            // Draw a cleaner loop icon: two opposing arrows forming a rounded rectangle
            let sz = (params.height as f32 * 0.26) as i32;
            let hw = sz; // half width
            let hh = (sz as f32 * 0.6) as i32; // half height (shorter than wide)
            canvas.set_draw_color(c);
            // Top bar (left to right)
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw + 3, icy - hh),
                sdl2::rect::Point::new(icx + hw - 2, icy - hh),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw + 3, icy - hh + 1),
                sdl2::rect::Point::new(icx + hw - 2, icy - hh + 1),
            );
            // Right side (down)
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw, icy - hh + 2),
                sdl2::rect::Point::new(icx + hw, icy),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw - 1, icy - hh + 2),
                sdl2::rect::Point::new(icx + hw - 1, icy),
            );
            // Bottom bar (right to left)
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw - 3, icy + hh),
                sdl2::rect::Point::new(icx - hw + 2, icy + hh),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx + hw - 3, icy + hh - 1),
                sdl2::rect::Point::new(icx - hw + 2, icy + hh - 1),
            );
            // Left side (up)
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw, icy + hh - 2),
                sdl2::rect::Point::new(icx - hw, icy),
            );
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(icx - hw - 1, icy + hh - 2),
                sdl2::rect::Point::new(icx - hw - 1, icy),
            );
            // Arrow head on top-right (pointing right)
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
            // Arrow head on bottom-left (pointing left)
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
            // Draw a left arrow (smaller than rewind)
            let _ = canvas.filled_trigon(
                icx as i16,
                (icy - sz as i32 + 2) as i16,
                (icx - sz as i32) as i16,
                icy as i16,
                icx as i16,
                (icy + sz as i32 - 2) as i16,
                c,
            );
            // Draw an underscore line
            canvas.set_draw_color(c);
            let _ = canvas.fill_rect(Rect::new(
                icx - sz as i32,
                icy + sz as i32 - 1,
                (sz * 2) as u32,
                2,
            ));
            // Draw vertical line on left (the "return to here" wall)
            let _ = canvas.fill_rect(Rect::new(
                icx - sz as i32 - 2,
                icy - sz as i32,
                2,
                (sz * 2) as u32,
            ));
        }
        ButtonIcon::None => {
            if !params.label.is_empty() {
                // Pixel-font text centred in the button using global font scale
                let scale = get_font_scale();
                let glyph_w = 4 * scale + 1; // glyph width + gap
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

// ── Small Toggle (Mute / Solo style) ─────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn toggle_button(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    x: i32,
    y: i32,
    size: i32,
    on_color: [u8; 4],
    toggled: bool,
    id: WidgetId,
    label: &str,
    hint: Option<&str>,
) -> bool {
    let hover = input.mouse_in_rect(x, y, size, size);
    let mut clicked = false;
    if hover && input.mouse_pressed && !input.consumed {
        input.active_widget = id;
        input.consume();
    }
    if hover && input.mouse_released && input.active_widget == id {
        clicked = true;
    }
    if hover {
        input.hot_widget = id;
        if let Some(h) = hint {
            input.hover_hint_text = Some(h.to_string());
        }
    }

    let bg = if toggled {
        Theme::c(on_color)
    } else if hover {
        Theme::c(theme.button_hover)
    } else {
        Theme::c(theme.button_bg)
    };

    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, size as u32, size as u32));
    canvas.set_draw_color(Theme::c(theme.panel_border));
    let _ = canvas.draw_rect(Rect::new(x, y, size as u32, size as u32));

    // Draw letter label
    if !label.is_empty() {
        let fg = if toggled {
            sdl2::pixels::Color::RGBA(10, 10, 10, 255)
        } else {
            Theme::c(theme.button_text)
        };
        let lw = (label.len() as i32 * 9).min(size - 4);
        let lx = x + (size - lw) / 2;
        let ly = y + (size - 10) / 2;
        draw_pixel_label(canvas, theme, label, lx, ly, lw + 2, fg);
    }

    clicked
}

// ── Value display ────────────────────────────────────────────────────

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

// ── Text Field ───────────────────────────────────────────────────────
//
// An immediate-mode text input field. Returns true when the value is committed (Enter/unfocus).
// The caller must supply a mutable reference to:
//   - `value`: the current string value
//   - `state`: AppState for text_field_active_id, text_field_buffer, text_field_cursor

pub struct TextFieldParams {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub hint: Option<String>,
}

/// Returns (committed: bool, new_value: Option<String>).
/// When committed=true, new_value contains the final text.
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

    // Click to focus
    if hover && input.mouse_pressed && !is_active {
        *active_id = id;
        *buffer = value.to_string();
        *cursor = buffer.len();
    }

    // Click elsewhere to commit
    if is_active && input.mouse_pressed && !hover {
        let result = buffer.clone();
        *active_id = 0;
        return (true, Some(result));
    }

    // Handle keyboard input when active
    let mut committed = false;
    let mut result_value: Option<String> = None;

    if is_active {
        use sdl2::keyboard::Keycode;

        // Handle text characters from SDL2 TextInput events
        for ch in &input.text_input_chars {
            if !ch.is_control() && *cursor <= buffer.len() {
                buffer.insert(*cursor, *ch);
                *cursor += 1;
            }
        }

        // Handle special keys
        for key in &input.keys_pressed {
            match *key {
                Keycode::Return | Keycode::KpEnter => {
                    committed = true;
                    result_value = Some(buffer.clone());
                    *active_id = 0;
                }
                Keycode::Escape => {
                    // Cancel — revert to original value
                    *active_id = 0;
                }
                Keycode::Backspace => {
                    if *cursor > 0 {
                        *cursor -= 1;
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
                        *cursor -= 1;
                    }
                }
                Keycode::Right => {
                    if *cursor < buffer.len() {
                        *cursor += 1;
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

        // Consume all keyboard input so nothing leaks to DAW shortcuts.
        // (DAW shortcuts in main.rs are also gated on text_field_active_id == 0,
        //  but clearing here prevents any widget that runs later from seeing stale keys.)
        input.keys_pressed.clear();
        input.text_input_chars.clear();
    }

    // ── Draw ──

    // Background
    let bg = if is_active {
        sdl2::pixels::Color::RGBA(40, 42, 50, 255)
    } else if hover {
        sdl2::pixels::Color::RGBA(55, 58, 68, 255)
    } else {
        sdl2::pixels::Color::RGBA(45, 48, 56, 255)
    };
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    // Border
    let border = if is_active {
        let a = theme.accent;
        sdl2::pixels::Color::RGBA(a[0], a[1], a[2], 220)
    } else {
        sdl2::pixels::Color::RGBA(70, 75, 85, 200)
    };
    canvas.set_draw_color(border);
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    // Text content
    let display_text = if is_active { buffer.as_str() } else { value };
    let text_x = x + 4;
    let text_y = y + (h - 10) / 2;
    let text_w = w - 8;

    if display_text.is_empty() && !is_active {
        // Show hint text
        if let Some(ref hint) = params.hint {
            draw_pixel_label(
                canvas,
                theme,
                hint,
                text_x,
                text_y,
                text_w,
                sdl2::pixels::Color::RGBA(100, 105, 120, 150),
            );
        }
    } else {
        let col = if is_active {
            Theme::c(theme.text_primary)
        } else {
            sdl2::pixels::Color::RGBA(190, 195, 210, 230)
        };
        draw_pixel_label(canvas, theme, display_text, text_x, text_y, text_w, col);
    }

    // Draw cursor when active
    if is_active {
        // Each character is ~9px wide (8px glyph + 1px spacing at 2x scale)
        let char_w = 9i32;
        // Compute how many characters fit in the visible area
        let visible_chars = (text_w / char_w).max(1) as usize;
        // Scroll offset: keep cursor visible by scrolling the text view
        let scroll_start = if *cursor > visible_chars {
            *cursor - visible_chars
        } else {
            0
        };
        // Re-draw the text with scroll applied so cursor stays in view
        if scroll_start > 0 {
            // Erase previously drawn text and redraw with scroll offset
            canvas.set_draw_color(if is_active {
                sdl2::pixels::Color::RGBA(40, 42, 50, 255)
            } else {
                sdl2::pixels::Color::RGBA(45, 48, 56, 255)
            });
            let _ = canvas.fill_rect(Rect::new(text_x, text_y, text_w as u32, 10));
            let scrolled = &buffer[scroll_start.min(buffer.len())..];
            let col = Theme::c(theme.text_primary);
            draw_pixel_label(canvas, theme, scrolled, text_x, text_y, text_w, col);
        }
        // Cursor pixel position relative to visible window
        let cursor_x = text_x + ((*cursor - scroll_start) as i32) * char_w;
        // Clamp strictly inside the text field (1px margin from right edge)
        let cursor_x = cursor_x.min(text_x + text_w - 1);
        let a = theme.accent;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(a[0], a[1], a[2], 255));
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(cursor_x, y + 2),
            sdl2::rect::Point::new(cursor_x, y + h - 2),
        );
    }

    // Hover hint
    if hover {
        if let Some(ref hint) = params.hint {
            if !is_active {
                input.hover_hint_text = Some(hint.clone());
            }
        }
    }

    (committed, result_value)
}

// ── Dropdown ─────────────────────────────────────────────────────────
//
// Usage:
//   let changed = dropdown(canvas, input, theme, id, x, y, w, h,
//                          &options, &mut selected_index, open_id);
//   // open_id is a shared mutable u32: 0 = closed, non-zero id = that dropdown is open.
//
// Scroll wheel over the closed dropdown cycles through options.
// Click opens/closes the popup list.

#[allow(clippy::too_many_arguments)]
pub fn dropdown(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    options: &[&str],
    selected: &mut usize,
    open_id: &mut u32,
) -> bool {
    if options.is_empty() {
        return false;
    }
    let mut changed = false;
    let is_open = *open_id == id;
    let hover = input.mouse_in_rect(x, y, w, h);

    // Scroll wheel on closed dropdown cycles options
    if hover && input.scroll_y != 0 && !is_open && !input.scroll_consumed {
        let n = options.len();
        if input.scroll_y > 0 {
            *selected = selected.saturating_sub(1);
        } else {
            *selected = (*selected + 1).min(n - 1);
        }
        input.scroll_consumed = true;
        changed = true;
    }

    // Click to toggle open — only act if the click hasn't already been consumed
    // (e.g. by the transport bar's inline closed-box handler on the same frame).
    if hover && input.mouse_pressed && !input.consumed {
        if is_open {
            *open_id = 0;
        } else {
            *open_id = id;
        }
        input.mouse_pressed = false; // consume click
    }

    // Draw closed box
    let bg = if is_open || hover {
        Theme::c(theme.button_hover)
    } else {
        Theme::c(theme.button_bg)
    };
    canvas.set_draw_color(bg);
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));
    canvas.set_draw_color(if is_open {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.panel_border)
    });
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    // Selected label (pixel blocks)
    let label = options[*selected];
    draw_pixel_label(
        canvas,
        theme,
        label,
        x + 4,
        y + (h - 10) / 2,
        w - 18,
        sdl2::pixels::Color::RGBA(220, 220, 220, 255),
    );

    // Arrow chevron on right
    let ax = x + w - 10;
    let ay = y + h / 2;
    canvas.set_draw_color(Theme::c(theme.text_secondary));
    let _ = canvas.fill_rect(Rect::new(ax, ay - 1, 7, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 1, ay + 1, 5, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 2, ay + 3, 3, 2));
    let _ = canvas.fill_rect(Rect::new(ax + 3, ay + 5, 1, 2));

    // Draw open popup list
    if is_open {
        let item_h = h;
        let popup_h = options.len() as i32 * item_h;
        let popup_y = y + h;

        // Consume any mouse activity while dropdown is open (block click-through to items below)
        let over_dropdown = input.mouse_in_rect(x, y, w, h + popup_h);
        if over_dropdown && (input.mouse_pressed || input.mouse_down) {
            input.consumed = true;
        }

        // Shadow
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(x + 2, popup_y + 2, w as u32, popup_h as u32));

        // Background
        canvas.set_draw_color(Theme::c(theme.panel_bg));
        let _ = canvas.fill_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));
        canvas.set_draw_color(Theme::c(theme.accent));
        let _ = canvas.draw_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));

        for (i, opt) in options.iter().enumerate() {
            let iy = popup_y + i as i32 * item_h;
            let item_hover = input.mouse_in_rect(x, iy, w, item_h);

            if item_hover {
                canvas.set_draw_color(Theme::c(theme.button_hover));
                let _ = canvas.fill_rect(Rect::new(x, iy, w as u32, item_h as u32));
            }
            if i == *selected {
                canvas.set_draw_color(Theme::c(theme.accent_active));
                let _ = canvas.fill_rect(Rect::new(x, iy, 3, item_h as u32));
            }

            draw_pixel_label(
                canvas,
                theme,
                opt,
                x + 6,
                iy + (item_h - 10) / 2,
                w - 10,
                if i == *selected {
                    Theme::c(theme.accent)
                } else {
                    sdl2::pixels::Color::RGBA(210, 210, 210, 255)
                },
            );

            // Click on item — consume click to prevent bleed-through
            if item_hover && input.mouse_pressed {
                *selected = i;
                *open_id = 0;
                changed = true;
                input.mouse_pressed = false;
            }
        }

        // Click outside closes — only on mouse_down (not released) to avoid false positives
        let outside = !input.mouse_in_rect(x, y, w, h + popup_h);
        if input.mouse_pressed && outside {
            *open_id = 0;
            input.mouse_pressed = false;
        }
    }

    changed
}

/// Redraw just the popup overlay for an open dropdown.
/// Call this at the very end of your draw function (after all content)
/// so the popup renders on top of everything.
#[allow(clippy::too_many_arguments)]
pub fn dropdown_popup_overlay(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    id: u32,
    x: i32,
    y: i32,
    _w_closed: i32,
    h: i32,
    w: i32,
    options: &[&str],
    selected: usize,
    open_id: u32,
    mouse_x: i32,
    mouse_y: i32,
) {
    if open_id != id || options.is_empty() {
        return;
    }
    canvas.set_clip_rect(None);
    let item_h = h;
    let popup_h = options.len() as i32 * item_h;
    let popup_y = y + h;

    // Shadow
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 100));
    let _ = canvas.fill_rect(Rect::new(x + 2, popup_y + 2, w as u32, popup_h as u32));

    // Background
    canvas.set_draw_color(Theme::c(theme.panel_bg));
    let _ = canvas.fill_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));
    canvas.set_draw_color(Theme::c(theme.accent));
    let _ = canvas.draw_rect(Rect::new(x, popup_y, w as u32, popup_h as u32));

    for (i, opt) in options.iter().enumerate() {
        let iy = popup_y + i as i32 * item_h;
        let item_hover = mouse_x >= x && mouse_x < x + w && mouse_y >= iy && mouse_y < iy + item_h;

        if item_hover {
            canvas.set_draw_color(Theme::c(theme.button_hover));
            let _ = canvas.fill_rect(Rect::new(x, iy, w as u32, item_h as u32));
        }
        if i == selected {
            canvas.set_draw_color(Theme::c(theme.accent_active));
            let _ = canvas.fill_rect(Rect::new(x, iy, 3, item_h as u32));
        }

        draw_pixel_label(
            canvas,
            theme,
            opt,
            x + 6,
            iy + (item_h - 10) / 2,
            w - 10,
            if i == selected {
                Theme::c(theme.accent)
            } else {
                sdl2::pixels::Color::RGBA(210, 210, 210, 255)
            },
        );
    }
}

// ── Number spinner ────────────────────────────────────────────────────
//
// Displays a numeric value. Scroll wheel or click+drag to change.
// Returns true when value changes.

#[allow(clippy::too_many_arguments)]
pub fn number_spinner(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: WidgetId,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    min: f64,
    max: f64,
    step: f64,
    decimals: usize,
    value: &mut f64,
) -> bool {
    let hover = input.mouse_in_rect(x, y, w, h);
    let mut changed = false;

    // Scroll wheel
    if hover && input.scroll_y != 0 && !input.scroll_consumed {
        *value = (*value + step * input.scroll_y as f64).clamp(min, max);
        input.scroll_consumed = true;
        changed = true;
    }

    // Click+drag (vertical drag changes value)
    if hover && input.mouse_pressed {
        // Shift+click → reset to default (midpoint)
        if input.shift() {
            *value = (min + max) / 2.0;
            return true;
        }
        input.active_widget = id;
        input.drag_widget = id;
        input.drag_start_value = *value;
    }
    if input.active_widget == id && input.mouse_down {
        let delta = -input.mouse_dy as f64 * step * 0.5;
        *value = (*value + delta).clamp(min, max);
        if delta.abs() > 0.0 {
            changed = true;
        }
    }
    if hover {
        input.hot_widget = id;
    }

    let is_active = input.active_widget == id;
    let is_hot = hover;

    // Background
    canvas.set_draw_color(if is_active {
        Theme::c(theme.bg_light)
    } else if is_hot {
        Theme::c(theme.button_hover)
    } else {
        Theme::c(theme.button_bg)
    });
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));
    canvas.set_draw_color(if is_active {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.panel_border)
    });
    let _ = canvas.draw_rect(Rect::new(x, y, w as u32, h as u32));

    // Value text (pixel blocks for each digit/char)
    let text = if decimals == 0 {
        format!("{:.0}", *value)
    } else {
        format!("{:.prec$}", *value, prec = decimals)
    };
    let col = if is_active {
        Theme::c(theme.accent)
    } else {
        sdl2::pixels::Color::RGBA(220, 220, 220, 255)
    };
    draw_pixel_label(canvas, theme, &text, x + 4, y + (h - 10) / 2, w - 8, col);

    // Up/down arrows on right edge — fully within widget bounds
    canvas.set_draw_color(Theme::c(theme.text_dim));
    let ax = x + w - 8; // leave 1px margin from right edge
                        // Up arrow (pointing up) — triangle inside top portion
    let mid_y_up = y + h / 4;
    let _ = canvas.fill_rect(Rect::new(ax, mid_y_up + 1, 5, 2)); // base
    let _ = canvas.fill_rect(Rect::new(ax + 1, mid_y_up - 1, 3, 2)); // middle
    let _ = canvas.fill_rect(Rect::new(ax + 2, mid_y_up - 2, 1, 1)); // tip
                                                                     // Down arrow (pointing down) — triangle inside bottom portion
    let mid_y_dn = y + h - h / 4;
    let _ = canvas.fill_rect(Rect::new(ax, mid_y_dn - 2, 5, 2)); // base
    let _ = canvas.fill_rect(Rect::new(ax + 1, mid_y_dn, 3, 2)); // middle
    let _ = canvas.fill_rect(Rect::new(ax + 2, mid_y_dn + 2, 1, 1)); // tip

    changed
}

// ── Pixel label helper ────────────────────────────────────────────────
// Draws a string as 4×5 pixel-block glyphs (no TTF needed).

pub fn draw_pixel_label(
    canvas: &mut Canvas<Window>,
    _theme: &Theme,
    text: &str,
    x: i32,
    y: i32,
    max_w: i32,
    color: sdl2::pixels::Color,
) {
    // Each glyph: 4px wide × 5px tall at base, rendered at font_scale pixels per dot
    let scale = get_font_scale();
    let gw = 4 * scale;
    let gh = 5 * scale;
    let gap = 1i32;

    canvas.set_draw_color(color);
    let mut cx = x;

    for ch in text.chars() {
        if cx + gw > x + max_w {
            break;
        }
        let rows = glyph_rows(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..4u8 {
                if bits & (1 << (3 - col)) != 0 {
                    let px = cx + col as i32 * scale;
                    let py = y + row as i32 * scale;
                    if py < y + gh {
                        let _ = canvas.fill_rect(Rect::new(px, py, scale as u32, scale as u32));
                    }
                }
            }
        }
        cx += gw + gap;
    }
}

/// Like `draw_pixel_label` but with a custom pixel scale multiplier.
#[allow(clippy::too_many_arguments)]
pub fn draw_pixel_label_scaled(
    canvas: &mut Canvas<Window>,
    _theme: &Theme,
    text: &str,
    x: i32,
    y: i32,
    max_w: i32,
    color: sdl2::pixels::Color,
    scale: i32,
) {
    let gw = 4 * scale;
    let gap = scale / 2;

    canvas.set_draw_color(color);
    let mut cx = x;

    for ch in text.chars() {
        if cx + gw > x + max_w {
            break;
        }
        let rows = glyph_rows(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..4u8 {
                if bits & (1 << (3 - col)) != 0 {
                    let px = cx + col as i32 * scale;
                    let py = y + row as i32 * scale;
                    let _ = canvas.fill_rect(Rect::new(px, py, scale as u32, scale as u32));
                }
            }
        }
        cx += gw + gap;
    }
}

/// Returns a 5-element array of bitmasks (4 bits each, MSB = leftmost pixel)
/// for a 4×5 pixel font glyph.
fn glyph_rows(ch: char) -> [u8; 5] {
    match ch {
        '0' => [0b0110, 0b1001, 0b1001, 0b1001, 0b0110],
        '1' => [0b0110, 0b0010, 0b0010, 0b0010, 0b0111],
        '2' => [0b0110, 0b0001, 0b0110, 0b1000, 0b1111],
        '3' => [0b0110, 0b0001, 0b0110, 0b0001, 0b0110],
        '4' => [0b1001, 0b1001, 0b1111, 0b0001, 0b0001],
        '5' => [0b1111, 0b1000, 0b1110, 0b0001, 0b1110],
        '6' => [0b0110, 0b1000, 0b1110, 0b1001, 0b0110],
        '7' => [0b1111, 0b0001, 0b0010, 0b0100, 0b0100],
        '8' => [0b0110, 0b1001, 0b0110, 0b1001, 0b0110],
        '9' => [0b0110, 0b1001, 0b0111, 0b0001, 0b0110],
        'A' => [0b0110, 0b1001, 0b1111, 0b1001, 0b1001],
        'B' => [0b1110, 0b1001, 0b1110, 0b1001, 0b1110],
        'C' => [0b0110, 0b1000, 0b1000, 0b1000, 0b0110],
        'D' => [0b1110, 0b1001, 0b1001, 0b1001, 0b1110],
        'E' => [0b1111, 0b1000, 0b1110, 0b1000, 0b1111],
        'F' => [0b1111, 0b1000, 0b1110, 0b1000, 0b1000],
        'G' => [0b0110, 0b1000, 0b1011, 0b1001, 0b0110],
        'H' => [0b1001, 0b1001, 0b1111, 0b1001, 0b1001],
        'I' => [0b0110, 0b0010, 0b0010, 0b0010, 0b0110],
        'J' => [0b0001, 0b0001, 0b0001, 0b1001, 0b0110],
        'K' => [0b1001, 0b1010, 0b1100, 0b1010, 0b1001],
        'L' => [0b1000, 0b1000, 0b1000, 0b1000, 0b1111],
        'M' => [0b1001, 0b1111, 0b1001, 0b1001, 0b1001],
        'N' => [0b1001, 0b1101, 0b1011, 0b1001, 0b1001],
        'O' => [0b0110, 0b1001, 0b1001, 0b1001, 0b0110],
        'P' => [0b1110, 0b1001, 0b1110, 0b1000, 0b1000],
        'Q' => [0b0110, 0b1001, 0b1011, 0b0101, 0b0111],
        'R' => [0b1110, 0b1001, 0b1110, 0b1010, 0b1001],
        'S' => [0b0111, 0b1000, 0b0110, 0b0001, 0b1110],
        'T' => [0b1110, 0b0100, 0b0100, 0b0100, 0b0100],
        'U' => [0b1001, 0b1001, 0b1001, 0b1001, 0b0110],
        'V' => [0b1001, 0b1001, 0b0110, 0b0110, 0b0100],
        'W' => [0b1001, 0b1001, 0b1001, 0b1111, 0b1001],
        'X' => [0b1001, 0b0110, 0b0110, 0b0110, 0b1001],
        'Y' => [0b1001, 0b0110, 0b0100, 0b0100, 0b0100],
        'Z' => [0b1111, 0b0001, 0b0110, 0b1000, 0b1111],
        'a' => [0b0000, 0b0110, 0b1001, 0b1001, 0b0111],
        'b' => [0b1000, 0b1110, 0b1001, 0b1001, 0b1110],
        'c' => [0b0000, 0b0110, 0b1000, 0b1000, 0b0110],
        'd' => [0b0001, 0b0111, 0b1001, 0b1001, 0b0111],
        'e' => [0b0000, 0b0110, 0b1111, 0b1000, 0b0110],
        'f' => [0b0011, 0b0100, 0b1110, 0b0100, 0b0100],
        'g' => [0b0000, 0b0111, 0b1001, 0b0111, 0b0001],
        'h' => [0b1000, 0b1110, 0b1001, 0b1001, 0b1001],
        'i' => [0b0100, 0b0000, 0b0110, 0b0100, 0b1110],
        'j' => [0b0010, 0b0000, 0b0011, 0b0010, 0b1100],
        'k' => [0b1000, 0b1001, 0b1110, 0b1001, 0b1001],
        'l' => [0b0110, 0b0010, 0b0010, 0b0010, 0b0111],
        'm' => [0b0000, 0b1111, 0b1111, 0b1001, 0b1001],
        'n' => [0b0000, 0b1110, 0b1001, 0b1001, 0b1001],
        'o' => [0b0000, 0b0110, 0b1001, 0b1001, 0b0110],
        'p' => [0b0000, 0b1110, 0b1001, 0b1110, 0b1000],
        'q' => [0b0000, 0b0110, 0b1001, 0b0111, 0b0001],
        'r' => [0b0000, 0b1011, 0b1100, 0b1000, 0b1000],
        's' => [0b0000, 0b0111, 0b1110, 0b0001, 0b1110],
        't' => [0b0100, 0b1110, 0b0100, 0b0100, 0b0011],
        'u' => [0b0000, 0b1001, 0b1001, 0b1001, 0b0111],
        'v' => [0b0000, 0b1001, 0b1001, 0b0110, 0b0110],
        'w' => [0b0000, 0b1001, 0b1111, 0b1111, 0b0110],
        'x' => [0b0000, 0b1001, 0b0110, 0b0110, 0b1001],
        'y' => [0b0000, 0b1001, 0b0111, 0b0001, 0b0110],
        'z' => [0b0000, 0b1111, 0b0010, 0b0100, 0b1111],
        '.' => [0b0000, 0b0000, 0b0000, 0b0000, 0b0100],
        ',' => [0b0000, 0b0000, 0b0000, 0b0100, 0b1000],
        ':' => [0b0000, 0b0100, 0b0000, 0b0100, 0b0000],
        ';' => [0b0000, 0b0100, 0b0000, 0b0100, 0b1000],
        '/' => [0b0001, 0b0010, 0b0100, 0b1000, 0b0000],
        '\\' => [0b1000, 0b0100, 0b0010, 0b0001, 0b0000],
        '-' => [0b0000, 0b0000, 0b1110, 0b0000, 0b0000],
        '+' => [0b0000, 0b0100, 0b1110, 0b0100, 0b0000],
        '=' => [0b0000, 0b1110, 0b0000, 0b1110, 0b0000],
        '(' => [0b0010, 0b0100, 0b0100, 0b0100, 0b0010],
        ')' => [0b0100, 0b0010, 0b0010, 0b0010, 0b0100],
        '[' => [0b0110, 0b0100, 0b0100, 0b0100, 0b0110],
        ']' => [0b0110, 0b0010, 0b0010, 0b0010, 0b0110],
        '<' => [0b0010, 0b0100, 0b1000, 0b0100, 0b0010],
        '>' => [0b0100, 0b0010, 0b0001, 0b0010, 0b0100],
        '#' => [0b1010, 0b1111, 0b1010, 0b1111, 0b1010],
        '!' => [0b0100, 0b0100, 0b0100, 0b0000, 0b0100],
        '?' => [0b0110, 0b0001, 0b0010, 0b0000, 0b0010],
        '*' => [0b1010, 0b0100, 0b1010, 0b0000, 0b0000],
        '_' => [0b0000, 0b0000, 0b0000, 0b0000, 0b1111],
        '~' => [0b0000, 0b0101, 0b1010, 0b0000, 0b0000],
        '%' => [0b1001, 0b0010, 0b0100, 0b1001, 0b0000],
        '\'' => [0b0100, 0b0100, 0b0000, 0b0000, 0b0000],
        '"' => [0b1010, 0b1010, 0b0000, 0b0000, 0b0000],
        ' ' => [0b0000, 0b0000, 0b0000, 0b0000, 0b0000],
        // ── Special symbols for DAW UI ──
        // Triangle arrows
        '▶' => [0b1000, 0b1100, 0b1110, 0b1100, 0b1000], // play/right arrow
        '◀' => [0b0010, 0b0110, 0b1110, 0b0110, 0b0010], // left arrow
        '▼' => [0b0000, 0b1111, 0b1111, 0b0110, 0b0100], // down arrow
        '▲' => [0b0100, 0b0110, 0b1111, 0b1111, 0b0000], // up arrow
        '■' => [0b1111, 0b1111, 0b1111, 0b1111, 0b1111], // stop/filled square
        '←' => [0b0010, 0b0100, 0b1111, 0b0100, 0b0010], // left arrow
        '→' => [0b0100, 0b0010, 0b1111, 0b0010, 0b0100], // right arrow
        '✕' => [0b1001, 0b0110, 0b0110, 0b0110, 0b1001], // X/close
        '✓' => [0b0000, 0b0001, 0b0010, 0b1100, 0b0000], // checkmark
        '♪' => [0b0011, 0b0010, 0b0010, 0b1100, 0b1100], // music note
        '★' => [0b0100, 0b1111, 0b0110, 0b1001, 0b0000], // filled star
        '☆' => [0b0100, 0b1111, 0b0110, 0b1001, 0b0000], // empty star (color differentiates)
        _ => [0b1111, 0b1001, 0b1001, 0b1001, 0b1111],   // fallback box
    }
}

// ── Clip handle helpers ──────────────────────────────────────────────

pub const CLIP_HANDLE_W: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipHitZone {
    None,
    LeftHandle,
    RightHandle,
    /// Top strip of the clip (height = header_h). This is the clone-drag zone.
    Header,
    Body,
}

pub fn clip_hit_test(
    input: &InputState,
    clip_x: i32,
    clip_y: i32,
    clip_w: i32,
    clip_h: i32,
    header_h: i32,
) -> ClipHitZone {
    if !input.mouse_in_rect(clip_x, clip_y, clip_w.max(4), clip_h) {
        return ClipHitZone::None;
    }
    let lx = input.mouse_x - clip_x;
    let ly = input.mouse_y - clip_y;
    // Resize handles take priority over header/body
    if clip_w > CLIP_HANDLE_W * 4 {
        if lx < CLIP_HANDLE_W {
            return ClipHitZone::LeftHandle;
        }
        if lx > clip_w - CLIP_HANDLE_W {
            return ClipHitZone::RightHandle;
        }
    }
    // Header strip (clone-drag zone)
    if ly < header_h {
        return ClipHitZone::Header;
    }
    ClipHitZone::Body
}

#[allow(clippy::too_many_arguments)]
pub fn draw_clip_handles(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    cx: i32,
    cy: i32,
    cw: i32,
    ch: i32,
    hover_zone: ClipHitZone,
    selected: bool,
) {
    let hovered = hover_zone != ClipHitZone::None;
    if !hovered && !selected {
        return;
    }
    if cw <= CLIP_HANDLE_W * 4 {
        return;
    }

    // Base handle color
    let base_hc = if selected {
        Theme::c(theme.accent)
    } else {
        sdl2::pixels::Color::RGBA(255, 255, 255, 60)
    };
    // Bright hover color for the specific side being hovered
    let hot_hc = sdl2::pixels::Color::RGBA(255, 255, 255, 200);

    let left_hc = if hover_zone == ClipHitZone::LeftHandle {
        hot_hc
    } else {
        base_hc
    };
    let right_hc = if hover_zone == ClipHitZone::RightHandle {
        hot_hc
    } else {
        base_hc
    };

    canvas.set_draw_color(left_hc);
    let _ = canvas.fill_rect(Rect::new(cx, cy, CLIP_HANDLE_W as u32, ch as u32));
    canvas.set_draw_color(right_hc);
    let _ = canvas.fill_rect(Rect::new(
        cx + cw - CLIP_HANDLE_W,
        cy,
        CLIP_HANDLE_W as u32,
        ch as u32,
    ));

    // Grip lines
    let gc = sdl2::pixels::Color::RGBA(0, 0, 0, 90);
    canvas.set_draw_color(gc);
    for i in 0..3 {
        let gy = cy + ch / 2 - 4 + i * 4;
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(cx + 1, gy),
            sdl2::rect::Point::new(cx + CLIP_HANDLE_W - 1, gy),
        );
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(cx + cw - CLIP_HANDLE_W + 1, gy),
            sdl2::rect::Point::new(cx + cw - 1, gy),
        );
    }
}

// ── Track type icon ────────────────────────────────────────────────────
// Draws a small shape representing the track type at position (x, y) in a
// box of `size` × `size` pixels.
//
// MIDI      → mini piano keys (white/black keys)
// Audio     → waveform zigzag
// Automation→ diagonal ramp curve

pub fn draw_track_type_icon(
    canvas: &mut Canvas<Window>,
    track_type: &crate::models::TrackType,
    x: i32,
    y: i32,
    size: i32,
    color: sdl2::pixels::Color,
) {
    match track_type {
        crate::models::TrackType::Midi => {
            // 4 white keys + 3 black key tops
            let kw = (size / 4).max(2);
            let kh = size - 1;
            let bh = kh * 6 / 10;
            let bw = (kw * 6 / 10).max(1);
            canvas.set_draw_color(color);
            for k in 0..4 {
                let kx = x + k * kw;
                let _ = canvas.draw_rect(Rect::new(kx, y, kw as u32, kh as u32));
            }
            // black keys between 0-1, 1-2, 2-3
            let dark = sdl2::pixels::Color::RGBA(color.r / 4, color.g / 4, color.b / 4, color.a);
            canvas.set_draw_color(dark);
            for k in 0..3 {
                let bx = x + k * kw + kw - bw / 2;
                let _ = canvas.fill_rect(Rect::new(bx, y, bw as u32, bh as u32));
            }
        }
        crate::models::TrackType::Audio => {
            // Simple waveform: alternating up/down lines
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
        crate::models::TrackType::Automation => {
            // Rising ramp line with a dot
            canvas.set_draw_color(color);
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(x, y + size - 2),
                sdl2::rect::Point::new(x + size - 2, y + 1),
            );
            // Three dots along the ramp
            for i in 0..3 {
                let t = i as f32 / 2.0;
                let dx = x + (t * (size - 2) as f32) as i32;
                let dy = y + ((1.0 - t) * (size - 2) as f32) as i32;
                let _ = canvas.fill_rect(Rect::new(dx - 1, dy - 1, 3, 3));
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// ── VU Meter Widget ──────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════
/// Draws an analog-style VU meter gauge (wide shallow arc like a real VU).
/// `x, y` = top-left of the bounding box, `w` = width, `h` = height.
/// `needle_pos` = smoothed 0.0–1.0 value (from VU ballistic state).
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
    use sdl2::gfx::primitives::DrawRenderer;
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
    // We want the arc chord to span the full inner width.
    // The arc is from a large circle, pivot (cx, cy) is far below the widget.
    // Half the chord = inner_w / 2. We choose sagitta (arc height) to be ~40% of inner_h.
    let half_chord = inner_w as f64 / 2.0;
    let sagitta = (inner_h as f64 * 0.35).max(8.0); // how tall the arc bulge is
                                                    // From chord and sagitta: radius = (half_chord^2 + sagitta^2) / (2 * sagitta)
    let arc_r = (half_chord * half_chord + sagitta * sagitta) / (2.0 * sagitta);

    // Center of the circle is below the widget
    let cx = (inner_x + inner_w / 2) as f64;
    let cy = (inner_y as f64) + sagitta + (arc_r - sagitta); // = inner_y + arc_r

    // The arc endpoints: leftmost and rightmost points of the chord
    // Angle from center to left edge: asin(half_chord / arc_r)
    // In SDL2 coords: 0°=right, 90°=down, so "up" is 270°.
    // The arc top is at angle 270° (straight up from center).
    // Left edge angle = 270° - half_angle, Right edge = 270° + half_angle
    let half_angle_rad = (half_chord / arc_r).asin();
    let half_angle_deg = half_angle_rad.to_degrees();
    // SDL2 arc: angles in degrees, screen coords (clockwise from +X)
    let start_deg = 270.0 - half_angle_deg; // left side
    let end_deg = 270.0 + half_angle_deg; // right side
    let sweep = end_deg - start_deg;

    // The arc top (at 270°) should be at the label area.
    // Labels sit above the arc, needle below.
    let label_zone_h = 9i32; // space for tiny scale labels above the arc
                             // Shift the arc down a bit so labels fit above
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

    // ── Scale markings: -20, -10, -7, -5, -3, 0, +3 dB ──
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

        // Tick mark (inward from arc)
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

        // Label outside arc (above it) — use scale=1 for tiny text
        let label_r = arc_r + 3.0;
        let lx = cx + label_r * cos_a;
        let ly = arc_cy + label_r * sin_a;
        let lbl_col = if db >= 0.0 {
            sdl2::pixels::Color::RGBA(190, 70, 50, 200)
        } else {
            sdl2::pixels::Color::RGBA(120, 128, 140, 180)
        };
        // Pixel font at scale 1 = 4px wide per char, 5px tall
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

    // "VU" label at bottom-center
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

    // We only want to draw the needle from near the arc down to a visible pivot point.
    // The visible pivot is at the bottom of the widget inner area.
    let vis_pivot_y = (inner_y + inner_h - 3) as f64;
    let vis_pivot_x = cx; // bottom center

    // ── Peak hold needle (red, slow decay) ──
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
            // Draw peak needle as two adjacent anti-aliased lines for extra thickness
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

    // But the needle line should point toward (nx, ny) from the visible pivot
    // Direction from vis_pivot to the arc point
    let dx = nx - vis_pivot_x;
    let dy = ny - vis_pivot_y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > 2.0 {
        // Shadow
        let _ = canvas.aa_line(
            vis_pivot_x as i16,
            vis_pivot_y as i16 + 1,
            nx as i16 + 1,
            ny as i16 + 1,
            sdl2::pixels::Color::RGBA(0, 0, 0, 80),
        );
        // Needle — draw two aa_lines side-by-side for a thicker, brighter look
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

// ══════════════════════════════════════════════════════════════════════
// ── Compressor Curve Widget ──────────────────────────────────────────
// ══════════════════════════════════════════════════════════════════════
/// Draws a tiny compressor knee/curve + gain reduction bar.
/// `compress` = compressor amount param (0.0–1.0),
/// `gr_db` = actual gain reduction in dB from metering (negative or zero).
/// `input_rms` = current track RMS level (linear, 0.0–1.0+) for the riding dot.
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
    use sdl2::gfx::primitives::DrawRenderer;
    if w < 10 || h < 10 {
        return;
    }

    // Background
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 20, 26, 220));
    let _ = canvas.fill_rect(Rect::new(x, y, w as u32, h as u32));

    let curve_h = (h * 2 / 3).max(6);
    let bar_h = h - curve_h - 2;

    // ── Tiny transfer curve (input dB → output dB) ──
    // Diagonal = no compression, bent = compressed
    let cx0 = x + 1;
    let cy0 = y + curve_h; // bottom-left of curve area
    let cw = w - 2;
    let ch = curve_h - 1;

    // 1:1 reference diagonal (dim)
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 55, 65, 100));
    let _ = canvas.draw_line(
        sdl2::rect::Point::new(cx0, cy0),
        sdl2::rect::Point::new(cx0 + cw, cy0 - ch),
    );

    // Compressed curve
    let ratio = 1.0 + compress * 7.0; // 1:1 → 1:8
    let thresh_frac = 1.0 - compress * 0.5; // threshold moves down with more compression
    let curve_col = sdl2::pixels::Color::RGBA(200, 140, 60, 220);
    canvas.set_draw_color(curve_col);
    let mut prev_px = cx0;
    let mut prev_py = cy0;
    for px_i in 0..=cw {
        let in_frac = px_i as f32 / cw as f32; // 0..1
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

    // ── Riding dot: shows current input level on the curve ──
    if input_rms > 1e-6 {
        // Map input RMS to 0..1 fraction (using dB scale: -60dB..0dB → 0..1)
        let in_db = 20.0 * input_rms.log10();
        let in_frac = ((in_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let out_frac = if in_frac < thresh_frac {
            in_frac
        } else {
            thresh_frac + (in_frac - thresh_frac) / ratio
        };
        let dot_px = cx0 + (in_frac * cw as f32) as i32;
        let dot_py = cy0 - (out_frac * ch as f32) as i32;
        // Bright dot
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

    // ── GR bar below the curve ──
    if bar_h > 2 {
        let bar_y = y + curve_h + 1;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(30, 32, 38, 200));
        let _ = canvas.fill_rect(Rect::new(x, bar_y, w as u32, bar_h as u32));

        // gr_db is typically negative (e.g. -6.0 means 6dB of reduction)
        // Map: 0dB → empty, -20dB → full bar
        let gr_frac = (gr_db.abs() / 20.0).clamp(0.0, 1.0);
        if gr_frac > 0.001 {
            let fill_w = (gr_frac * (w - 2) as f32) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 100, 40, 200));
            let _ = canvas.fill_rect(Rect::new(x + 1, bar_y, fill_w as u32, bar_h as u32));
        }

        // "GR" label
        draw_pixel_label(
            canvas,
            theme,
            "GR",
            x + 2,
            bar_y,
            14,
            sdl2::pixels::Color::RGBA(200, 140, 60, 140),
        );

        // dB text
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
