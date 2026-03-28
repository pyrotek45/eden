// Eden DAW — DSP track setup & render instance creation
//
// Pre-loads audio caches, builds AudioTrack snapshots, creates DSP instances.

use crate::engine::{AudioMidiClip, AudioNote, AudioSampleClip, AudioTrack};
use crate::models::*;
use crate::modules::{
    create_effect, create_instrument, create_midi_effect, EffectModule, InstrumentModule,
    MidiEffect, ModuleExtra,
};
use std::sync::Arc;

// ── Audio cache ──────────────────────────────────────────────────────────────

/// Pre-load audio files referenced by the project into a cache.
pub fn preload_audio_cache(
    project: &Project,
) -> std::collections::HashMap<String, (Arc<Vec<f32>>, u32)> {
    let mut cache: std::collections::HashMap<String, (Arc<Vec<f32>>, u32)> =
        std::collections::HashMap::new();
    for track in &project.tracks {
        for clip in &track.clips {
            if let Clip::Audio(ac) = clip {
                if !ac.source_file.is_empty() && !cache.contains_key(&ac.source_file) {
                    match crate::engine::load_audio(std::path::Path::new(&ac.source_file)) {
                        Ok((samples, sr)) => {
                            cache.insert(ac.source_file.clone(), (Arc::new(samples), sr));
                        }
                        Err(e) => {
                            eprintln!("[dsp] Failed to load {}: {}", ac.source_file, e);
                            cache.insert(ac.source_file.clone(), (Arc::new(Vec::new()), 44100));
                        }
                    }
                }
            }
        }
        // Also preload sampler files
        if let Some(ref sample_path) = track.sampler_file {
            if !sample_path.is_empty() && !cache.contains_key(sample_path) {
                match crate::engine::load_audio(std::path::Path::new(sample_path)) {
                    Ok((samples, sr)) => {
                        cache.insert(sample_path.clone(), (Arc::new(samples), sr));
                    }
                    Err(_) => {
                        cache.insert(sample_path.clone(), (Arc::new(Vec::new()), 44100));
                    }
                }
            }
        }
    }
    cache
}

// ── Track snapshot builder ───────────────────────────────────────────────────

/// Build a Vec<AudioTrack> snapshot from the project, ready for rendering.
/// `audio_cache` should be pre-populated via `preload_audio_cache`.
pub fn build_render_tracks(
    project: &Project,
    audio_cache: &std::collections::HashMap<String, (Arc<Vec<f32>>, u32)>,
) -> Vec<AudioTrack> {
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

        // Build ModuleExtra for sampler data etc.
        let extra = if instrument_module.as_deref() == Some("Sampler") {
            if let Some(ref sample_path) = track.sampler_file {
                if !sample_path.is_empty() {
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
    render_tracks
}

/// Build the master effect slot snapshot from the project's master rack.
pub fn build_master_effect_slots(project: &Project) -> Vec<(String, Vec<(String, f32)>)> {
    project
        .master_rack
        .iter()
        .filter(|s| s.enabled && crate::modules::is_effect(&s.plugin_name))
        .map(|s| {
            let params: Vec<(String, f32)> =
                s.params.iter().map(|p| (p.id.clone(), p.value)).collect();
            (s.plugin_name.clone(), params)
        })
        .collect()
}

// ── Render instance set ──────────────────────────────────────────────────────

/// All the mutable DSP instances needed for offline rendering.
pub struct RenderInstances {
    pub track_instruments: Vec<Option<Box<dyn InstrumentModule>>>,
    pub track_effects: Vec<Vec<Box<dyn EffectModule>>>,
    pub track_midi_effects: Vec<Vec<Box<dyn MidiEffect>>>,
    pub track_cstrip: Vec<Box<dyn EffectModule>>,
    pub track_arp_state: Vec<(usize, f64)>,
    pub master_effects: Vec<Box<dyn EffectModule>>,
}

/// Create all DSP instances for a render pass.
pub fn create_render_instances(
    render_tracks: &[AudioTrack],
    master_effect_slots: &[(String, Vec<(String, f32)>)],
    sample_rate: u32,
) -> RenderInstances {
    let num_tracks = render_tracks.len();
    let track_instruments: Vec<Option<Box<dyn InstrumentModule>>> = render_tracks
        .iter()
        .map(|t| t.instrument_module.as_deref().and_then(create_instrument))
        .collect();
    let track_effects: Vec<Vec<Box<dyn EffectModule>>> = render_tracks
        .iter()
        .map(|t| {
            t.effect_slots
                .iter()
                .filter_map(|(name, _)| create_effect(name, sample_rate))
                .collect()
        })
        .collect();
    let track_midi_effects: Vec<Vec<Box<dyn MidiEffect>>> = render_tracks
        .iter()
        .map(|t| {
            t.midi_effect_slots
                .iter()
                .filter_map(|(name, _)| create_midi_effect(name))
                .collect()
        })
        .collect();
    let track_cstrip: Vec<Box<dyn EffectModule>> = (0..num_tracks)
        .map(|_| create_effect("CStrip2", sample_rate).unwrap())
        .collect();
    let track_arp_state: Vec<(usize, f64)> = vec![(0, -999.0); num_tracks];
    let master_effects: Vec<Box<dyn EffectModule>> = master_effect_slots
        .iter()
        .filter_map(|(name, _)| create_effect(name, sample_rate))
        .collect();
    RenderInstances {
        track_instruments,
        track_effects,
        track_midi_effects,
        track_cstrip,
        track_arp_state,
        master_effects,
    }
}
