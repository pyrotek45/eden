// Eden DAW — DSP voice triggering & synthesis

use crate::engine::AudioTrack;
use crate::modules::{
    voice_is_done, InstrumentModule, MidiEffect, MidiEvent, MidiEvents, ModuleExtra, ModuleVoice,
};

use super::{midi_to_freq, run_midi_chain};

// ── Voice triggering ─────────────────────────────────────────────────────────

/// Trigger new MIDI voices for a single track at the current beat position.
///
/// `track_idx` — index into `render_tracks` / `track_voices`.
/// `track_voices` — per-track voice array (only this track's index is mutated).
/// `track_midi_effects` — MIDI effect chain instances (for running the chain).
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
    keyboard_held: &[MidiEvent],
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

    let timed = track_midi_effects.iter().any(|m| m.manages_voices());
    let mut events = MidiEvents::new();
    for clip in &track.midi_clips {
        if pos_beats < clip.start_beats || pos_beats >= clip.start_beats + clip.length_beats {
            continue;
        }
        let clip_pos = pos_beats - clip.start_beats;
        for note in clip.notes.iter() {
            let active =
                clip_pos >= note.start_beats && clip_pos < note.start_beats + note.length_beats;
            let starts = (clip_pos - beats_per_sample < note.start_beats
                && clip_pos >= note.start_beats)
                || (clip_pos < beats_per_sample && active);
            if (timed && active) || (!timed && starts) {
                events.push(MidiEvent::new(note.pitch, note.velocity as f32 / 127.0));
            }
        }
    }
    if timed {
        for event in keyboard_held {
            events.push(*event);
        }
    }
    let held_empty = events.is_empty();
    let output = run_midi_chain(
        events,
        track_midi_effects,
        track.midi_effect_slots.iter().map(|(_, p)| p),
        pos_beats,
        pos_beats - beats_per_sample,
        bpm,
        sample_rate,
    );
    if timed
        && (held_empty
            || !output.is_empty()
            || track_midi_effects.iter().any(|m| !m.gate_open(pos_beats)))
    {
        for voice in track_voices
            .iter_mut()
            .filter(|v| v.track_idx == track_idx && v.preview_samples_remaining.is_none())
        {
            voice.released = true;
        }
    }
    for event in output {
        if timed
            || !track_voices
                .iter()
                .any(|v| v.track_idx == track_idx && v.pitch == event.pitch && !v.released)
        {
            let mut voice = ModuleVoice::new(
                midi_to_freq(event.pitch),
                event.velocity,
                track_idx,
                event.pitch,
            );
            voice.original_pitch = event.original_pitch;
            track_voices.push(voice);
        }
    }
    if !timed {
        for voice in track_voices.iter_mut().filter(|v| {
            v.track_idx == track_idx && !v.released && v.preview_samples_remaining.is_none()
        }) {
            let active = keyboard_held
                .iter()
                .any(|event| event.original_pitch == voice.original_pitch)
                || track.midi_clips.iter().any(|clip| {
                    let cp = pos_beats - clip.start_beats;
                    cp >= 0.0
                        && cp < clip.length_beats
                        && clip.notes.iter().any(|note| {
                            note.pitch == voice.original_pitch
                                && cp >= note.start_beats
                                && cp < note.start_beats + note.length_beats
                        })
                });
            if !active {
                voice.released = true;
            }
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

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    use crate::app::models::*;
    #[test]
    fn arrangement_scheduler_runs_full_chain_and_releases_every_chord_voice() {
        let mut project = Project::demo();
        let track = &mut project.tracks[0];
        track.rack = vec![
            RackSlot::transpose(1),
            RackSlot::arpeggiator(2),
            RackSlot::chord(3),
            RackSlot::subtractive_synth(4),
        ];
        track.rack[0].params[0].value = 12.0;
        let tracks = crate::dsp::build_render_tracks(&project, &std::collections::HashMap::new());
        let mut effects: Vec<_> = tracks[0]
            .midi_effect_slots
            .iter()
            .map(|(name, _)| crate::modules::create_midi_effect(name).unwrap())
            .collect();
        let mut voices = Vec::new();
        trigger_and_release_voices(
            &tracks[0],
            0,
            &mut voices,
            &mut effects,
            &[],
            0.0,
            1.0 / 22050.0,
            120.0,
            44100.0,
        );
        assert_eq!(
            voices.iter().map(|v| v.pitch).collect::<Vec<_>>(),
            vec![72, 76, 79]
        );
        assert!(voices
            .iter()
            .all(|v| v.original_pitch == 60 && (v.velocity - 100.0 / 127.0).abs() < 0.0001));
        trigger_and_release_voices(
            &tracks[0],
            0,
            &mut voices,
            &mut effects,
            &[],
            0.21,
            1.0 / 22050.0,
            120.0,
            44100.0,
        );
        assert!(voices.iter().all(|v| v.released));
    }
}
