// Eden DAW — Knob widget

use sdl2::gfx::primitives::DrawRenderer;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::{InputState, WidgetId};
use crate::theme::Theme;
use crate::widgets::{draw_pixel_label, draw_thick_arc, inv_lerp, lerp};

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
    if hover && input.middle_mouse_down && input.middle_drag_widget == WidgetId::None {
        input.middle_drag_widget = params.id;
        input.drag_start_value = *value as f64;
    }
    if input.middle_drag_widget == params.id && input.middle_mouse_down {
        let fine_sens = params.sensitivity * 0.2;
        let delta = -input.mouse_dy as f32 * fine_sens;
        *value = (*value + delta * (params.max - params.min)).clamp(params.min, params.max);
        if delta.abs() > 0.0 {
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

    // Keep hint visible while middle-dragging
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

    // Value display below knob
    let below_y = params.y + params.radius + 3;
    if is_active || hover {
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

    // Label below the knob
    if let Some(ref label) = params.label {
        let label_scale = 1i32;
        let glyph_w = 4 * label_scale * 2 + 1;
        let label_px_w = label.len() as i32 * glyph_w;
        let label_x = params.x - label_px_w / 2;
        let label_y = if is_active || hover {
            below_y + 12
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
