// Eden DAW — DSP voice triggering & synthesis

use crate::engine::AudioTrack;
use crate::modules::{
    voice_is_done, InstrumentModule, MidiEffect, MidiEvent, ModuleExtra, ModuleVoice,
};

use super::{midi_to_freq, run_midi_chain};

// ── Voice triggering ─────────────────────────────────────────────────────────

/// Trigger new MIDI voices for a single track at the current beat position.
///
/// `track_idx` — index into `render_tracks` / `track_voices`.
/// `track_voices` — per-track voice array (only this track's index is mutated).
/// `track_midi_effects` — MIDI effect chain instances (for running the chain).
/// `arp_state` — (step_index, last_beat) for this track's arpeggiator.
/// `pos_beats` — current playback position in beats.
/// `beats_per_sample` — 1/SR in beat units.
/// `bpm` — current tempo.
/// `sample_rate` — audio sample rate (f64).
#[allow(clippy::too_many_arguments)]
pub fn trigger_and_release_voices(
    track: &AudioTrack,
    track_idx: usize,
    track_voices: &mut Vec<ModuleVoice>,
    track_midi_effects: &mut [Box<dyn MidiEffect>],
    arp_state: &mut (usize, f64),
    pos_beats: f64,
    beats_per_sample: f64,
    bpm: f64,
    sample_rate: f64,
) {
    if track.is_automation || track.mute {
        return;
    }
    if track.instrument_module.is_none() {
        return;
    }

    for clip in &track.midi_clips {
        let clip_end = clip.start_beats + clip.length_beats;

        // Release all voices at clip boundary
        let prev_beats = pos_beats - beats_per_sample;
        if prev_beats < clip_end && pos_beats >= clip_end {
            for note in &clip.notes {
                for v in track_voices.iter_mut() {
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

            // Trigger new voice at note start
            if prev < note.start_beats && clip_pos >= note.start_beats {
                let vel = note.velocity as f32 / 127.0;
                let seed = vec![MidiEvent::new(note.pitch, vel)];

                let has_arp = track_midi_effects.iter().any(|m| m.manages_voices());

                if !has_arp {
                    let final_events = if !track_midi_effects.is_empty() {
                        run_midi_chain(
                            seed,
                            track_midi_effects,
                            track.midi_effect_slots.iter().map(|(_, p)| p),
                            pos_beats,
                            pos_beats - beats_per_sample,
                            bpm,
                            sample_rate,
                        )
                    } else {
                        vec![MidiEvent::new(note.pitch, vel)]
                    };
                    for ev in final_events {
                        if !track_voices
                            .iter()
                            .any(|v| v.pitch == ev.pitch && !v.released)
                        {
                            let freq = midi_to_freq(ev.pitch);
                            let mut voice =
                                ModuleVoice::new(freq, ev.velocity, track_idx, ev.pitch);
                            voice.original_pitch = note.pitch;
                            track_voices.push(voice);
                        }
                    }
                }
            }

            // Release voice at note end
            if prev < note_end && clip_pos >= note_end {
                for v in track_voices.iter_mut() {
                    if v.original_pitch == note.pitch && !v.released {
                        v.released = true;
                        break;
                    }
                }
            }
        }
    }

    // ── Arp step trigger ──
    let has_arp_instance = track_midi_effects.iter().any(|m| m.manages_voices());
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
            let vel_default = 1.0_f32;

            let (ref mut step, ref mut last_beat) = arp_state;

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
                    for v in track_voices.iter_mut() {
                        if !v.released {
                            v.released = true;
                        }
                    }
                    let idx = *step % pool.len();
                    let pitch = pool[idx];
                    let mut voice =
                        ModuleVoice::new(midi_to_freq(pitch), vel_default, track_idx, pitch);
                    voice.original_pitch = pitch;
                    track_voices.push(voice);
                    *step = (*step + 1) % pool.len().max(1);
                    *last_beat = pos_beats;
                }
            } else {
                for v in track_voices.iter_mut() {
                    if !v.released {
                        v.released = true;
                    }
                }
                *step = 0;
                *last_beat = -999.0;
            }
        }
    }

    // Release voices whose notes have ended
    for v in track_voices.iter_mut() {
        if v.released {
            continue;
        }
        let has_arp = track_midi_effects.iter().any(|m| m.manages_voices());
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

    // Remove fully finished voices
    track_voices.retain(|v| !voice_is_done(v));
}

// ── Voice synthesis ──────────────────────────────────────────────────────────

/// Synthesize all active voices for a track through its instrument module.
/// Returns (left, right) stereo sum and the voice count.
#[inline]
pub fn synthesize_voices(
    voices: &mut [ModuleVoice],
    instrument: &dyn InstrumentModule,
    instrument_params: &[(String, f32)],
    sample_rate: f64,
    extra: &ModuleExtra,
) -> (f64, f64, usize) {
    let mut sum_l = 0.0_f64;
    let mut sum_r = 0.0_f64;
    let mut count = 0usize;
    for v in voices.iter_mut() {
        let (sl, sr) = instrument.process_voice(v, instrument_params, sample_rate, extra);
        sum_l += sl;
        sum_r += sr;
        count += 1;
    }
    (sum_l, sum_r, count)
}
