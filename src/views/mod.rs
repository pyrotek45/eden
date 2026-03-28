// Eden DAW — View drawing functions
// Split into logical sub-modules for maintainability.

// ── Musical gain ↔ slider-position helpers ──────────────────────────
// The gain multiplier 0.0–2.0 is mapped through a dB-aware curve so
// that the slider feels more natural: the lower half covers silence to
// unity, the upper half covers unity to +6 dB.
//
//  pos 0.0  →  gain 0.0   (−∞ dB)
//  pos 0.75 →  gain 1.0   (  0 dB)
//  pos 1.0  →  gain 2.0   ( +6 dB)
//
// We use a cubic power curve: gain = pos^3 * 2.0 (adjusted so 0.75^3*2 ≈ 0.84
// is close to unity; for exact unity at 0.75 we scale so f(0.75)=1.0).
// f(pos) = pos^3 * (1.0 / 0.75^3) when pos <= 0.75  → maps [0, 0.75] to [0, 1.0]
// f(pos) = 1.0 + (pos - 0.75) / 0.25 * 1.0          → maps [0.75, 1.0] to [1.0, 2.0]

/// Convert a slider position [0,1] to a gain multiplier [0,2].
pub(crate) fn vol_pos_to_gain(pos: f32) -> f32 {
    if pos <= 0.75 {
        // Cubic ramp from 0 to 1.0
        let t = pos / 0.75; // 0..1
        t * t * t // 0..1 gain
    } else {
        // Linear from 1.0 to 2.0
        1.0 + (pos - 0.75) / 0.25
    }
}

/// Convert a gain multiplier [0,2] to a slider position [0,1].
pub(crate) fn vol_gain_to_pos(gain: f32) -> f32 {
    if gain <= 1.0 {
        // Inverse cubic
        0.75 * gain.max(0.0).cbrt()
    } else {
        // Inverse linear
        0.75 + (gain - 1.0) * 0.25
    }
}

/// Format a gain value as a dB string for display.
pub(crate) fn gain_to_db_label(gain: f32) -> String {
    if gain < 1e-6 {
        "-∞ dB".to_string()
    } else {
        let db = 20.0 * gain.log10();
        if db.abs() < 0.05 {
            "0.0 dB".to_string()
        } else {
            format!("{:+.1} dB", db)
        }
    }
}

// ── Sub-modules ─────────────────────────────────────────────────────
mod arrangement;
mod audio_editor;
mod automation_editor;
mod bottom_panel;
mod clip_manager;
mod edit;
mod help;
mod left_panel;
mod mixer;
mod mixer_view;
mod overlays;
mod piano_roll;
mod project_manager;
mod track_headers;
mod track_lanes;
mod transport;

// ── Re-exports (public API of the views module) ────────────────────
pub use arrangement::draw_arrangement;
pub use help::draw_help_screen;
pub use project_manager::draw_project_manager;
