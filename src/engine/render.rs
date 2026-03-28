// Eden DAW — Offline render engine
// Renders the entire project (or loop region) to a stereo WAV file.
//
// ╔══════════════════════════════════════════════════════════════════╗
// ║  This file is a THIN WRAPPER around the shared DSP pipeline     ║
// ║  in dsp.rs. All audio processing logic lives there — this       ║
// ║  file only handles: file I/O, progress reporting, render range  ║
// ║  calculation, and WAV encoding. NO DSP code is duplicated here. ║
// ╚══════════════════════════════════════════════════════════════════╝
//
// Audio clips always play at NATIVE speed regardless of BPM.
// BPM only controls where beats fall on the timeline — it does NOT time-stretch
// audio files. (Time-stretch is a separate, explicit operation.)

use crate::dsp;
use crate::models::*;
use crate::modules::{voice_is_done, ModuleVoice};

// ── Render settings ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RenderSettings {
    pub master_volume: f32,
    /// 0 = 44100, 1 = 48000, 2 = 96000
    pub sample_rate_idx: usize,
    /// 0 = 16-bit, 1 = 24-bit, 2 = 32-bit float
    pub bit_depth_idx: usize,
    /// If true, only render the loop region (loop_start..loop_end)
    pub loop_only: bool,
    pub loop_start_beats: f64,
    pub loop_end_beats: f64,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            sample_rate_idx: 0,
            bit_depth_idx: 0,
            loop_only: false,
            loop_start_beats: 0.0,
            loop_end_beats: 8.0,
        }
    }
}

// ── In-memory render (for tests) ─────────────────────────────────────────────

/// Render a project to an in-memory buffer of stereo sample pairs.
/// Uses the exact same DSP pipeline as render_to_wav and audio.rs playback.
pub fn render_to_buffer(
    project: &Project,
    sample_rate: u32,
    master_volume: f32,
) -> Vec<(f64, f64)> {
    let bpm = project.tempo_map.bpm_at(0.0);
    let start_beats = 0.0;
    let end_beats = dsp::song_length_beats(project);

    let render_secs = (end_beats - start_beats) * 60.0 / bpm;
    let total_samples = (render_secs * sample_rate as f64).ceil() as usize;
    if total_samples == 0 {
        return Vec::new();
    }

    let audio_cache = dsp::preload_audio_cache(project);
    let mut render_tracks = dsp::build_render_tracks(project, &audio_cache);
    let master_effect_slots = dsp::build_master_effect_slots(project);
    let mut instances =
        dsp::create_render_instances(&render_tracks, &master_effect_slots, sample_rate);

    let any_solo = render_tracks.iter().any(|t| t.solo && !t.is_automation);
    let num_tracks = render_tracks.len();
    let mut track_voices: Vec<Vec<ModuleVoice>> = (0..num_tracks).map(|_| Vec::new()).collect();

    let beats_per_sample = bpm / 60.0 / sample_rate as f64;
    let beats_per_sec = bpm / 60.0;
    let mut pos_beats = start_beats;
    let mut dc_hp = dsp::DcHpState::new();

    // Pre-compute automation mapping
    let track_id_to_render_idx: std::collections::HashMap<u32, usize> = project
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id, i))
        .collect();

    let mut output = Vec::with_capacity(total_samples);

    for _si in 0..total_samples {
        let mut per_track_sample = vec![(0.0_f64, 0.0_f64); num_tracks];
        let mut per_track_voices = vec![0usize; num_tracks];

        // Evaluate and apply automation
        let auto_vals = dsp::evaluate_automation_at(project, pos_beats);
        dsp::apply_automation_to_tracks(
            &auto_vals,
            &mut render_tracks,
            project,
            &track_id_to_render_idx,
        );

        // Trigger/release voices and synthesize per track
        for ti in 0..num_tracks {
            let track = &render_tracks[ti];
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }

            // Voice triggering
            let midi_effects = &mut instances.track_midi_effects[ti];
            let arp_state = &mut instances.track_arp_state[ti];
            dsp::trigger_and_release_voices(
                track,
                ti,
                &mut track_voices[ti],
                midi_effects,
                arp_state,
                pos_beats,
                beats_per_sample,
                bpm,
                sample_rate as f64,
            );

            // Synthesize voices
            if let Some(ref instrument) = instances.track_instruments[ti] {
                let (sl, sr, vc) = dsp::synthesize_voices(
                    &mut track_voices[ti],
                    instrument.as_ref(),
                    &track.instrument_params,
                    sample_rate as f64,
                    &track.extra,
                );
                per_track_sample[ti].0 += sl;
                per_track_sample[ti].1 += sr;
                per_track_voices[ti] += vc;
            }

            // Mix audio clips
            let (al, ar) =
                dsp::mix_audio_clips(track, pos_beats, beats_per_sec, sample_rate as f64);
            per_track_sample[ti].0 += al;
            per_track_sample[ti].1 += ar;
        }

        // Run effect chains per track
        for ti in 0..num_tracks {
            let track = &render_tracks[ti];
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            if per_track_sample[ti] == (0.0, 0.0) && per_track_voices[ti] == 0 {
                let has_tail = instances.track_effects[ti].iter().any(|fx| fx.has_tail());
                if !has_tail {
                    continue;
                }
            }
            // Build raw param refs for this track's effects
            let fx_params: Vec<&[(String, f32)]> = track
                .effect_slots
                .iter()
                .map(|(_, p)| p.as_slice())
                .collect();
            let cstrip_params = track.cstrip2_params.as_slice();

            per_track_sample[ti] = dsp::run_track_effects(
                per_track_sample[ti],
                per_track_voices[ti],
                track,
                ti,
                &per_track_sample,
                &mut instances.track_effects[ti],
                &mut instances.track_cstrip[ti],
                &fx_params,
                cstrip_params,
                bpm,
                sample_rate as f64,
            );
        }

        // Stereo mix with equal-power pan
        let mut mix_l = 0.0_f64;
        let mut mix_r = 0.0_f64;
        for (ti, track) in render_tracks.iter().enumerate() {
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            let (tl, tr) =
                dsp::pan_and_mix(per_track_sample[ti], track.pan as f64, track.volume as f64);
            mix_l += tl;
            mix_r += tr;
        }

        // Master rack effects
        let master_fx_params: Vec<&[(String, f32)]> = master_effect_slots
            .iter()
            .map(|(_, p)| p.as_slice())
            .collect();
        let (ml, mr) = dsp::apply_master_effects(
            mix_l,
            mix_r,
            &mut instances.master_effects,
            &master_fx_params,
            bpm,
            sample_rate as f64,
        );
        mix_l = ml * master_volume as f64;
        mix_r = mr * master_volume as f64;

        // DC-offset removal
        let (fl, fr) = dc_hp.process(mix_l, mix_r);
        mix_l = fl;
        mix_r = fr;

        // Denormal prevention
        mix_l += 1.0e-24;
        mix_r += 1.0e-24;

        output.push((mix_l, mix_r));
        pos_beats += beats_per_sample;
    }

    output
}

// ── WAV file render ──────────────────────────────────────────────────────────

pub fn render_to_wav(
    project: &Project,
    path: &str,
    settings: &RenderSettings,
) -> Result<(), String> {
    render_to_wav_with_progress(project, path, settings, None)
}

pub fn render_to_wav_with_progress(
    project: &Project,
    path: &str,
    settings: &RenderSettings,
    progress: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
) -> Result<(), String> {
    let sr_values = [44100u32, 48000, 96000];
    let sample_rate = sr_values[settings.sample_rate_idx.min(2)];
    let bpm = project.tempo_map.bpm_at(0.0);
    let master_volume = settings.master_volume;

    // Determine render range
    let (start_beats, end_beats) = if settings.loop_only {
        let s = settings.loop_start_beats;
        let e = settings.loop_end_beats.max(s + 0.25);
        (s, e)
    } else {
        (0.0, dsp::song_length_beats(project))
    };

    let render_secs = (end_beats - start_beats) * 60.0 / bpm;
    const RENDER_TAIL_SECS: f64 = 2.0;
    let base_samples = (render_secs * sample_rate as f64).ceil() as usize;
    let tail_samples = (RENDER_TAIL_SECS * sample_rate as f64) as usize;
    let total_samples = base_samples + tail_samples;
    if total_samples == 0 {
        return Err("Nothing to render (zero-length region)".into());
    }

    // Build shared DSP state
    let audio_cache = dsp::preload_audio_cache(project);
    let mut render_tracks = dsp::build_render_tracks(project, &audio_cache);
    let master_effect_slots = dsp::build_master_effect_slots(project);
    let mut instances =
        dsp::create_render_instances(&render_tracks, &master_effect_slots, sample_rate);

    let any_solo = render_tracks.iter().any(|t| t.solo && !t.is_automation);
    let num_tracks = render_tracks.len();
    let mut track_voices: Vec<Vec<ModuleVoice>> = (0..num_tracks).map(|_| Vec::new()).collect();

    // Pre-compute automation mapping
    let track_id_to_render_idx: std::collections::HashMap<u32, usize> = project
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id, i))
        .collect();

    // Open WAV writer
    let (bits_per_sample, sample_format) = match settings.bit_depth_idx {
        1 => (24u16, hound::SampleFormat::Int),
        2 => (32u16, hound::SampleFormat::Float),
        _ => (16u16, hound::SampleFormat::Int),
    };
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample,
        sample_format,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("WAV create error: {}", e))?;

    let beats_per_sample = bpm / 60.0 / sample_rate as f64;
    let beats_per_sec = bpm / 60.0;
    let mut pos_beats = start_beats;
    let mut dc_hp = dsp::DcHpState::new();

    // Export fade-in / fade-out lengths
    let export_fade_in_len = (0.002 * sample_rate as f64) as usize;
    let export_fade_out_len = (0.005 * sample_rate as f64) as usize;
    let fade_out_start = total_samples.saturating_sub(export_fade_out_len);

    for _si in 0..total_samples {
        // Report progress
        if let Some(ref prog) = progress {
            if _si & 0xFFF == 0 {
                let permille = (_si as u64 * 1000 / total_samples as u64) as u32;
                prog.store(permille, std::sync::atomic::Ordering::Relaxed);
            }
        }

        let mut per_track_sample = vec![(0.0_f64, 0.0_f64); num_tracks];
        let mut per_track_voices = vec![0usize; num_tracks];

        // Evaluate and apply automation
        let auto_vals = dsp::evaluate_automation_at(project, pos_beats);
        dsp::apply_automation_to_tracks(
            &auto_vals,
            &mut render_tracks,
            project,
            &track_id_to_render_idx,
        );

        // Trigger/release voices and synthesize per track
        for ti in 0..num_tracks {
            let track = &render_tracks[ti];
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }

            let midi_effects = &mut instances.track_midi_effects[ti];
            let arp_state = &mut instances.track_arp_state[ti];
            dsp::trigger_and_release_voices(
                track,
                ti,
                &mut track_voices[ti],
                midi_effects,
                arp_state,
                pos_beats,
                beats_per_sample,
                bpm,
                sample_rate as f64,
            );

            if let Some(ref instrument) = instances.track_instruments[ti] {
                let (sl, sr, vc) = dsp::synthesize_voices(
                    &mut track_voices[ti],
                    instrument.as_ref(),
                    &track.instrument_params,
                    sample_rate as f64,
                    &track.extra,
                );
                per_track_sample[ti].0 += sl;
                per_track_sample[ti].1 += sr;
                per_track_voices[ti] += vc;
            }

            let (al, ar) =
                dsp::mix_audio_clips(track, pos_beats, beats_per_sec, sample_rate as f64);
            per_track_sample[ti].0 += al;
            per_track_sample[ti].1 += ar;
        }

        // Run effect chains per track
        for ti in 0..num_tracks {
            let track = &render_tracks[ti];
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            if per_track_sample[ti] == (0.0, 0.0) && per_track_voices[ti] == 0 {
                let has_tail = instances.track_effects[ti].iter().any(|fx| fx.has_tail());
                if !has_tail {
                    continue;
                }
            }
            let fx_params: Vec<&[(String, f32)]> = track
                .effect_slots
                .iter()
                .map(|(_, p)| p.as_slice())
                .collect();
            let cstrip_params = track.cstrip2_params.as_slice();

            per_track_sample[ti] = dsp::run_track_effects(
                per_track_sample[ti],
                per_track_voices[ti],
                track,
                ti,
                &per_track_sample,
                &mut instances.track_effects[ti],
                &mut instances.track_cstrip[ti],
                &fx_params,
                cstrip_params,
                bpm,
                sample_rate as f64,
            );
        }

        // Stereo mix
        let mut mix_l = 0.0_f64;
        let mut mix_r = 0.0_f64;
        for (ti, track) in render_tracks.iter().enumerate() {
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            let (tl, tr) =
                dsp::pan_and_mix(per_track_sample[ti], track.pan as f64, track.volume as f64);
            mix_l += tl;
            mix_r += tr;
        }

        // Master effects
        let master_fx_params: Vec<&[(String, f32)]> = master_effect_slots
            .iter()
            .map(|(_, p)| p.as_slice())
            .collect();
        let (ml, mr) = dsp::apply_master_effects(
            mix_l,
            mix_r,
            &mut instances.master_effects,
            &master_fx_params,
            bpm,
            sample_rate as f64,
        );
        mix_l = ml * master_volume as f64;
        mix_r = mr * master_volume as f64;

        // DC-offset removal
        let (fl, fr) = dc_hp.process(mix_l, mix_r);
        mix_l = fl;
        mix_r = fr;

        // Denormal prevention
        mix_l += 1.0e-24;
        mix_r += 1.0e-24;

        // Export fade-in (equal-power sine, ~2 ms)
        if _si < export_fade_in_len && export_fade_in_len > 0 {
            let t = _si as f64 / export_fade_in_len as f64;
            let gain = (t * std::f64::consts::FRAC_PI_2).sin();
            mix_l *= gain;
            mix_r *= gain;
        }
        // Export fade-out (equal-power sine, ~5 ms)
        if _si >= fade_out_start && export_fade_out_len > 0 {
            let elapsed = _si - fade_out_start;
            let t = 1.0 - (elapsed as f64 / export_fade_out_len as f64);
            let gain = (t * std::f64::consts::FRAC_PI_2).sin();
            mix_l *= gain;
            mix_r *= gain;
        }

        write_sample(&mut writer, mix_l, mix_r, settings.bit_depth_idx)?;
        pos_beats += beats_per_sample;
    }

    // ── Release all remaining voices and render tail ──
    if !settings.loop_only {
        for voices in track_voices.iter_mut() {
            for v in voices.iter_mut() {
                if !v.released {
                    v.released = true;
                }
            }
        }
        let max_tail_samples = (10.0 * sample_rate as f64) as usize;
        let silence_threshold = 1e-5;
        let silence_required = (0.25 * sample_rate as f64) as usize;
        let mut consecutive_silent = 0usize;

        for _tail in 0..max_tail_samples {
            let any_alive = track_voices.iter().any(|voices| !voices.is_empty());
            if !any_alive {
                break;
            }
            for voices in track_voices.iter_mut() {
                voices.retain(|v| !voice_is_done(v));
            }
            if track_voices.iter().all(|voices| voices.is_empty()) {
                break;
            }

            let mut per_track_sample = vec![(0.0_f64, 0.0_f64); num_tracks];
            for (ti, voices) in track_voices.iter_mut().enumerate() {
                let track = &render_tracks[ti];
                if let Some(ref instrument) = instances.track_instruments[ti] {
                    let (sl, sr, vc) = dsp::synthesize_voices(
                        voices,
                        instrument.as_ref(),
                        &track.instrument_params,
                        sample_rate as f64,
                        &track.extra,
                    );
                    per_track_sample[ti].0 += sl;
                    per_track_sample[ti].1 += sr;

                    // Normalize + run effects
                    let fx_params: Vec<&[(String, f32)]> = track
                        .effect_slots
                        .iter()
                        .map(|(_, p)| p.as_slice())
                        .collect();
                    let cstrip_params = track.cstrip2_params.as_slice();
                    per_track_sample[ti] = dsp::run_track_effects(
                        per_track_sample[ti],
                        vc,
                        track,
                        ti,
                        &per_track_sample,
                        &mut instances.track_effects[ti],
                        &mut instances.track_cstrip[ti],
                        &fx_params,
                        cstrip_params,
                        bpm,
                        sample_rate as f64,
                    );
                }
            }

            // Mix
            let mut mix_l = 0.0_f64;
            let mut mix_r = 0.0_f64;
            for (ti, track) in render_tracks.iter().enumerate() {
                if track.is_automation || track.mute {
                    continue;
                }
                if any_solo && !track.solo {
                    continue;
                }
                let (tl, tr) =
                    dsp::pan_and_mix(per_track_sample[ti], track.pan as f64, track.volume as f64);
                mix_l += tl;
                mix_r += tr;
            }

            let master_fx_params: Vec<&[(String, f32)]> = master_effect_slots
                .iter()
                .map(|(_, p)| p.as_slice())
                .collect();
            let (ml, mr) = dsp::apply_master_effects(
                mix_l,
                mix_r,
                &mut instances.master_effects,
                &master_fx_params,
                bpm,
                sample_rate as f64,
            );
            mix_l = ml * master_volume as f64;
            mix_r = mr * master_volume as f64;

            if mix_l.abs() < silence_threshold && mix_r.abs() < silence_threshold {
                consecutive_silent += 1;
                if consecutive_silent >= silence_required {
                    break;
                }
            } else {
                consecutive_silent = 0;
            }

            write_sample(&mut writer, mix_l, mix_r, settings.bit_depth_idx)?;
        }
    }

    // Signal completion
    if let Some(ref prog) = progress {
        prog.store(1000, std::sync::atomic::Ordering::Relaxed);
    }

    writer
        .finalize()
        .map_err(|e| format!("Finalize error: {}", e))?;

    let range_desc = if settings.loop_only {
        format!("loop {:.2}–{:.2} beats", start_beats, end_beats)
    } else {
        format!("full song ({:.2} beats)", end_beats)
    };
    println!(
        "[render] Exported (stereo) {} @ {}Hz to {}",
        range_desc, sample_rate, path
    );
    Ok(())
}

/// Write a stereo sample pair to the WAV writer at the configured bit depth.
fn write_sample(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    mix_l: f64,
    mix_r: f64,
    bit_depth_idx: usize,
) -> Result<(), String> {
    match bit_depth_idx {
        2 => {
            writer
                .write_sample(mix_l.clamp(-1.0, 1.0) as f32)
                .map_err(|e| format!("Write: {}", e))?;
            writer
                .write_sample(mix_r.clamp(-1.0, 1.0) as f32)
                .map_err(|e| format!("Write: {}", e))?;
        }
        1 => {
            writer
                .write_sample((mix_l.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
                .map_err(|e| format!("Write: {}", e))?;
            writer
                .write_sample((mix_r.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
                .map_err(|e| format!("Write: {}", e))?;
        }
        _ => {
            writer
                .write_sample((mix_l.clamp(-1.0, 1.0) * 32767.0) as i16)
                .map_err(|e| format!("Write: {}", e))?;
            writer
                .write_sample((mix_r.clamp(-1.0, 1.0) * 32767.0) as i16)
                .map_err(|e| format!("Write: {}", e))?;
        }
    }
    Ok(())
}
