// Eden DAW — Views: rack device displays
//
// One display per effect, drawn into the right-hand panel of a rack device card.
// The track rack and the master rack both call `draw_device_visual`, so a
// display is changed in exactly one place. Displays that show a transfer or
// frequency response describe what the DSP in `modules/effects/` does; where
// the response is analytic it comes from `modules::response`, which is tested
// against the real effect.

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::modules::response::{filter_cutoff_hz, reverb_early_taps, svf_magnitude_db, SvfOutput};
use crate::theme::Theme;
use crate::widgets::*;

/// Real-time values a display may animate with. Pure parameter displays ignore it.
pub(super) struct LiveSignals {
    /// Linear RMS level entering the device's channel (track or master bus).
    pub input_rms: f32,
    /// Gain reduction in dB (zero or negative) reported by the audio engine,
    /// or `None` if the engine has not published a value for this device.
    pub gr_db: Option<f32>,
    /// Whether the device is keyed from another track.
    pub sidechain: bool,
    /// Project sample rate; frequency responses are warped by it near Nyquist.
    pub sample_rate_hz: f64,
}

/// Advance of one glyph of the pixel font at the default scale (8 px glyph + 1 px gap).
const LABEL_CHAR_PX: i32 = 9;
/// Frequency range shown by filter displays, in Hz (the audible band).
const DISPLAY_MIN_HZ: f64 = 20.0;
const DISPLAY_MAX_HZ: f64 = 20_000.0;
/// Level range of the filter display. The top leaves room for a resonant peak;
/// the bottom shows roughly two octaves of 12 dB/octave roll-off.
const FILTER_DISPLAY_TOP_DB: f64 = 24.0;
const FILTER_DISPLAY_BOTTOM_DB: f64 = -48.0;
/// Spacing of horizontal level lines in the filter display.
const FILTER_DISPLAY_GRID_DB: f64 = 12.0;
/// Frequencies given a labelled vertical line, one per decade.
const FILTER_GRID_HZ: [f64; 3] = [100.0, 1_000.0, 10_000.0];
/// Opacity (0-255) of the area under the response curve.
const FILTER_FILL_ALPHA: u8 = 45;

/// Horizontal pixel offset in a display `width_px` wide for `hz` on a log axis.
pub(super) fn hz_to_x(hz: f64, width_px: i32) -> i32 {
    let t = (hz / DISPLAY_MIN_HZ).ln() / (DISPLAY_MAX_HZ / DISPLAY_MIN_HZ).ln();
    (t.clamp(0.0, 1.0) * (width_px - 1).max(0) as f64).round() as i32
}

/// Short frequency label such as "80", "1.2k" or "10k".
pub(super) fn format_hz(hz: f64) -> String {
    if hz >= 10_000.0 {
        format!("{:.0}k", hz / 1000.0)
    } else if hz >= 1_000.0 {
        format!("{:.1}k", hz / 1000.0)
    } else {
        format!("{:.0}", hz)
    }
}

fn param_or(params: &[(String, f32)], id: &str, default: f32) -> f32 {
    params
        .iter()
        .find(|(k, _)| k == id)
        .map(|(_, v)| *v)
        .unwrap_or(default)
}

/// LP/HP filter display: log-frequency magnitude response with the cutoff
/// marked. The curve comes from `svf_magnitude_db` and includes the Output gain.
fn draw_filter_display(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    area: Rect,
    output: SvfOutput,
    params: &[(String, f32)],
    sample_rate_hz: f64,
) {
    let (x0, y0) = (area.x(), area.y());
    let (w, h) = (area.width() as i32, area.height() as i32);
    let default_cutoff = if output == SvfOutput::LowPass { 0.8 } else { 0.5 };
    let cutoff_hz = filter_cutoff_hz(param_or(params, "cutoff", default_cutoff) as f64, sample_rate_hz);
    let resonance = param_or(params, "reso", 0.0) as f64;
    let output_db = param_or(params, "output_db", 0.0) as f64;
    let db_to_y = |db: f64| -> i32 {
        let t = (FILTER_DISPLAY_TOP_DB - db) / (FILTER_DISPLAY_TOP_DB - FILTER_DISPLAY_BOTTOM_DB);
        y0 + (t.clamp(0.0, 1.0) * (h - 1) as f64).round() as i32
    };

    // Grid: level lines every 12 dB (0 dB brighter), decade lines labelled.
    let mut db = FILTER_DISPLAY_TOP_DB - FILTER_DISPLAY_GRID_DB;
    while db > FILTER_DISPLAY_BOTTOM_DB {
        let alpha = if db == 0.0 { 110 } else { 45 };
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(120, 124, 140, alpha));
        let y = db_to_y(db);
        let _ = canvas.draw_line(sdl2::rect::Point::new(x0 + 1, y), sdl2::rect::Point::new(x0 + w - 2, y));
        db -= FILTER_DISPLAY_GRID_DB;
    }
    for &hz in &FILTER_GRID_HZ {
        let x = x0 + hz_to_x(hz, w);
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(120, 124, 140, 45));
        let _ = canvas.draw_line(sdl2::rect::Point::new(x, y0 + 1), sdl2::rect::Point::new(x, y0 + h - 2));
        let text = format_hz(hz);
        let text_w = text.chars().count() as i32 * LABEL_CHAR_PX;
        let text_x = (x + 2).min(x0 + w - text_w - 2);
        draw_pixel_label(canvas, theme, &text, text_x, y0 + h - 12, text_w, sdl2::pixels::Color::RGBA(130, 134, 150, 200));
    }

    // Response: translucent fill under the curve, then the curve itself.
    let accent = Theme::c(theme.accent);
    let mut prev: Option<(i32, i32)> = None;
    for px in 1..(w - 1) {
        let t = px as f64 / (w - 1) as f64;
        let hz = DISPLAY_MIN_HZ * (DISPLAY_MAX_HZ / DISPLAY_MIN_HZ).powf(t);
        let level_db = svf_magnitude_db(output, hz, cutoff_hz, resonance, sample_rate_hz) + output_db;
        let (x, y) = (x0 + px, db_to_y(level_db));
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(accent.r, accent.g, accent.b, FILTER_FILL_ALPHA));
        let _ = canvas.draw_line(sdl2::rect::Point::new(x, y), sdl2::rect::Point::new(x, y0 + h - 2));
        if let Some((px_prev, py_prev)) = prev {
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(accent.r, accent.g, accent.b, 235));
            let _ = canvas.draw_line(sdl2::rect::Point::new(px_prev, py_prev), sdl2::rect::Point::new(x, y));
        }
        prev = Some((x, y));
    }

    // Cutoff marker and value.
    let cutoff_x = x0 + hz_to_x(cutoff_hz, w);
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 90));
    let _ = canvas.draw_line(sdl2::rect::Point::new(cutoff_x, y0 + 1), sdl2::rect::Point::new(cutoff_x, y0 + h - 2));
    let label = format!("{} Hz", format_hz(cutoff_hz));
    let label_w = label.chars().count() as i32 * LABEL_CHAR_PX;
    let label_x = (cutoff_x + 3).min(x0 + w - label_w - 2).max(x0 + 2);
    draw_pixel_label(canvas, theme, &label, label_x, y0 + 3, label_w, sdl2::pixels::Color::RGBA(235, 235, 240, 230));
}


/// Longest time axis a reverb display uses, in seconds (the Decay control's maximum).
const REVERB_AXIS_MAX_S: f64 = 12.0;
/// Shortest time axis, so a very short decay still shows its pre-delay and early taps.
const REVERB_AXIS_MIN_S: f64 = 0.5;
/// The axis extends this far past the decay so the tail reaches the floor with room to spare.
const REVERB_AXIS_HEADROOM: f64 = 1.25;
/// Decay in dB the tail reaches at the RT60 time (the definition of RT60).
const RT60_DECAY_DB: f64 = 60.0;
/// Fraction of the display height used by the tallest early-reflection stem.
const EARLY_STEM_MAX_HEIGHT_FRACTION: f64 = 0.45;
/// Opacity (0-255) of the area under the late-tail decay line.
const REVERB_TAIL_FILL_ALPHA: u8 = 45;

/// Smallest gap between two time-axis ticks, in pixels: a label such as "0.5s"
/// is four glyphs wide, so this keeps neighbouring labels from touching.
const MIN_TICK_SPACING_PX: f64 = 56.0;

/// Tick spacing in seconds for a time axis spanning `axis_s` seconds across
/// `width_px` pixels: the finest round step whose ticks are far enough apart to label.
pub(super) fn time_axis_step_s(axis_s: f64, width_px: i32) -> f64 {
    const STEPS_S: [f64; 8] = [0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0];
    STEPS_S
        .iter()
        .copied()
        .find(|step| step / axis_s * width_px as f64 >= MIN_TICK_SPACING_PX)
        .unwrap_or(STEPS_S[STEPS_S.len() - 1])
}

/// Reverb display: level against time after an impulse. Shows the pre-delay,
/// the early reflections at their real tap times (`reverb_early_taps`) and the
/// late tail as a line that falls 60 dB over the Decay (RT60) time, which is how
/// `FxReverb::process_late` sets its comb feedback.
fn draw_reverb_display(canvas: &mut Canvas<Window>, theme: &Theme, area: Rect, params: &[(String, f32)]) {
    let (x0, y0) = (area.x(), area.y());
    let (w, h) = (area.width() as i32, area.height() as i32);
    let predelay_s = param_or(params, "predelay", 14.0) as f64 / 1000.0;
    let decay_s = param_or(params, "decay", 1.6) as f64;
    let size = param_or(params, "size", 24.0) as f64;
    let axis_s = (decay_s * REVERB_AXIS_HEADROOM + predelay_s).clamp(REVERB_AXIS_MIN_S, REVERB_AXIS_MAX_S);
    let t_to_x = |t: f64| x0 + ((t / axis_s).clamp(0.0, 1.0) * (w - 1) as f64).round() as i32;
    let db_to_y = |db: f64| y0 + ((-db / RT60_DECAY_DB).clamp(0.0, 1.0) * (h - 1) as f64).round() as i32;
    let grid = sdl2::pixels::Color::RGBA(120, 124, 140, 45);
    let label_color = sdl2::pixels::Color::RGBA(130, 134, 150, 200);

    // Level lines every 20 dB and time ticks.
    for db in [-20.0, -40.0] {
        canvas.set_draw_color(grid);
        let _ = canvas.draw_line(sdl2::rect::Point::new(x0 + 1, db_to_y(db)), sdl2::rect::Point::new(x0 + w - 2, db_to_y(db)));
    }
    let step = time_axis_step_s(axis_s, w);
    let mut tick = step;
    while tick < axis_s {
        let x = t_to_x(tick);
        canvas.set_draw_color(grid);
        let _ = canvas.draw_line(sdl2::rect::Point::new(x, y0 + 1), sdl2::rect::Point::new(x, y0 + h - 2));
        let text = if step < 1.0 { format!("{:.1}s", tick) } else { format!("{:.0}s", tick) };
        let text_w = text.chars().count() as i32 * LABEL_CHAR_PX;
        draw_pixel_label(canvas, theme, &text, (x + 2).min(x0 + w - text_w - 2), y0 + h - 12, text_w, label_color);
        tick += step;
    }

    // Late tail: 0 dB at the end of the pre-delay, -60 dB one RT60 later.
    let accent = Theme::c(theme.accent);
    let tail_start_x = t_to_x(predelay_s);
    let tail_end_x = t_to_x(predelay_s + decay_s);
    let (tail_start_y, tail_end_y) = (db_to_y(0.0), db_to_y(-RT60_DECAY_DB));
    for x in tail_start_x..=tail_end_x.min(x0 + w - 2) {
        let t = if tail_end_x > tail_start_x { (x - tail_start_x) as f64 / (tail_end_x - tail_start_x) as f64 } else { 1.0 };
        let y = tail_start_y + (t * (tail_end_y - tail_start_y) as f64).round() as i32;
        canvas.set_draw_color(sdl2::pixels::Color::RGBA(accent.r, accent.g, accent.b, REVERB_TAIL_FILL_ALPHA));
        let _ = canvas.draw_line(sdl2::rect::Point::new(x, y), sdl2::rect::Point::new(x, y0 + h - 2));
    }
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(accent.r, accent.g, accent.b, 235));
    let _ = canvas.draw_line(sdl2::rect::Point::new(tail_start_x, tail_start_y), sdl2::rect::Point::new(tail_end_x, tail_end_y));

    // Early reflections, drawn as stems rising from the floor.
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(235, 235, 240, 200));
    for tap in reverb_early_taps(size) {
        let x = t_to_x(predelay_s + tap.time_ms / 1000.0);
        let stem = (tap.relative_level * EARLY_STEM_MAX_HEIGHT_FRACTION * (h - 1) as f64).round() as i32;
        let _ = canvas.draw_line(sdl2::rect::Point::new(x, y0 + h - 2), sdl2::rect::Point::new(x, y0 + h - 2 - stem));
    }

    // Pre-delay marker and RT60 readout.
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 90));
    let _ = canvas.draw_line(sdl2::rect::Point::new(tail_start_x, y0 + 1), sdl2::rect::Point::new(tail_start_x, y0 + h - 2));
    let label = format!("RT60 {:.1}s", decay_s);
    let label_w = label.chars().count() as i32 * LABEL_CHAR_PX;
    draw_pixel_label(canvas, theme, &label, x0 + w - label_w - 3, y0 + 3, label_w, sdl2::pixels::Color::RGBA(235, 235, 240, 230));
}

/// Draw the display for `plugin_ref` into `area` (background and border included).
pub(super) fn draw_device_visual(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    plugin_ref: &str,
    params_snap: &[(String, f32)],
    area: Rect,
    live: &LiveSignals,
) {
    let vis_x = area.x();
    let vis_y = area.y();
    let vis_w = area.width();
    let vis_h = area.height() as i32;

    canvas.set_draw_color(sdl2::pixels::Color::RGBA(18, 18, 22, 255));
    let _ = canvas.fill_rect(area);
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(40, 42, 50, 200));
    let _ = canvas.draw_rect(area);

    match plugin_ref {
        "LP Filter" | "HP Filter" => draw_filter_display(
            canvas,
            theme,
            area,
            if plugin_ref == "LP Filter" { SvfOutput::LowPass } else { SvfOutput::HighPass },
            params_snap,
            live.sample_rate_hz,
        ),

        "Reverb" => draw_reverb_display(canvas, theme, area, params_snap),

        "Compressor" => {
            // Static compression curve on top half
            let threshold = params_snap
                .iter()
                .find(|(k, _)| k == "threshold")
                .map(|(_, v)| *v)
                .unwrap_or(-18.0);
            let ratio = params_snap
                .iter()
                .find(|(k, _)| k == "ratio")
                .map(|(_, v)| *v)
                .unwrap_or(4.0);
            let knee_db = params_snap
                .iter()
                .find(|(k, _)| k == "knee")
                .map(|(_, v)| *v)
                .unwrap_or(6.0);
            let accent = Theme::c(theme.accent);

            // Curve shows -60..0 dBFS on both axes
            let curve_h = (vis_h * 2 / 3).max(20);
            let w_f = vis_w as f32;
            let h_f = curve_h as f32;
            let thresh_db = threshold; // already in dBFS
            let comp_ratio = ratio.max(1.0);

            // Draw unity diagonal (faint)
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 100));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(vis_x, vis_y + curve_h),
                sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
            );

            // Draw compression curve
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                accent.r, accent.g, accent.b, 200,
            ));
            let slope = 1.0 - 1.0 / comp_ratio;
            let half_knee = knee_db * 0.5;
            let mut prev_y_px = 0i32;
            for px in 0..vis_w as i32 {
                // Map pixel to dB input (-60..0)
                let in_db = -60.0 + (px as f32 / w_f) * 60.0;
                let over = in_db - thresh_db;
                let gr = if over <= -half_knee {
                    0.0_f32
                } else if over >= half_knee {
                    -slope * over
                } else {
                    let x = over + half_knee;
                    let t = x / knee_db.max(0.01);
                    -slope * knee_db * t * t * 0.5
                };
                let out_db = in_db + gr;
                let y_norm = 1.0 - (out_db + 60.0) / 60.0;
                let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                if px > 0 {
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                        sdl2::rect::Point::new(vis_x + px, y_px),
                    );
                }
                prev_y_px = y_px;
            }
            // Threshold line
            let thresh_x_norm = (thresh_db + 60.0) / 60.0;
            let thresh_x = vis_x + (thresh_x_norm * w_f) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 80, 150));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(thresh_x, vis_y),
                sdl2::rect::Point::new(thresh_x, vis_y + curve_h),
            );

            // ── Input + GR meters on bottom third ──
            let meter_y = vis_y + curve_h + 2;
            let meter_h = (vis_h - curve_h - 4).max(4);
            let track_rms_pre: f32 = live.input_rms;

            let in_db_rms = if track_rms_pre > 1e-6 {
                20.0 * track_rms_pre.log10()
            } else {
                -60.0_f32
            };
            // Use actual gain reduction from audio engine
            let gr_db_live = live.gr_db.unwrap_or(0.0).min(0.0);

            // ── Real-time dot on the compression curve ──
            // Shows current audio level riding along the curve.
            {
                let dot_in_db = in_db_rms;
                let dot_over = dot_in_db - thresh_db;
                let dot_gr = if dot_over <= -half_knee {
                    0.0_f32
                } else if dot_over >= half_knee {
                    -slope * dot_over
                } else {
                    let x = dot_over + half_knee;
                    let t = x / knee_db.max(0.01);
                    -slope * knee_db * t * t * 0.5
                };
                let dot_out_db = dot_in_db + dot_gr;
                let dot_x_norm = (dot_in_db + 60.0) / 60.0;
                let dot_y_norm = 1.0 - (dot_out_db + 60.0) / 60.0;
                let dot_px = vis_x + (dot_x_norm * w_f).clamp(0.0, w_f - 1.0) as i32;
                let dot_py = vis_y + (dot_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                // Draw a filled circle (radius 3) as the dot
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 100, 240));
                for dy in -3i32..=3 {
                    for dx in -3i32..=3 {
                        if dx * dx + dy * dy <= 9 {
                            let _ = canvas.draw_point(sdl2::rect::Point::new(
                                dot_px + dx,
                                dot_py + dy,
                            ));
                        }
                    }
                }
            }

            // Draw input level bar
            let in_meter_w = (vis_w as i32 - 4) / 2 - 1;
            let in_frac = ((in_db_rms + 60.0) / 60.0).clamp(0.0, 1.0);
            let in_fill_w = (in_frac * in_meter_w as f32) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
            let _ = canvas.fill_rect(Rect::new(
                vis_x + 2,
                meter_y,
                in_meter_w as u32,
                meter_h as u32,
            ));
            if in_fill_w > 0 {
                let col = if in_frac > 0.85 {
                    sdl2::pixels::Color::RGBA(220, 60, 60, 230)
                } else if in_frac > 0.6 {
                    sdl2::pixels::Color::RGBA(200, 180, 50, 230)
                } else {
                    sdl2::pixels::Color::RGBA(60, 180, 80, 230)
                };
                canvas.set_draw_color(col);
                let _ = canvas.fill_rect(Rect::new(
                    vis_x + 2,
                    meter_y,
                    in_fill_w as u32,
                    meter_h as u32,
                ));
            }
            draw_pixel_label(
                canvas,
                theme,
                "IN",
                vis_x + 2,
                meter_y,
                20,
                sdl2::pixels::Color::RGBA(140, 200, 140, 160),
            );

            // Draw GR bar (fills from right, as GR increases)
            let gr_meter_x = vis_x + 2 + in_meter_w + 2;
            let gr_meter_w = vis_w as i32 - 4 - in_meter_w - 2;
            let gr_frac = ((-gr_db_live) / 24.0).clamp(0.0, 1.0);
            let gr_fill_w = (gr_frac * gr_meter_w as f32) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
            let _ = canvas.fill_rect(Rect::new(
                gr_meter_x,
                meter_y,
                gr_meter_w as u32,
                meter_h as u32,
            ));
            if gr_fill_w > 0 {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 60, 220));
                let _ = canvas.fill_rect(Rect::new(
                    gr_meter_x,
                    meter_y,
                    gr_fill_w as u32,
                    meter_h as u32,
                ));
            }
            draw_pixel_label(
                canvas,
                theme,
                "GR",
                gr_meter_x + 2,
                meter_y,
                20,
                sdl2::pixels::Color::RGBA(180, 100, 100, 160),
            );

            // Sidechain indicator
            let sc_active = live.sidechain;
            if sc_active {
                draw_pixel_label(
                    canvas,
                    theme,
                    "SC",
                    vis_x + vis_w as i32 - 18,
                    meter_y + 1,
                    14,
                    sdl2::pixels::Color::RGBA(120, 220, 255, 220),
                );
            }
        }

        "EQ" => {
            let lo_gain_db = params_snap
                .iter()
                .find(|(k, _)| k == "lo_gain")
                .map(|(_, v)| *v as f64)
                .unwrap_or(0.0);
            let mid_gain_db = params_snap
                .iter()
                .find(|(k, _)| k == "mid_gain")
                .map(|(_, v)| *v as f64)
                .unwrap_or(0.0);
            let hi_gain_db = params_snap
                .iter()
                .find(|(k, _)| k == "hi_gain")
                .map(|(_, v)| *v as f64)
                .unwrap_or(0.0);
            let lo_freq = params_snap
                .iter()
                .find(|(k, _)| k == "lo_freq")
                .map(|(_, v)| *v as f64)
                .unwrap_or(200.0);
            let mid_freq = params_snap
                .iter()
                .find(|(k, _)| k == "mid_freq")
                .map(|(_, v)| *v as f64)
                .unwrap_or(1000.0);
            let hi_freq = params_snap
                .iter()
                .find(|(k, _)| k == "hi_freq")
                .map(|(_, v)| *v as f64)
                .unwrap_or(4000.0);
            let sr = 44100.0_f64;
            let lo_c = crate::modules::BiquadCoeffs::low_shelf(lo_freq, lo_gain_db, sr);
            let mid_c =
                crate::modules::BiquadCoeffs::peaking(mid_freq, mid_gain_db, 0.7, sr);
            let hi_c = crate::modules::BiquadCoeffs::high_shelf(hi_freq, hi_gain_db, sr);

            let db_range = 18.0_f64;
            let log_min = (20.0_f64).ln();
            let log_max = (20000.0_f64).ln();
            let w_i = vis_w as i32;
            let h_f = vis_h as f64;
            let mid_line = vis_y + vis_h / 2;

            // Build pixel-per-column Y values
            let mut curve_py: Vec<i32> = Vec::with_capacity(w_i as usize);
            for col in 0..w_i {
                let t = col as f64 / (w_i - 1).max(1) as f64;
                let freq = (log_min + t * (log_max - log_min)).exp();
                let omega = 2.0 * std::f64::consts::PI * freq / sr;
                let mag = lo_c.magnitude_at(omega)
                    * mid_c.magnitude_at(omega)
                    * hi_c.magnitude_at(omega);
                let db = if mag > 1e-10 {
                    20.0 * mag.log10()
                } else {
                    -db_range
                };
                let db_clamped = db.clamp(-db_range, db_range);
                let py = mid_line - (db_clamped / db_range * (h_f / 2.0)) as i32;
                curve_py.push(py.clamp(vis_y, vis_y + vis_h - 1));
            }

            // 0 dB reference line
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 120));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(vis_x, mid_line),
                sdl2::rect::Point::new(vis_x + w_i, mid_line),
            );

            // Filled area between curve and centre
            let accent = Theme::c(theme.accent);
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                accent.r, accent.g, accent.b, 30,
            ));
            for col in 0..w_i {
                let px = vis_x + col;
                let py = curve_py[col as usize];
                let (ya, yb) = if py <= mid_line {
                    (py, mid_line)
                } else {
                    (mid_line, py)
                };
                let fill_h = (yb - ya + 1).max(1) as u32;
                let _ = canvas.fill_rect(Rect::new(px, ya, 1, fill_h));
            }

            // Curve line
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                accent.r, accent.g, accent.b, 200,
            ));
            for col in 1..w_i {
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(vis_x + col - 1, curve_py[(col - 1) as usize]),
                    sdl2::rect::Point::new(vis_x + col, curve_py[col as usize]),
                );
            }
        }

        "Distortion" => {
            let drive = params_snap
                .iter()
                .find(|(k, _)| k == "drive")
                .map(|(_, v)| *v)
                .unwrap_or(1.0);
            let accent = Theme::c(theme.accent);
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                accent.r, accent.g, accent.b, 200,
            ));
            let w_f = vis_w as f32;
            let h_f = vis_h as f32;
            let mut prev_y_px = 0i32;
            for px in 0..vis_w as i32 {
                let inp = (px as f32 / w_f) * 2.0 - 1.0;
                let driven = inp * (1.0 + drive * 9.0);
                let output = driven.tanh();
                let y_norm = 0.5 - output * 0.45;
                let y_px = vis_y + (y_norm * h_f) as i32;
                if px > 0 {
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                        sdl2::rect::Point::new(vis_x + px, y_px),
                    );
                }
                prev_y_px = y_px;
            }
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 100));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(vis_x, vis_y + vis_h),
                sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
            );
        }

        "Delay" => {
            let div_l = params_snap
                .iter()
                .find(|(k, _)| k == "time_l")
                .map(|(_, v)| v.round() as usize)
                .unwrap_or(5);
            let div_r = params_snap
                .iter()
                .find(|(k, _)| k == "time_r")
                .map(|(_, v)| v.round() as usize)
                .unwrap_or(3);
            let feedback = params_snap
                .iter()
                .find(|(k, _)| k == "feedback")
                .map(|(_, v)| *v)
                .unwrap_or(0.3);
            let mix = params_snap
                .iter()
                .find(|(k, _)| k == "mix")
                .map(|(_, v)| *v)
                .unwrap_or(0.5);
            let w_f = vis_w as f32;
            let h_f = vis_h as f32;
            let half_h = (h_f * 0.5) as i32;
            let accent = Theme::c(theme.accent);
            // Beat values for each division index (same order as DELAY_DIVISIONS)
            let beat_vals: [f32; 10] = [
                4.0,
                2.0,
                4.0 / 3.0,
                1.0,
                2.0 / 3.0,
                0.5,
                1.0 / 3.0,
                0.25,
                0.5 / 3.0,
                0.125,
            ];
            // Draw L taps on top half
            {
                let beats = beat_vals.get(div_l).copied().unwrap_or(0.5);
                let tap_spacing =
                    ((beats / 4.0) * w_f * 0.9).max(4.0).min(w_f * 0.45) as i32;
                let tap_w = (tap_spacing / 3).clamp(2, 12);
                let mut tap_x = vis_x + 2;
                let mut level = mix;
                for _ in 0..12 {
                    if level < 0.02 || tap_x + tap_w > vis_x + vis_w as i32 - 1 {
                        break;
                    }
                    let bar_h = (level * half_h as f32 * 0.85) as i32;
                    let bar_y = vis_y + half_h - bar_h;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        accent.r,
                        accent.g,
                        accent.b,
                        (level * 220.0) as u8,
                    ));
                    let _ = canvas.fill_rect(Rect::new(
                        tap_x,
                        bar_y,
                        tap_w as u32,
                        bar_h as u32,
                    ));
                    tap_x += tap_spacing;
                    level *= feedback;
                }
            }
            // Draw R taps on bottom half
            {
                let beats = beat_vals.get(div_r).copied().unwrap_or(1.0);
                let tap_spacing =
                    ((beats / 4.0) * w_f * 0.9).max(4.0).min(w_f * 0.45) as i32;
                let tap_w = (tap_spacing / 3).clamp(2, 12);
                let mut tap_x = vis_x + 2;
                let mut level = mix;
                for _ in 0..12 {
                    if level < 0.02 || tap_x + tap_w > vis_x + vis_w as i32 - 1 {
                        break;
                    }
                    let bar_h = (level * half_h as f32 * 0.85) as i32;
                    let bar_y = vis_y + half_h;
                    canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                        100,
                        accent.g,
                        accent.b,
                        (level * 200.0) as u8,
                    ));
                    let _ = canvas.fill_rect(Rect::new(
                        tap_x,
                        bar_y,
                        tap_w as u32,
                        bar_h as u32,
                    ));
                    tap_x += tap_spacing;
                    level *= feedback;
                }
            }
            // Centre divider
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(60, 60, 70, 80));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(vis_x, vis_y + half_h),
                sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y + half_h),
            );
        }

        "Limiter" => {
            let gain_db = params_snap
                .iter()
                .find(|(k, _)| k == "gain_db")
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            let ceiling_db = params_snap
                .iter()
                .find(|(k, _)| k == "ceiling_db")
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            let accent = Theme::c(theme.accent);

            // ── Transfer curve (upper 2/3) ──
            // X axis = input dB (-60..0), Y axis = output dB (-60..ceiling)
            let curve_h = (vis_h as f32 * 0.65) as i32;
            let w_f = vis_w as f32;
            let h_f = curve_h as f32;

            // Reference line (unity, no processing)
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(50, 50, 60, 120));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(vis_x, vis_y + curve_h),
                sdl2::rect::Point::new(vis_x + vis_w as i32, vis_y),
            );

            // Draw the limiter transfer curve
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                accent.r, accent.g, accent.b, 220,
            ));
            let mut prev_y_px = 0i32;
            for px in 0..vis_w as i32 {
                // Map pixel to input dB: 0..w → -60..0
                let in_db_c = -60.0 + (px as f32 / w_f) * 60.0;
                // Apply input gain
                let after_gain = in_db_c + gain_db;
                // Limiter: hard ceiling
                let out_db_c = after_gain.min(ceiling_db);
                // Map output dB to Y: -60dB = bottom, 0dB = top
                let y_norm = 1.0 - (out_db_c + 60.0) / 60.0;
                let y_px = vis_y + (y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                if px > 0 {
                    let _ = canvas.draw_line(
                        sdl2::rect::Point::new(vis_x + px - 1, prev_y_px),
                        sdl2::rect::Point::new(vis_x + px, y_px),
                    );
                }
                prev_y_px = y_px;
            }

            // Ceiling horizontal reference line
            let ceil_y_norm = 1.0 - (ceiling_db + 60.0) / 60.0;
            let ceil_y = vis_y + (ceil_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 90, 70, 160));
            let _ = canvas.draw_line(
                sdl2::rect::Point::new(vis_x, ceil_y),
                sdl2::rect::Point::new(vis_x + vis_w as i32, ceil_y),
            );

            // ── Real-time dot on the limiter transfer curve ──
            {
                let input_rms_dot: f32 = live.input_rms;
                let dot_in_db = if input_rms_dot > 1e-6 {
                    20.0 * input_rms_dot.log10()
                } else {
                    -60.0_f32
                };
                let dot_after_gain = dot_in_db + gain_db;
                let dot_out_db = dot_after_gain.min(ceiling_db);
                let dot_x_norm = (dot_in_db + 60.0) / 60.0;
                let dot_y_norm = 1.0 - (dot_out_db + 60.0) / 60.0;
                let dot_px = vis_x + (dot_x_norm * w_f).clamp(0.0, w_f - 1.0) as i32;
                let dot_py = vis_y + (dot_y_norm * h_f).clamp(0.0, h_f - 1.0) as i32;
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 100, 240));
                for dy in -3i32..=3 {
                    for dx in -3i32..=3 {
                        if dx * dx + dy * dy <= 9 {
                            let _ = canvas.draw_point(sdl2::rect::Point::new(
                                dot_px + dx,
                                dot_py + dy,
                            ));
                        }
                    }
                }
            }

            // ── Meters on bottom third (IN, GR) ──
            let meter_y = vis_y + curve_h + 3;
            let meter_h = (vis_h - curve_h - 6).max(4);

            let input_rms: f32 = live.input_rms;
            let in_db_live = if input_rms > 1e-6 {
                20.0 * input_rms.log10()
            } else {
                -60.0_f32
            };
            // Use actual gain reduction from audio engine
            // Without an engine reading (e.g. the device was just added) estimate
            // the reduction from the static curve so the meter still moves.
            let gr_db = live
                .gr_db
                .unwrap_or_else(|| {
                    let after_gain_db = in_db_live + gain_db;
                    if after_gain_db > ceiling_db {
                        ceiling_db - after_gain_db
                    } else {
                        0.0
                    }
                })
                .min(0.0);

            // Input level bar (horizontal)
            let in_meter_w = (vis_w as i32 - 4) / 2 - 1;
            let in_frac = ((in_db_live + 60.0) / 60.0).clamp(0.0, 1.0);
            let in_fill_w = (in_frac * in_meter_w as f32) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
            let _ = canvas.fill_rect(Rect::new(
                vis_x + 2,
                meter_y,
                in_meter_w as u32,
                meter_h as u32,
            ));
            if in_fill_w > 0 {
                let col = if in_frac > 0.85 {
                    sdl2::pixels::Color::RGBA(220, 60, 60, 230)
                } else if in_frac > 0.6 {
                    sdl2::pixels::Color::RGBA(200, 180, 50, 230)
                } else {
                    sdl2::pixels::Color::RGBA(60, 180, 80, 230)
                };
                canvas.set_draw_color(col);
                let _ = canvas.fill_rect(Rect::new(
                    vis_x + 2,
                    meter_y,
                    in_fill_w as u32,
                    meter_h as u32,
                ));
            }
            draw_pixel_label(
                canvas,
                theme,
                "IN",
                vis_x + 2,
                meter_y,
                20,
                sdl2::pixels::Color::RGBA(140, 200, 140, 160),
            );

            // GR bar (fills from right as GR increases)
            let gr_meter_x = vis_x + 2 + in_meter_w + 2;
            let gr_meter_w = vis_w as i32 - 4 - in_meter_w - 2;
            let gr_frac = ((-gr_db) / 24.0).clamp(0.0, 1.0);
            let gr_fill_w = (gr_frac * gr_meter_w as f32) as i32;
            canvas.set_draw_color(sdl2::pixels::Color::RGBA(25, 25, 30, 220));
            let _ = canvas.fill_rect(Rect::new(
                gr_meter_x,
                meter_y,
                gr_meter_w as u32,
                meter_h as u32,
            ));
            if gr_fill_w > 0 {
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(200, 80, 60, 220));
                let _ = canvas.fill_rect(Rect::new(
                    gr_meter_x,
                    meter_y,
                    gr_fill_w as u32,
                    meter_h as u32,
                ));
            }
            draw_pixel_label(
                canvas,
                theme,
                "GR",
                gr_meter_x + 2,
                meter_y,
                20,
                sdl2::pixels::Color::RGBA(180, 100, 100, 160),
            );

            // GR dB readout next to GR label
            if gr_db < -0.1 {
                let gr_text = format!("{:.1}dB", gr_db);
                draw_pixel_label(
                    canvas,
                    theme,
                    &gr_text,
                    gr_meter_x + 18,
                    meter_y,
                    gr_meter_w - 20,
                    sdl2::pixels::Color::RGBA(200, 100, 80, 200),
                );
            }

            // Ceiling label on curve
            let ceil_label = format!("C:{:.1}", ceiling_db);
            draw_pixel_label(
                canvas,
                theme,
                &ceil_label,
                vis_x + vis_w as i32 - 36,
                ceil_y - 10,
                34,
                sdl2::pixels::Color::RGBA(255, 100, 80, 180),
            );
        }

        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_axis_spans_the_whole_display_width() {
        let width_px = 200;
        assert_eq!(hz_to_x(DISPLAY_MIN_HZ, width_px), 0);
        assert_eq!(hz_to_x(DISPLAY_MAX_HZ, width_px), width_px - 1);
        // 632 Hz is the geometric midpoint of 20 Hz..20 kHz.
        let mid = hz_to_x(632.455, width_px);
        assert!((mid - (width_px - 1) / 2).abs() <= 1, "mid={}", mid);
        // Out-of-band frequencies are clamped rather than drawn off the display.
        assert_eq!(hz_to_x(5.0, width_px), 0);
        assert_eq!(hz_to_x(40_000.0, width_px), width_px - 1);
    }

    #[test]
    fn frequency_labels_are_short() {
        assert_eq!(format_hz(80.0), "80");
        assert_eq!(format_hz(1200.0), "1.2k");
        assert_eq!(format_hz(10_000.0), "10k");
    }

    #[test]
    fn time_axis_ticks_leave_room_for_their_labels() {
        let width_px = 220;
        for axis_s in [0.5, 1.0, 3.75, 7.5, 12.0] {
            let step = time_axis_step_s(axis_s, width_px);
            let spacing_px = step / axis_s * width_px as f64;
            assert!(spacing_px >= MIN_TICK_SPACING_PX, "axis {axis_s}s: ticks {spacing_px:.0}px apart");
        }
        // A short axis uses fine steps; a long one uses coarse steps.
        assert!(time_axis_step_s(0.5, width_px) < time_axis_step_s(12.0, width_px));
    }
}
