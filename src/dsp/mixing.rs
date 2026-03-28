// Eden DAW — DSP audio mixing & effect processing
//
// Audio clip mixing, per-track effect chains, master effects.

use crate::engine::AudioTrack;
use crate::modules::EffectModule;

use super::CLIP_FADE_SAMPLES;

// ── Audio clip mixing ────────────────────────────────────────────────────────

/// Mix audio clips into the per-track stereo accumulator for a single track.
/// `pos_beats` is the current global position in beats.
/// `beats_per_sec` = bpm / 60.
/// `sample_rate` is the render/playback sample rate.
#[inline]
pub fn mix_audio_clips(
    track: &AudioTrack,
    pos_beats: f64,
    beats_per_sec: f64,
    sample_rate: f64,
) -> (f64, f64) {
    let mut sum_l = 0.0_f64;
    let mut sum_r = 0.0_f64;
    for aclip in &track.audio_clips {
        let clip_end = aclip.start_beats + aclip.length_beats;
        if pos_beats < aclip.start_beats || pos_beats >= clip_end {
            continue;
        }
        let clip_pos_secs = (pos_beats - aclip.start_beats) / beats_per_sec;
        let audio_pos_secs = clip_pos_secs + aclip.offset_secs;
        let src_pos = audio_pos_secs * aclip.sample_rate as f64;
        let src_idx = src_pos as usize;

        if src_idx < aclip.samples.len() {
            let s0 = aclip.samples[src_idx] as f64;
            let s1 = if src_idx + 1 < aclip.samples.len() {
                aclip.samples[src_idx + 1] as f64
            } else {
                s0
            };
            let frac = src_pos - src_pos.floor();
            let raw = s0 + (s1 - s0) * frac;
            let mut s = raw * aclip.gain as f64;

            // Equal-power micro-fade at clip boundaries
            let fade_len = CLIP_FADE_SAMPLES;
            let clip_sample = (clip_pos_secs * sample_rate) as usize;
            let clip_len_samples = (aclip.length_beats / beats_per_sec * sample_rate) as usize;
            // Fade in at clip start
            if clip_sample < fade_len {
                let t = clip_sample as f64 / fade_len as f64;
                s *= (t * std::f64::consts::FRAC_PI_2).sin();
            }
            // Fade out at clip end
            let remaining = clip_len_samples.saturating_sub(clip_sample);
            if remaining < fade_len && fade_len > 0 {
                let t = remaining as f64 / fade_len as f64;
                s *= (t * std::f64::consts::FRAC_PI_2).sin();
            }
            // User-controlled fade-in
            if aclip.fade_in > 0.0 {
                let fade_in_samples = (aclip.fade_in * sample_rate) as usize;
                if fade_in_samples > 0 && clip_sample < fade_in_samples {
                    s *= clip_sample as f64 / fade_in_samples as f64;
                }
            }
            // User-controlled fade-out
            if aclip.fade_out > 0.0 {
                let fade_out_samples = (aclip.fade_out * sample_rate) as usize;
                if fade_out_samples > 0 && clip_len_samples > fade_out_samples {
                    let fade_out_start = clip_len_samples - fade_out_samples;
                    if clip_sample >= fade_out_start {
                        s *= (clip_len_samples - clip_sample) as f64 / fade_out_samples as f64;
                    }
                }
            }

            sum_l += s;
            sum_r += s;
        }
    }
    (sum_l, sum_r)
}

// ── Per-track effect chain ───────────────────────────────────────────────────

/// Run the effect chain for a single track (effects + CStrip2).
///
/// `per_track_sample` — full per-track sample array (needed for sidechain lookup).
/// `ti` — this track's index.
/// `track_effects` — this track's effect instances.
/// `track_cstrip` — this track's CStrip2 instance.
/// `fx_params` — param slices for each effect slot (may be smoothed or raw).
/// `cstrip_params` — CStrip2 params (may be smoothed or raw).
///
/// Returns the processed (left, right) pair.
#[allow(clippy::too_many_arguments)]
pub fn run_track_effects(
    input: (f64, f64),
    voice_count: usize,
    track: &AudioTrack,
    _ti: usize,
    per_track_sample: &[(f64, f64)],
    track_effects: &mut [Box<dyn EffectModule>],
    track_cstrip: &mut Box<dyn EffectModule>,
    fx_params: &[&[(String, f32)]],
    cstrip_params: &[(String, f32)],
    bpm: f64,
    sample_rate: f64,
) -> (f64, f64) {
    let mut out = input;

    // Normalize voices before effects
    if voice_count > 0 {
        let norm = (voice_count as f64).sqrt();
        out.0 /= norm;
        out.1 /= norm;
    }

    // Process through each effect module
    for (fi, fx) in track_effects.iter_mut().enumerate() {
        fx.set_bpm(bpm);
        // Resolve sidechain source signal
        let sc_ti = track.effect_sidechain_track.get(fi).copied().flatten();
        let (key_l, key_r) = if let Some(sc_idx) = sc_ti {
            if sc_idx < per_track_sample.len() {
                per_track_sample[sc_idx]
            } else {
                out
            }
        } else {
            out
        };
        let params = if fi < fx_params.len() {
            fx_params[fi]
        } else if fi < track.effect_slots.len() {
            &track.effect_slots[fi].1
        } else {
            continue;
        };
        let (ol, or2) = fx.process_sidechain(out.0, out.1, key_l, key_r, params, sample_rate);
        out = (ol, or2);
    }

    // CStrip2 channel strip
    if !track.cstrip2_bypass && !cstrip_params.is_empty() {
        let (cl, cr) = track_cstrip.process(out.0, out.1, cstrip_params, sample_rate);
        out = (cl, cr);
    }

    out
}

// ── Master effects ───────────────────────────────────────────────────────────

/// Apply master rack effects to the stereo mix.
pub fn apply_master_effects(
    mut mix_l: f64,
    mut mix_r: f64,
    master_effects: &mut [Box<dyn EffectModule>],
    master_effect_params: &[&[(String, f32)]],
    bpm: f64,
    sample_rate: f64,
) -> (f64, f64) {
    for (fi, fx) in master_effects.iter_mut().enumerate() {
        fx.set_bpm(bpm);
        let params = if fi < master_effect_params.len() {
            master_effect_params[fi]
        } else {
            continue;
        };
        let (ml, mr) = fx.process_sidechain(mix_l, mix_r, mix_l, mix_r, params, sample_rate);
        mix_l = ml;
        mix_r = mr;
    }
    (mix_l, mix_r)
}
