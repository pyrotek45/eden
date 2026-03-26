// Eden DAW — Offline render engine
// Renders the entire project (or loop region) to a stereo WAV file.
//
// ╔══════════════════════════════════════════════════════════════════╗
// ║  INVARIANT: render == playback                                  ║
// ║  The render pipeline MUST be identical to the realtime audio    ║
// ║  engine (audio.rs + main.rs). Every feature that affects sound  ║
// ║  in playback (automation, filter, panning, gain, mute/solo)     ║
// ║  MUST also be applied here. If you add something to playback,   ║
// ║  add it here too, and vice versa.                               ║
// ╚══════════════════════════════════════════════════════════════════╝
//
// Audio clips always play at NATIVE speed regardless of BPM.
// BPM only controls where beats fall on the timeline — it does NOT time-stretch
// audio files. (Time-stretch is a separate, explicit operation.)

use crate::audio::{AudioMidiClip, AudioNote, AudioSampleClip, AudioTrack};
use crate::models::*;
use crate::modules::{
    create_effect, create_instrument, create_midi_effect, voice_is_done, EffectModule,
    InstrumentModule, MidiContext, MidiEffect, MidiEvent, ModuleExtra, ModuleVoice,
};
use std::sync::Arc;

// ── MIDI effect chain helper ─────────────────────────────────────────────────

fn run_midi_chain<'a>(
    mut events: Vec<MidiEvent>,
    chain: &'a mut [Box<dyn MidiEffect>],
    param_slices: impl Iterator<Item = &'a Vec<(String, f32)>>,
    pos_beats: f64,
    prev_beats: f64,
    bpm: f64,
    sample_rate: f64,
) -> Vec<MidiEvent> {
    for (fx, params) in chain.iter_mut().zip(param_slices) {
        let ctx = MidiContext {
            pos_beats,
            prev_beats,
            bpm,
            sample_rate,
            params: params.as_slice(),
        };
        events = fx.process(events, &ctx);
    }
    events
}

// ── Automation evaluation (MUST mirror main.rs apply-automation logic) ─────
//
// Given the project and a beat position, returns a map of
// "track_id:slot_id:param_id" → interpolated value for every automation
// clip that is active at that position.
//
// This is a pure function — it does not mutate project state.
// The render loop calls this per-sample (or per block) to apply automation
// exactly as the realtime engine does via main.rs.
fn evaluate_automation_at(
    project: &Project,
    pos_beats: f64,
) -> std::collections::HashMap<String, f32> {
    let mut result = std::collections::HashMap::new();
    for track in project
        .tracks
        .iter()
        .filter(|t| t.track_type == TrackType::Automation && t.automation_enabled)
    {
        for clip in &track.clips {
            if let Clip::Automation(ac) = clip {
                let clip_end = ac.start_time + ac.length;
                if pos_beats < ac.start_time || pos_beats > clip_end {
                    continue;
                }
                let clip_pos = pos_beats - ac.start_time;
                if ac.points.is_empty() {
                    continue;
                }
                let value = if ac.points.len() == 1 {
                    ac.points[0].value
                } else {
                    let mut before = &ac.points[0];
                    let mut after = &ac.points[ac.points.len() - 1];
                    for i in 0..ac.points.len().saturating_sub(1) {
                        if ac.points[i].time <= clip_pos && ac.points[i + 1].time >= clip_pos {
                            before = &ac.points[i];
                            after = &ac.points[i + 1];
                            break;
                        }
                    }
                    let dt = after.time - before.time;
                    if dt <= 0.0 {
                        before.value
                    } else {
                        let t = ((clip_pos - before.time) / dt).clamp(0.0, 1.0) as f32;
                        before.value + t * (after.value - before.value)
                    }
                };
                result.insert(ac.target_param.clone(), value);
            }
        }
    }
    result
}

/// Render settings passed from the UI.
pub struct RenderSettings {
    pub master_volume: f32,
    /// 0 = 44100, 1 = 48000, 2 = 96000
    pub sample_rate_idx: usize,
    /// 0 = 16-bit PCM, 1 = 24-bit PCM, 2 = 32-bit float
    pub bit_depth_idx: usize,
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

/// Render a project to an in-memory buffer of stereo sample pairs.
/// This mirrors the render pipeline exactly and is used for tests.
pub fn render_to_buffer(
    project: &Project,
    sample_rate: u32,
    master_volume: f32,
) -> Vec<(f64, f64)> {
    let bpm = project.tempo_map.bpm_at(0.0);

    let song_length_beats = project
        .tracks
        .iter()
        .filter(|t| t.track_type != TrackType::Automation)
        .flat_map(|t| t.clips.iter())
        .map(|c| c.start_time() + c.length())
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let start_beats = 0.0;
    let end_beats = song_length_beats;

    let render_secs = (end_beats - start_beats) * 60.0 / bpm;
    let total_samples = (render_secs * sample_rate as f64).ceil() as usize;
    if total_samples == 0 {
        return Vec::new();
    }

    // Build AudioTrack snapshot (same as render_to_wav_with_progress)
    let mut render_tracks: Vec<AudioTrack> = Vec::new();
    for track in &project.tracks {
        let instrument_module: Option<String> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled)
            .find(|slot| crate::modules::is_instrument(&slot.plugin_name))
            .map(|slot| slot.plugin_name.clone());

        let instrument_params: Vec<(String, f32)> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled)
            .find(|slot| crate::modules::is_instrument(&slot.plugin_name))
            .map(|slot| {
                slot.params
                    .iter()
                    .map(|p| (p.id.clone(), p.value))
                    .collect()
            })
            .unwrap_or_default();

        let mut midi_clips: Vec<AudioMidiClip> = Vec::new();
        if track.track_type == TrackType::Midi {
            for clip in &track.clips {
                if let Clip::Midi(mc) = clip {
                    let notes = mc
                        .notes
                        .iter()
                        .map(|n| AudioNote {
                            pitch: n.pitch,
                            velocity: n.velocity,
                            start_beats: n.start,
                            length_beats: n.length,
                        })
                        .collect();
                    midi_clips.push(AudioMidiClip {
                        start_beats: mc.start_time,
                        length_beats: mc.length,
                        notes,
                    });
                }
            }
        }

        let effect_slots: Vec<(String, Vec<(String, f32)>)> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled && crate::modules::is_effect(&slot.plugin_name))
            .map(|slot| {
                let params: Vec<(String, f32)> = slot
                    .params
                    .iter()
                    .map(|p| (p.id.clone(), p.value))
                    .collect();
                (slot.plugin_name.clone(), params)
            })
            .collect();

        let effect_sidechain_track: Vec<Option<usize>> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled && crate::modules::is_effect(&slot.plugin_name))
            .map(|slot| {
                slot.sidechain_track_id
                    .and_then(|sc_id| project.tracks.iter().position(|t| t.id == sc_id))
            })
            .collect();

        // Collect MIDI effect slots from rack (same as main.rs)
        let midi_effect_slots: Vec<(String, Vec<(String, f32)>)> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled && crate::modules::is_midi_effect(&slot.plugin_name))
            .map(|slot| {
                let params: Vec<(String, f32)> = slot
                    .params
                    .iter()
                    .map(|p| (p.id.clone(), p.value))
                    .collect();
                (slot.plugin_name.clone(), params)
            })
            .collect();

        render_tracks.push(AudioTrack {
            volume: track.volume,
            pan: track.pan,
            mute: track.mute,
            solo: track.solo,
            is_automation: track.track_type == TrackType::Automation,
            midi_clips,
            audio_clips: Vec::new(),
            instrument_module,
            instrument_params,
            effect_slots,
            midi_effect_slots,
            effect_sidechain_track,
            cstrip2_params: track.cstrip2_params.clone(),
            cstrip2_bypass: track.cstrip2_bypass,
            extra: ModuleExtra::default(),
        });
    }

    let any_solo = render_tracks.iter().any(|t| t.solo && !t.is_automation);

    fn midi_to_freq(pitch: u8) -> f64 {
        440.0 * crate::modules::fast_pow2((pitch as f64 - 69.0) / 12.0)
    }

    let num_tracks = render_tracks.len();
    let mut track_voices: Vec<Vec<ModuleVoice>> = (0..num_tracks).map(|_| Vec::new()).collect();

    let track_instruments: Vec<Option<Box<dyn InstrumentModule>>> = render_tracks
        .iter()
        .map(|t| t.instrument_module.as_deref().and_then(create_instrument))
        .collect();
    let mut track_effects: Vec<Vec<Box<dyn EffectModule>>> = render_tracks
        .iter()
        .map(|t| {
            t.effect_slots
                .iter()
                .filter_map(|(name, _)| create_effect(name, sample_rate))
                .collect()
        })
        .collect();
    let mut track_cstrip_r: Vec<Box<dyn EffectModule>> = render_tracks
        .iter()
        .map(|_| create_effect("CStrip2", sample_rate).unwrap())
        .collect();
    let mut track_midi_effects: Vec<Vec<Box<dyn MidiEffect>>> = render_tracks
        .iter()
        .map(|t| {
            t.midi_effect_slots
                .iter()
                .filter_map(|(name, _)| create_midi_effect(name))
                .collect()
        })
        .collect();
    let mut track_arp_state: Vec<(usize, f64)> = vec![(0, -999.0); render_tracks.len()];

    let master_effect_slots: Vec<(String, Vec<(String, f32)>)> = project
        .master_rack
        .iter()
        .filter(|s| s.enabled && crate::modules::is_effect(&s.plugin_name))
        .map(|s| {
            let params: Vec<(String, f32)> =
                s.params.iter().map(|p| (p.id.clone(), p.value)).collect();
            (s.plugin_name.clone(), params)
        })
        .collect();
    let mut master_effects: Vec<Box<dyn EffectModule>> = master_effect_slots
        .iter()
        .filter_map(|(name, _)| create_effect(name, sample_rate))
        .collect();

    let beats_per_sample = bpm / 60.0 / sample_rate as f64;
    let mut pos_beats = start_beats;

    // DC-offset HP filter state (fc ≈ 20 Hz, mirrors render_to_wav_with_progress)
    const DC_HP_R_BUF: f64 = 0.99972;
    let mut dc_hp_x_l_b = 0.0_f64;
    let mut dc_hp_y_l_b = 0.0_f64;
    let mut dc_hp_x_r_b = 0.0_f64;
    let mut dc_hp_y_r_b = 0.0_f64;

    // Slew rate limiter state for render_to_buffer
    let mut slew_prev_l_b = 0.0_f64;
    let mut slew_prev_r_b = 0.0_f64;

    let mut output = Vec::with_capacity(total_samples);

    for _si in 0..total_samples {
        let mut per_track_sample = vec![(0.0_f64, 0.0_f64); num_tracks];
        let mut per_track_voices = vec![0usize; num_tracks];

        // Trigger new voices + release expired ones
        for (ti, track) in render_tracks.iter().enumerate() {
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            if track.instrument_module.is_none() {
                continue;
            }

            for clip in &track.midi_clips {
                let clip_end = clip.start_beats + clip.length_beats;
                let prev_beats = pos_beats - beats_per_sample;
                if prev_beats < clip_end && pos_beats >= clip_end {
                    for note in &clip.notes {
                        for v in track_voices[ti].iter_mut() {
                            if v.pitch == note.pitch && !v.released {
                                v.released = true;
                                break;
                            }
                        }
                    }
                }
                if pos_beats < clip.start_beats || pos_beats >= clip_end {
                    continue;
                }
                let clip_pos = pos_beats - clip.start_beats;
                let prev = clip_pos - beats_per_sample;

                for note in &clip.notes {
                    let note_end = note.start_beats + note.length_beats;
                    if prev < note.start_beats && clip_pos >= note.start_beats {
                        let vel = note.velocity as f32 / 127.0 * track.volume;
                        let seed = vec![MidiEvent::new(note.pitch, vel)];

                        let has_arp = ti < track_midi_effects.len()
                            && track_midi_effects[ti].iter().any(|m| m.manages_voices());

                        if has_arp {
                            // Arp: accumulate held notes; step logic fires below
                        } else {
                            let final_events = if ti < track_midi_effects.len() {
                                run_midi_chain(
                                    seed,
                                    &mut track_midi_effects[ti],
                                    track.midi_effect_slots.iter().map(|(_, p)| p),
                                    pos_beats,
                                    pos_beats - beats_per_sample,
                                    bpm,
                                    sample_rate as f64,
                                )
                            } else {
                                vec![MidiEvent::new(note.pitch, vel)]
                            };
                            for ev in final_events {
                                if !track_voices[ti]
                                    .iter()
                                    .any(|v| v.pitch == ev.pitch && !v.released)
                                {
                                    let freq = midi_to_freq(ev.pitch);
                                    let mut voice =
                                        ModuleVoice::new(freq, ev.velocity, ti, ev.pitch);
                                    voice.original_pitch = note.pitch;
                                    track_voices[ti].push(voice);
                                }
                            }
                        }
                    }
                    if prev < note_end && clip_pos >= note_end {
                        for v in track_voices[ti].iter_mut() {
                            if v.original_pitch == note.pitch && !v.released {
                                v.released = true;
                                break;
                            }
                        }
                    }
                }
            }

            // ── Arp step trigger ──
            let has_arp_instance = ti < track_midi_effects.len()
                && track_midi_effects[ti].iter().any(|m| m.manages_voices());
            if has_arp_instance {
                if let Some((_, arp_params)) = track
                    .midi_effect_slots
                    .iter()
                    .find(|(n, _)| n == "Arpeggiator")
                {
                    let get_arp = |k: &str, d: f32| -> f32 {
                        arp_params
                            .iter()
                            .find(|(id, _)| id == k)
                            .map(|(_, v)| *v)
                            .unwrap_or(d)
                    };
                    let rate_beats = get_arp("rate", 0.25) as f64;
                    let octaves = get_arp("octaves", 1.0) as i32;
                    let pattern = get_arp("pattern", 0.0) as i32;
                    let vel_default = track.volume;

                    let (ref mut step, ref mut last_beat) = track_arp_state[ti];

                    let mut active_pitches: Vec<u8> = Vec::new();
                    for clip in &track.midi_clips {
                        let clip_end = clip.start_beats + clip.length_beats;
                        if pos_beats < clip.start_beats || pos_beats >= clip_end {
                            continue;
                        }
                        let cp = pos_beats - clip.start_beats;
                        for note in &clip.notes {
                            if cp >= note.start_beats
                                && cp < note.start_beats + note.length_beats
                                && !active_pitches.contains(&note.pitch)
                            {
                                active_pitches.push(note.pitch);
                            }
                        }
                    }
                    active_pitches.sort_unstable();

                    let mut pool: Vec<u8> = Vec::new();
                    for oct in 0..octaves {
                        for &p in &active_pitches {
                            pool.push((p as i32 + oct * 12).clamp(0, 127) as u8);
                        }
                    }
                    match pattern {
                        1 => pool.reverse(),
                        2 => {
                            let mut d = pool.clone();
                            d.reverse();
                            if d.len() > 1 {
                                pool.extend_from_slice(&d[1..d.len() - 1]);
                            }
                        }
                        3 => {
                            let seed = (pos_beats * 1000.0) as u64;
                            for i in (1..pool.len()).rev() {
                                let j = (seed
                                    .wrapping_mul(6364136223846793005)
                                    .wrapping_add(1442695040888963407)
                                    >> 33) as usize
                                    % (i + 1);
                                pool.swap(i, j);
                            }
                        }
                        _ => {}
                    }

                    if !pool.is_empty() {
                        let fire = if *last_beat < 0.0 {
                            true
                        } else {
                            (pos_beats / rate_beats).floor() as usize
                                > (*last_beat / rate_beats).floor() as usize
                        };
                        if fire {
                            for v in track_voices[ti].iter_mut() {
                                if !v.released {
                                    v.released = true;
                                }
                            }
                            let idx = *step % pool.len();
                            let pitch = pool[idx];
                            let mut voice =
                                ModuleVoice::new(midi_to_freq(pitch), vel_default, ti, pitch);
                            voice.original_pitch = pitch;
                            track_voices[ti].push(voice);
                            *step = (*step + 1) % pool.len().max(1);
                            *last_beat = pos_beats;
                        }
                    } else {
                        for v in track_voices[ti].iter_mut() {
                            if !v.released {
                                v.released = true;
                            }
                        }
                        *step = 0;
                        *last_beat = -999.0;
                    }
                }
            }

            for v in track_voices[ti].iter_mut() {
                if v.released {
                    continue;
                }
                let has_arp = ti < track_midi_effects.len()
                    && track_midi_effects[ti].iter().any(|m| m.manages_voices());
                if has_arp {
                    continue;
                }
                let mut still_active = false;
                for clip in &track.midi_clips {
                    let clip_end = clip.start_beats + clip.length_beats;
                    if pos_beats < clip.start_beats || pos_beats >= clip_end {
                        continue;
                    }
                    let clip_pos = pos_beats - clip.start_beats;
                    for note in &clip.notes {
                        if note.pitch == v.original_pitch
                            && clip_pos >= note.start_beats
                            && clip_pos < note.start_beats + note.length_beats
                        {
                            still_active = true;
                            break;
                        }
                    }
                    if still_active {
                        break;
                    }
                }
                if !still_active {
                    v.released = true;
                }
            }

            track_voices[ti].retain(|v| !voice_is_done(v));
        }

        // Synthesize MIDI voices
        for (ti, voices) in track_voices.iter_mut().enumerate() {
            let track = &render_tracks[ti];
            if let Some(ref instrument) = track_instruments[ti] {
                for v in voices.iter_mut() {
                    let (sl, sr) = instrument.process_voice(
                        v,
                        &track.instrument_params,
                        sample_rate as f64,
                        &track.extra,
                    );
                    per_track_sample[ti].0 += sl;
                    per_track_sample[ti].1 += sr;
                    per_track_voices[ti] += 1;
                }
            }
        }

        // Run effect chain per track
        for (ti, track) in render_tracks.iter().enumerate() {
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            if per_track_sample[ti] == (0.0, 0.0) && per_track_voices[ti] == 0 {
                let has_tail =
                    ti < track_effects.len() && track_effects[ti].iter().any(|fx| fx.has_tail());
                if !has_tail {
                    continue;
                }
            }
            if per_track_voices[ti] > 0 {
                let vc = per_track_voices[ti] as f64;
                let norm = vc.sqrt();
                per_track_sample[ti].0 /= norm;
                per_track_sample[ti].1 /= norm;
            }
            for (fi, (_, fx_params)) in track.effect_slots.iter().enumerate() {
                if fi < track_effects[ti].len() {
                    let sc_ti = track.effect_sidechain_track.get(fi).copied().flatten();
                    let (key_l, key_r) = if let Some(sc_idx) = sc_ti {
                        if sc_idx < per_track_sample.len() {
                            per_track_sample[sc_idx]
                        } else {
                            per_track_sample[ti]
                        }
                    } else {
                        per_track_sample[ti]
                    };
                    let (ol, or2) = track_effects[ti][fi].process_sidechain(
                        per_track_sample[ti].0,
                        per_track_sample[ti].1,
                        key_l,
                        key_r,
                        fx_params,
                        sample_rate as f64,
                    );
                    per_track_sample[ti] = (ol, or2);
                }
            }
            // CStrip2 channel strip (first render fn)
            if ti < track_cstrip_r.len() {
                let cs_params = &render_tracks[ti].cstrip2_params;
                if !cs_params.is_empty() {
                    let (cl, cr) = track_cstrip_r[ti].process(
                        per_track_sample[ti].0,
                        per_track_sample[ti].1,
                        cs_params,
                        sample_rate as f64,
                    );
                    per_track_sample[ti] = (cl, cr);
                }
            }
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
            let (tl, tr) = per_track_sample[ti];
            let theta = (track.pan as f64 + 1.0) * 0.5 * std::f64::consts::FRAC_PI_2;
            mix_l += tl * crate::modules::fast_cos(theta);
            mix_r += tr * crate::modules::fast_sin(theta);
        }

        // Apply master rack effects
        for (fi, (_, fx_params)) in master_effect_slots.iter().enumerate() {
            if fi < master_effects.len() {
                let (ml, mr) = master_effects[fi].process_sidechain(
                    mix_l,
                    mix_r,
                    mix_l,
                    mix_r,
                    fx_params,
                    sample_rate as f64,
                );
                mix_l = ml;
                mix_r = mr;
            }
        }

        mix_l *= master_volume as f64;
        mix_r *= master_volume as f64;

        // DC-offset removal (mirrors render_to_wav_with_progress)
        {
            let new_l = mix_l - dc_hp_x_l_b + DC_HP_R_BUF * dc_hp_y_l_b;
            dc_hp_x_l_b = mix_l;
            dc_hp_y_l_b = new_l;
            mix_l = new_l;
            let new_r = mix_r - dc_hp_x_r_b + DC_HP_R_BUF * dc_hp_y_r_b;
            dc_hp_x_r_b = mix_r;
            dc_hp_y_r_b = new_r;
            mix_r = new_r;
        }
        // Denormal prevention
        mix_l += 1.0e-24;
        mix_r += 1.0e-24;

        // Slew rate limiter (last-resort spike catcher)
        const SLEW_MAX_B: f64 = 0.8;
        mix_l = slew_prev_l_b + (mix_l - slew_prev_l_b).clamp(-SLEW_MAX_B, SLEW_MAX_B);
        mix_r = slew_prev_r_b + (mix_r - slew_prev_r_b).clamp(-SLEW_MAX_B, SLEW_MAX_B);
        slew_prev_l_b = mix_l;
        slew_prev_r_b = mix_r;

        // Soft clipper (tanh, drive=1.0 — transparent below ±0.7, gentle above ±1.0)
        mix_l = mix_l.tanh();
        mix_r = mix_r.tanh();

        output.push((mix_l, mix_r));
        pos_beats += beats_per_sample;
    }

    output
}

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
        let song_length_beats = project
            .tracks
            .iter()
            .filter(|t| t.track_type != TrackType::Automation)
            .flat_map(|t| t.clips.iter())
            .map(|c| c.start_time() + c.length())
            .fold(0.0_f64, f64::max)
            .max(1.0);
        (0.0, song_length_beats)
    };

    let render_secs = (end_beats - start_beats) * 60.0 / bpm;
    // Add a 2-second tail beyond the last clip so reverb/delay effects can
    // fully decay before the file ends.  The tail is silent audio content —
    // it is still processed through the effects chain so their output is captured.
    const RENDER_TAIL_SECS: f64 = 2.0;
    let base_samples = (render_secs * sample_rate as f64).ceil() as usize;
    let tail_samples = (RENDER_TAIL_SECS * sample_rate as f64) as usize;
    let total_samples = base_samples + tail_samples;
    if total_samples == 0 {
        return Err("Nothing to render (zero-length region)".into());
    }

    // Pre-load audio files
    let mut audio_cache: std::collections::HashMap<String, (Arc<Vec<f32>>, u32)> =
        std::collections::HashMap::new();
    for track in &project.tracks {
        for clip in &track.clips {
            if let Clip::Audio(ac) = clip {
                if !ac.source_file.is_empty() && !audio_cache.contains_key(&ac.source_file) {
                    match crate::audio::load_audio(std::path::Path::new(&ac.source_file)) {
                        Ok((samples, sr)) => {
                            audio_cache.insert(ac.source_file.clone(), (Arc::new(samples), sr));
                        }
                        Err(e) => {
                            eprintln!("[render] Failed to load {}: {}", ac.source_file, e);
                            audio_cache
                                .insert(ac.source_file.clone(), (Arc::new(Vec::new()), 44100));
                        }
                    }
                }
            }
        }
    }

    // Build AudioTrack snapshot (mirrors main.rs sync loop exactly)
    let mut render_tracks: Vec<AudioTrack> = Vec::new();
    for track in &project.tracks {
        // Find instrument module in rack (mirrors main.rs exactly)
        let instrument_module: Option<String> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled)
            .find(|slot| crate::modules::is_instrument(&slot.plugin_name))
            .map(|slot| slot.plugin_name.clone());

        let instrument_params: Vec<(String, f32)> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled)
            .find(|slot| crate::modules::is_instrument(&slot.plugin_name))
            .map(|slot| {
                slot.params
                    .iter()
                    .map(|p| (p.id.clone(), p.value))
                    .collect()
            })
            .unwrap_or_default();

        let mut midi_clips: Vec<AudioMidiClip> = Vec::new();
        if track.track_type == TrackType::Midi {
            for clip in &track.clips {
                if let Clip::Midi(mc) = clip {
                    let notes = mc
                        .notes
                        .iter()
                        .map(|n| AudioNote {
                            pitch: n.pitch,
                            velocity: n.velocity,
                            start_beats: n.start,
                            length_beats: n.length,
                        })
                        .collect();
                    midi_clips.push(AudioMidiClip {
                        start_beats: mc.start_time,
                        length_beats: mc.length,
                        notes,
                    });
                }
            }
        }

        let mut audio_clips: Vec<AudioSampleClip> = Vec::new();
        for clip in &track.clips {
            if let Clip::Audio(ac) = clip {
                if ac.source_file.is_empty() {
                    continue;
                }
                if let Some((samples, sr)) = audio_cache.get(&ac.source_file) {
                    if !samples.is_empty() {
                        audio_clips.push(AudioSampleClip {
                            start_beats: ac.start_time,
                            length_beats: ac.length,
                            gain: ac.gain,
                            offset_secs: ac.offset,
                            samples: samples.clone(),
                            sample_rate: *sr,
                            fade_in: ac.fade_in,
                            fade_out: ac.fade_out,
                        });
                    }
                }
            }
        }

        // Collect effect slots from rack
        let effect_slots: Vec<(String, Vec<(String, f32)>)> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled && crate::modules::is_effect(&slot.plugin_name))
            .map(|slot| {
                let params: Vec<(String, f32)> = slot
                    .params
                    .iter()
                    .map(|p| (p.id.clone(), p.value))
                    .collect();
                (slot.plugin_name.clone(), params)
            })
            .collect();

        // Build ModuleExtra for sampler data etc.
        let extra = if instrument_module.as_deref() == Some("Sampler") {
            if let Some(ref sample_path) = track.sampler_file {
                if !sample_path.is_empty() {
                    if !audio_cache.contains_key(sample_path) {
                        let path_ref = std::path::Path::new(sample_path);
                        match crate::audio::load_audio(path_ref) {
                            Ok((samples, sr)) => {
                                audio_cache.insert(sample_path.clone(), (Arc::new(samples), sr));
                            }
                            Err(_) => {
                                audio_cache
                                    .insert(sample_path.clone(), (Arc::new(Vec::new()), 44100));
                            }
                        }
                    }
                    if let Some((samples, sr)) = audio_cache.get(sample_path) {
                        ModuleExtra {
                            sample_data: Some(samples.clone()),
                            sample_sr: *sr,
                        }
                    } else {
                        ModuleExtra::default()
                    }
                } else {
                    ModuleExtra::default()
                }
            } else {
                ModuleExtra::default()
            }
        } else {
            ModuleExtra::default()
        };

        // Sidechain source index per effect slot (parallel to effect_slots).
        // Maps sidechain_track_id (project track id) → render track index.
        let effect_sidechain_track: Vec<Option<usize>> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled && crate::modules::is_effect(&slot.plugin_name))
            .map(|slot| {
                slot.sidechain_track_id
                    .and_then(|sc_id| project.tracks.iter().position(|t| t.id == sc_id))
            })
            .collect();

        // Collect MIDI effect slots from rack (mirrors main.rs exactly)
        let midi_effect_slots: Vec<(String, Vec<(String, f32)>)> = track
            .rack
            .iter()
            .filter(|slot| slot.enabled && crate::modules::is_midi_effect(&slot.plugin_name))
            .map(|slot| {
                let params: Vec<(String, f32)> = slot
                    .params
                    .iter()
                    .map(|p| (p.id.clone(), p.value))
                    .collect();
                (slot.plugin_name.clone(), params)
            })
            .collect();

        render_tracks.push(AudioTrack {
            volume: track.volume,
            pan: track.pan,
            mute: track.mute,
            solo: track.solo,
            is_automation: track.track_type == TrackType::Automation,
            midi_clips,
            audio_clips,
            instrument_module,
            instrument_params,
            effect_slots,
            midi_effect_slots,
            effect_sidechain_track,
            cstrip2_params: track.cstrip2_params.clone(),
            cstrip2_bypass: track.cstrip2_bypass,
            extra,
        });
    }

    let any_solo = render_tracks.iter().any(|t| t.solo && !t.is_automation);

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

    fn midi_to_freq(pitch: u8) -> f64 {
        440.0 * crate::modules::fast_pow2((pitch as f64 - 69.0) / 12.0)
    }

    let num_tracks = render_tracks.len();
    let mut track_voices: Vec<Vec<ModuleVoice>> = (0..num_tracks).map(|_| Vec::new()).collect();

    // Create per-track instrument + effect instances for the render
    let track_instruments: Vec<Option<Box<dyn InstrumentModule>>> = render_tracks
        .iter()
        .map(|t| t.instrument_module.as_deref().and_then(create_instrument))
        .collect();
    let mut track_effects: Vec<Vec<Box<dyn EffectModule>>> = render_tracks
        .iter()
        .map(|t| {
            t.effect_slots
                .iter()
                .filter_map(|(name, _)| create_effect(name, sample_rate))
                .collect()
        })
        .collect();
    let mut track_cstrip_r: Vec<Box<dyn EffectModule>> = render_tracks
        .iter()
        .map(|_| create_effect("CStrip2", sample_rate).unwrap())
        .collect();
    let mut track_midi_effects_wav: Vec<Vec<Box<dyn MidiEffect>>> = render_tracks
        .iter()
        .map(|t| {
            t.midi_effect_slots
                .iter()
                .filter_map(|(name, _)| create_midi_effect(name))
                .collect()
        })
        .collect();
    let mut track_arp_state_wav: Vec<(usize, f64)> = vec![(0, -999.0); render_tracks.len()];

    // Create master rack effect instances (applied to the stereo mix after tracking)
    let master_effect_slots: Vec<(String, Vec<(String, f32)>)> = project
        .master_rack
        .iter()
        .filter(|s| s.enabled && crate::modules::is_effect(&s.plugin_name))
        .map(|s| {
            let params: Vec<(String, f32)> =
                s.params.iter().map(|p| (p.id.clone(), p.value)).collect();
            (s.plugin_name.clone(), params)
        })
        .collect();
    let mut master_effects: Vec<Box<dyn EffectModule>> = master_effect_slots
        .iter()
        .filter_map(|(name, _)| create_effect(name, sample_rate))
        .collect();

    // Pre-compute automation target → track index mapping for fast lookup.
    // Format of target_param: "track_id:slot_id:param_id"
    // We map each track's numeric id to its index in render_tracks.
    let track_id_to_render_idx: std::collections::HashMap<u32, usize> = project
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id, i))
        .collect();

    let beats_per_sample = bpm / 60.0 / sample_rate as f64;
    let beats_per_sec = bpm / 60.0;
    let mut pos_beats = start_beats;

    // ── DC-offset one-pole HP filter state (fc ≈ 20 Hz, mirrors audio.rs) ──
    const DC_HP_R: f64 = 0.99972;
    let mut dc_hp_x_l = 0.0_f64;
    let mut dc_hp_y_l = 0.0_f64;
    let mut dc_hp_x_r = 0.0_f64;
    let mut dc_hp_y_r = 0.0_f64;

    // ── Slew rate limiter state ──
    let mut slew_prev_l = 0.0_f64;
    let mut slew_prev_r = 0.0_f64;

    // ── Export fade-in / fade-out lengths (samples) ──
    // 2 ms fade-in at start, 5 ms fade-out at end — standard mastering practice.
    // Both use equal-power sine curve to preserve loudness perception.
    let export_fade_in_len = (0.002 * sample_rate as f64) as usize; //  ~88 samples @ 44.1kHz
    let export_fade_out_len = (0.005 * sample_rate as f64) as usize; // ~220 samples @ 44.1kHz
                                                                     // Fade-out starts this many samples before the end of the export
    let fade_out_start = total_samples.saturating_sub(export_fade_out_len);

    for _si in 0..total_samples {
        // Report progress (permille 0..1000) every 4096 samples to avoid atomic contention
        if let Some(ref prog) = progress {
            if _si & 0xFFF == 0 {
                let permille = (_si as u64 * 1000 / total_samples as u64) as u32;
                prog.store(permille, std::sync::atomic::Ordering::Relaxed);
            }
        }

        let mut per_track_sample = vec![(0.0_f64, 0.0_f64); num_tracks];
        let mut per_track_voices = vec![0usize; num_tracks];

        // ── Evaluate automation at current position (MUST mirror main.rs) ──
        // Apply automation values to BOTH instrument_params AND effect_slots.
        // Target key format: "track_id:slot_id:param_id"
        let auto_vals = evaluate_automation_at(project, pos_beats);
        for (target_key, auto_val) in &auto_vals {
            let parts: Vec<&str> = target_key.split(':').collect();
            if parts.len() != 3 {
                continue;
            }
            let Ok(track_id) = parts[0].parse::<u32>() else {
                continue;
            };
            let Ok(slot_id) = parts[1].parse::<u32>() else {
                continue;
            };
            let param_id = parts[2];
            if let Some(&ri) = track_id_to_render_idx.get(&track_id) {
                // Check which slot this automation targets by comparing slot_id
                // against the original project data to find if it's instrument or effect
                let is_instrument_slot = project
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .and_then(|t| t.rack.iter().find(|s| s.slot_id == slot_id))
                    .map(|s| crate::modules::is_instrument(&s.plugin_name))
                    .unwrap_or(false);

                if is_instrument_slot {
                    // Update instrument_params
                    for p in render_tracks[ri].instrument_params.iter_mut() {
                        if p.0 == param_id {
                            p.1 = *auto_val;
                        }
                    }
                } else {
                    // Update effect_slots params
                    // Find which effect slot index corresponds to this slot_id
                    let fx_slot_idx =
                        project
                            .tracks
                            .iter()
                            .find(|t| t.id == track_id)
                            .and_then(|t| {
                                t.rack
                                    .iter()
                                    .filter(|s| {
                                        s.enabled && crate::modules::is_effect(&s.plugin_name)
                                    })
                                    .position(|s| s.slot_id == slot_id)
                            });
                    if let Some(fi) = fx_slot_idx {
                        if fi < render_tracks[ri].effect_slots.len() {
                            for p in render_tracks[ri].effect_slots[fi].1.iter_mut() {
                                if p.0 == param_id {
                                    p.1 = *auto_val;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Trigger new voices + release expired ones (mirrors audio.rs exactly)
        for (ti, track) in render_tracks.iter().enumerate() {
            if track.is_automation || track.mute {
                continue; // skip, but don't clear voices (mirrors audio.rs)
            }
            if any_solo && !track.solo {
                continue; // skip, but don't clear voices (mirrors audio.rs)
            }
            // Skip if no instrument module
            if track.instrument_module.is_none() {
                continue;
            }

            for clip in &track.midi_clips {
                let clip_end = clip.start_beats + clip.length_beats;

                // ── Release all voices at clip boundary ──
                // When pos_beats crosses clip_end, release any still-active
                // voices from this clip to prevent stuck notes.
                let prev_beats = pos_beats - beats_per_sample;
                if prev_beats < clip_end && pos_beats >= clip_end {
                    for note in &clip.notes {
                        for v in track_voices[ti].iter_mut() {
                            if v.pitch == note.pitch && !v.released {
                                v.released = true;
                                break;
                            }
                        }
                    }
                }

                if pos_beats < clip.start_beats || pos_beats >= clip_end {
                    continue;
                }
                let clip_pos = pos_beats - clip.start_beats;
                let prev = clip_pos - beats_per_sample;

                for note in &clip.notes {
                    let note_end = note.start_beats + note.length_beats;

                    // Trigger new voice at note start via MIDI effect trait chain
                    if prev < note.start_beats && clip_pos >= note.start_beats {
                        let vel = note.velocity as f32 / 127.0 * track.volume;
                        let seed = vec![MidiEvent::new(note.pitch, vel)];

                        let has_arp = ti < track_midi_effects_wav.len()
                            && track_midi_effects_wav[ti]
                                .iter()
                                .any(|m| m.manages_voices());

                        if !has_arp {
                            let final_events = if ti < track_midi_effects_wav.len() {
                                run_midi_chain(
                                    seed,
                                    &mut track_midi_effects_wav[ti],
                                    track.midi_effect_slots.iter().map(|(_, p)| p),
                                    pos_beats,
                                    pos_beats - beats_per_sample,
                                    bpm,
                                    sample_rate as f64,
                                )
                            } else {
                                vec![MidiEvent::new(note.pitch, vel)]
                            };
                            for ev in final_events {
                                if !track_voices[ti]
                                    .iter()
                                    .any(|v| v.pitch == ev.pitch && !v.released)
                                {
                                    let freq = midi_to_freq(ev.pitch);
                                    let mut voice =
                                        ModuleVoice::new(freq, ev.velocity, ti, ev.pitch);
                                    voice.original_pitch = note.pitch;
                                    track_voices[ti].push(voice);
                                }
                            }
                        }
                    }

                    // Release voice at note end (set released=true for ADSR tail)
                    if prev < note_end && clip_pos >= note_end {
                        for v in track_voices[ti].iter_mut() {
                            if v.original_pitch == note.pitch && !v.released {
                                v.released = true;
                                break;
                            }
                        }
                    }
                }
            }

            // ── Arp step trigger (wav render) ──
            let has_arp_instance = ti < track_midi_effects_wav.len()
                && track_midi_effects_wav[ti]
                    .iter()
                    .any(|m| m.manages_voices());
            if has_arp_instance {
                if let Some((_, arp_params)) = track
                    .midi_effect_slots
                    .iter()
                    .find(|(n, _)| n == "Arpeggiator")
                {
                    let get_arp = |k: &str, d: f32| -> f32 {
                        arp_params
                            .iter()
                            .find(|(id, _)| id == k)
                            .map(|(_, v)| *v)
                            .unwrap_or(d)
                    };
                    let rate_beats = get_arp("rate", 0.25) as f64;
                    let octaves = get_arp("octaves", 1.0) as i32;
                    let pattern = get_arp("pattern", 0.0) as i32;
                    let vel_default = track.volume;

                    let (ref mut step, ref mut last_beat) = track_arp_state_wav[ti];

                    let mut active_pitches: Vec<u8> = Vec::new();
                    for clip in &track.midi_clips {
                        let clip_end = clip.start_beats + clip.length_beats;
                        if pos_beats < clip.start_beats || pos_beats >= clip_end {
                            continue;
                        }
                        let cp = pos_beats - clip.start_beats;
                        for note in &clip.notes {
                            if cp >= note.start_beats
                                && cp < note.start_beats + note.length_beats
                                && !active_pitches.contains(&note.pitch)
                            {
                                active_pitches.push(note.pitch);
                            }
                        }
                    }
                    active_pitches.sort_unstable();

                    let mut pool: Vec<u8> = Vec::new();
                    for oct in 0..octaves {
                        for &p in &active_pitches {
                            pool.push((p as i32 + oct * 12).clamp(0, 127) as u8);
                        }
                    }
                    match pattern {
                        1 => pool.reverse(),
                        2 => {
                            let mut d = pool.clone();
                            d.reverse();
                            if d.len() > 1 {
                                pool.extend_from_slice(&d[1..d.len() - 1]);
                            }
                        }
                        3 => {
                            let seed = (pos_beats * 1000.0) as u64;
                            for i in (1..pool.len()).rev() {
                                let j = (seed
                                    .wrapping_mul(6364136223846793005)
                                    .wrapping_add(1442695040888963407)
                                    >> 33) as usize
                                    % (i + 1);
                                pool.swap(i, j);
                            }
                        }
                        _ => {}
                    }

                    if !pool.is_empty() {
                        let fire = if *last_beat < 0.0 {
                            true
                        } else {
                            (pos_beats / rate_beats).floor() as usize
                                > (*last_beat / rate_beats).floor() as usize
                        };
                        if fire {
                            for v in track_voices[ti].iter_mut() {
                                if !v.released {
                                    v.released = true;
                                }
                            }
                            let idx = *step % pool.len();
                            let pitch = pool[idx];
                            let mut voice =
                                ModuleVoice::new(midi_to_freq(pitch), vel_default, ti, pitch);
                            voice.original_pitch = pitch;
                            track_voices[ti].push(voice);
                            *step = (*step + 1) % pool.len().max(1);
                            *last_beat = pos_beats;
                        }
                    } else {
                        for v in track_voices[ti].iter_mut() {
                            if !v.released {
                                v.released = true;
                            }
                        }
                        *step = 0;
                        *last_beat = -999.0;
                    }
                }
            }

            // ── Release / kill voices whose notes have ended ──
            for v in track_voices[ti].iter_mut() {
                if v.released {
                    continue;
                }
                let has_arp = ti < track_midi_effects_wav.len()
                    && track_midi_effects_wav[ti]
                        .iter()
                        .any(|m| m.manages_voices());
                if has_arp {
                    continue;
                }
                let mut still_active = false;
                for clip in &track.midi_clips {
                    let clip_end = clip.start_beats + clip.length_beats;
                    if pos_beats < clip.start_beats || pos_beats >= clip_end {
                        continue;
                    }
                    let clip_pos = pos_beats - clip.start_beats;
                    for note in &clip.notes {
                        if note.pitch == v.original_pitch
                            && clip_pos >= note.start_beats
                            && clip_pos < note.start_beats + note.length_beats
                        {
                            still_active = true;
                            break;
                        }
                    }
                    if still_active {
                        break;
                    }
                }
                if !still_active {
                    v.released = true;
                }
            }

            // Remove voices that are fully done (amp envelope finished)
            track_voices[ti].retain(|v| !voice_is_done(v));
        }

        // Synthesize MIDI voices via trait objects (mirrors audio.rs exactly)
        for (ti, voices) in track_voices.iter_mut().enumerate() {
            let track = &render_tracks[ti];
            if let Some(ref instrument) = track_instruments[ti] {
                for v in voices.iter_mut() {
                    let (sl, sr) = instrument.process_voice(
                        v,
                        &track.instrument_params,
                        sample_rate as f64,
                        &track.extra,
                    );
                    per_track_sample[ti].0 += sl;
                    per_track_sample[ti].1 += sr;
                    per_track_voices[ti] += 1;
                }
            }
        }

        // Mix audio clips — NATIVE speed (BPM only positions clips, doesn't stretch them)
        for (ti, track) in render_tracks.iter().enumerate() {
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            for aclip in &track.audio_clips {
                let clip_end = aclip.start_beats + aclip.length_beats;
                if pos_beats < aclip.start_beats || pos_beats >= clip_end {
                    continue;
                }
                // Real time elapsed since clip start (at current BPM) + trim offset
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
                    let mut s = raw * aclip.gain as f64 * track.volume as f64;
                    // Equal-power micro-fade at CLIP boundaries (~5 ms = 220 samples).
                    // sin(t·π/2) curve: removes click without tonal impact.
                    // Applied only at the first/last ~5 ms — rest of clip is unaffected.
                    let fade_len = 220usize;
                    let clip_sample = (clip_pos_secs * sample_rate as f64) as usize;
                    let clip_len_samples =
                        (aclip.length_beats / beats_per_sec * sample_rate as f64) as usize;
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
                        let fade_in_samples = (aclip.fade_in * sample_rate as f64) as usize;
                        if fade_in_samples > 0 && clip_sample < fade_in_samples {
                            s *= clip_sample as f64 / fade_in_samples as f64;
                        }
                    }
                    // User-controlled fade-out
                    if aclip.fade_out > 0.0 {
                        let fade_out_samples = (aclip.fade_out * sample_rate as f64) as usize;
                        if fade_out_samples > 0 && clip_len_samples > fade_out_samples {
                            let fade_out_start = clip_len_samples - fade_out_samples;
                            if clip_sample >= fade_out_start {
                                s *= (clip_len_samples - clip_sample) as f64
                                    / fade_out_samples as f64;
                            }
                        }
                    }
                    per_track_sample[ti].0 += s;
                    per_track_sample[ti].1 += s;
                }
            }
        }

        // ── Run effect chain per track via trait objects ──
        for (ti, track) in render_tracks.iter().enumerate() {
            if track.is_automation || track.mute {
                continue;
            }
            if any_solo && !track.solo {
                continue;
            }
            if per_track_sample[ti] == (0.0, 0.0) && per_track_voices[ti] == 0 {
                // Don't skip if any effect in this track's chain has a tail
                // (e.g. reverb, delay, chorus) — they produce output after input stops.
                let has_tail =
                    ti < track_effects.len() && track_effects[ti].iter().any(|fx| fx.has_tail());
                if !has_tail {
                    continue;
                }
            }
            // Normalize voices before effects (must match audio.rs exactly)
            if per_track_voices[ti] > 0 {
                let vc = per_track_voices[ti] as f64;
                let norm = vc.sqrt();
                per_track_sample[ti].0 /= norm;
                per_track_sample[ti].1 /= norm;
            }
            for (fi, (_, fx_params)) in track.effect_slots.iter().enumerate() {
                if fi < track_effects[ti].len() {
                    track_effects[ti][fi].set_bpm(bpm);
                    // Resolve sidechain source signal (default: self)
                    let sc_ti = track.effect_sidechain_track.get(fi).copied().flatten();
                    let (key_l, key_r) = if let Some(sc_idx) = sc_ti {
                        if sc_idx < per_track_sample.len() {
                            per_track_sample[sc_idx]
                        } else {
                            per_track_sample[ti]
                        }
                    } else {
                        per_track_sample[ti]
                    };
                    let (ol, or2) = track_effects[ti][fi].process_sidechain(
                        per_track_sample[ti].0,
                        per_track_sample[ti].1,
                        key_l,
                        key_r,
                        fx_params,
                        sample_rate as f64,
                    );
                    per_track_sample[ti] = (ol, or2);
                }
            }
            // CStrip2 channel strip (second render fn)
            if ti < track_cstrip_r.len() {
                let cs_params = &render_tracks[ti].cstrip2_params;
                if !cs_params.is_empty() {
                    let (cl, cr) = track_cstrip_r[ti].process(
                        per_track_sample[ti].0,
                        per_track_sample[ti].1,
                        cs_params,
                        sample_rate as f64,
                    );
                    per_track_sample[ti] = (cl, cr);
                }
            }
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
            let (tl, tr) = per_track_sample[ti];
            let theta = (track.pan as f64 + 1.0) * 0.5 * std::f64::consts::FRAC_PI_2;
            mix_l += tl * crate::modules::fast_cos(theta);
            mix_r += tr * crate::modules::fast_sin(theta);
        }

        // ── Apply master rack effects to the stereo mix ──
        for (fi, (_, fx_params)) in master_effect_slots.iter().enumerate() {
            if fi < master_effects.len() {
                master_effects[fi].set_bpm(bpm);
                let (ml, mr) = master_effects[fi].process_sidechain(
                    mix_l,
                    mix_r,
                    mix_l,
                    mix_r,
                    fx_params,
                    sample_rate as f64,
                );
                mix_l = ml;
                mix_r = mr;
            }
        }

        mix_l *= master_volume as f64;
        mix_r *= master_volume as f64;

        // DC-offset high-pass filter (fc ≈ 20 Hz) — removes slowly-drifting bias
        // that can cause clicks with abrupt gain changes. No audible tonal effect.
        {
            let new_l = mix_l - dc_hp_x_l + DC_HP_R * dc_hp_y_l;
            dc_hp_x_l = mix_l;
            dc_hp_y_l = new_l;
            mix_l = new_l;
            let new_r = mix_r - dc_hp_x_r + DC_HP_R * dc_hp_y_r;
            dc_hp_x_r = mix_r;
            dc_hp_y_r = new_r;
            mix_r = new_r;
        }

        // Denormal prevention — completely inaudible sub-threshold DC offset
        mix_l += 1.0e-24;
        mix_r += 1.0e-24;

        // Slew rate limiter (last-resort spike catcher — generous threshold)
        const SLEW_MAX: f64 = 0.8;
        mix_l = slew_prev_l + (mix_l - slew_prev_l).clamp(-SLEW_MAX, SLEW_MAX);
        mix_r = slew_prev_r + (mix_r - slew_prev_r).clamp(-SLEW_MAX, SLEW_MAX);
        slew_prev_l = mix_l;
        slew_prev_r = mix_r;

        // Soft clipper (tanh at drive=1.0) — transparent below ±0.7,
        // gently saturates hot peaks instead of hard-clipping them.
        mix_l = mix_l.tanh();
        mix_r = mix_r.tanh();

        // Export fade-in (equal-power sine, ~2 ms) — removes start discontinuity
        if _si < export_fade_in_len && export_fade_in_len > 0 {
            let t = _si as f64 / export_fade_in_len as f64;
            let gain = (t * std::f64::consts::FRAC_PI_2).sin();
            mix_l *= gain;
            mix_r *= gain;
        }
        // Export fade-out (equal-power sine, ~5 ms) — removes end discontinuity
        if _si >= fade_out_start && export_fade_out_len > 0 {
            let elapsed = _si - fade_out_start;
            let t = 1.0 - (elapsed as f64 / export_fade_out_len as f64);
            let gain = (t * std::f64::consts::FRAC_PI_2).sin();
            mix_l *= gain;
            mix_r *= gain;
        }

        match settings.bit_depth_idx {
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

        pos_beats += beats_per_sample;
    }

    // ── Release all remaining voices and render tail ──
    // In range/loop mode the user specified exact boundaries — skip the tail entirely
    // so no extra silence is added past the range end.
    if !settings.loop_only {
        // Release every active voice so ADSR envelopes can finish naturally.
        for voices in track_voices.iter_mut() {
            for v in voices.iter_mut() {
                if !v.released {
                    v.released = true;
                }
            }
        }
        // Render up to 10 seconds of tail to let envelopes decay
        let max_tail_samples = (10.0 * sample_rate as f64) as usize;
        // Stop after 0.25 seconds of consecutive silence (well below audible threshold)
        let silence_threshold = 1e-5; // ~-100 dB
        let silence_required = (0.25 * sample_rate as f64) as usize;
        let mut consecutive_silent = 0usize;
        for _tail in 0..max_tail_samples {
            // Check if any voices are still alive
            let any_alive = track_voices.iter().any(|voices| !voices.is_empty());
            if !any_alive {
                break;
            }
            // Remove finished voices
            for voices in track_voices.iter_mut() {
                voices.retain(|v| !voice_is_done(v));
            }
            if track_voices.iter().all(|voices| voices.is_empty()) {
                break;
            }
            // Synthesize remaining voices
            let mut per_track_sample = vec![(0.0_f64, 0.0_f64); num_tracks];
            for (ti, voices) in track_voices.iter_mut().enumerate() {
                let track = &render_tracks[ti];
                if let Some(ref instrument) = track_instruments[ti] {
                    for v in voices.iter_mut() {
                        let (sl, sr) = instrument.process_voice(
                            v,
                            &track.instrument_params,
                            sample_rate as f64,
                            &track.extra,
                        );
                        per_track_sample[ti].0 += sl;
                        per_track_sample[ti].1 += sr;
                    }
                }
                // Normalize
                if !voices.is_empty() {
                    let norm = (voices.len() as f64).sqrt();
                    per_track_sample[ti].0 /= norm;
                    per_track_sample[ti].1 /= norm;
                }
                // Run effects
                for (fi, (_, fx_params)) in track.effect_slots.iter().enumerate() {
                    if fi < track_effects[ti].len() {
                        track_effects[ti][fi].set_bpm(bpm);
                        let sc_ti = track.effect_sidechain_track.get(fi).copied().flatten();
                        let (key_l, key_r) = if let Some(sc_idx) = sc_ti {
                            if sc_idx < per_track_sample.len() {
                                per_track_sample[sc_idx]
                            } else {
                                per_track_sample[ti]
                            }
                        } else {
                            per_track_sample[ti]
                        };
                        let (ol, or2) = track_effects[ti][fi].process_sidechain(
                            per_track_sample[ti].0,
                            per_track_sample[ti].1,
                            key_l,
                            key_r,
                            fx_params,
                            sample_rate as f64,
                        );
                        per_track_sample[ti] = (ol, or2);
                    }
                }
                // CStrip2 channel strip (tail render fn)
                if ti < track_cstrip_r.len() {
                    let cs_params = &render_tracks[ti].cstrip2_params;
                    if !cs_params.is_empty() {
                        let (cl, cr) = track_cstrip_r[ti].process(
                            per_track_sample[ti].0,
                            per_track_sample[ti].1,
                            cs_params,
                            sample_rate as f64,
                        );
                        per_track_sample[ti] = (cl, cr);
                    }
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
                let (tl, tr) = per_track_sample[ti];
                let theta = (track.pan as f64 + 1.0) * 0.5 * std::f64::consts::FRAC_PI_2;
                mix_l += tl * crate::modules::fast_cos(theta);
                mix_r += tr * crate::modules::fast_sin(theta);
            }
            for (fi, (_, fx_params)) in master_effect_slots.iter().enumerate() {
                if fi < master_effects.len() {
                    master_effects[fi].set_bpm(bpm);
                    let (ml, mr) = master_effects[fi].process_sidechain(
                        mix_l,
                        mix_r,
                        mix_l,
                        mix_r,
                        fx_params,
                        sample_rate as f64,
                    );
                    mix_l = ml;
                    mix_r = mr;
                }
            }
            mix_l *= master_volume as f64;
            mix_r *= master_volume as f64;
            // Only write if there's audible signal
            if mix_l.abs() < silence_threshold && mix_r.abs() < silence_threshold {
                consecutive_silent += 1;
                if consecutive_silent >= silence_required {
                    break;
                }
            } else {
                consecutive_silent = 0;
            }
            match settings.bit_depth_idx {
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
        }
    } // end if !settings.loop_only

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
        "[render] Exported {} samples (stereo) {} @ {}Hz to {}",
        total_samples, range_desc, sample_rate, path
    );
    Ok(())
}
