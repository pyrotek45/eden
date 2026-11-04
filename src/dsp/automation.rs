// Eden DAW — DSP automation evaluation & application

use crate::app::models::*;
use crate::engine::AudioTrack;

// ── Automation evaluation ────────────────────────────────────────────────────

/// Given the project and a beat position, returns a map of
/// "track_id:slot_id:param_id" → interpolated value for every automation
/// clip that is active at that position.
pub fn evaluate_automation_at(project: &Project, pos_beats: f64) -> Vec<(String, f32)> {
    let mut results = Vec::new();
    for track in &project.tracks {
        if track.track_type != TrackType::Automation || !track.automation_enabled {
            continue;
        }
        for clip in &track.clips {
            if let Clip::Automation(ac) = clip {
                let clip_end = ac.start_time + ac.length;
                if pos_beats < ac.start_time || pos_beats >= clip_end || ac.length <= 0.0 {
                    continue;
                }
                let local = pos_beats - ac.start_time + ac.content_offset;
                let t = local; // Automation editor, playback and export all use source beats.
                let pts = &ac.points;
                if pts.is_empty() {
                    continue;
                }
                // Find surrounding points and linearly interpolate
                let val = if t <= pts[0].time {
                    pts[0].value
                } else if t >= pts[pts.len() - 1].time {
                    pts[pts.len() - 1].value
                } else {
                    let mut v = pts[0].value;
                    for w in pts.windows(2) {
                        let (t0, v0) = (w[0].time, w[0].value);
                        let (t1, v1) = (w[1].time, w[1].value);
                        if t >= t0 && t <= t1 {
                            let frac = if (t1 - t0).abs() < 1e-12 {
                                0.0
                            } else {
                                ((t - t0) / (t1 - t0)) as f32
                            };
                            v = v0 + (v1 - v0) * frac;
                            break;
                        }
                    }
                    v
                };
                results.push((ac.target_param.clone(), val));
            }
        }
    }
    results
}

/// Apply automation values to render tracks' instrument and effect params.
/// `auto_vals` is the output of `evaluate_automation_at`.
/// `project` is needed to look up slot_id → effect index mapping.
/// `track_id_to_render_idx` maps project track IDs to render track indices.
pub fn apply_automation_to_tracks(
    auto_vals: &[(String, f32)],
    render_tracks: &mut [AudioTrack],
    project: &Project,
    track_id_to_render_idx: &std::collections::HashMap<u32, usize>,
) {
    for (target_key, auto_val) in auto_vals {
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
            let is_instrument_slot = project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| t.rack.iter().find(|s| s.slot_id == slot_id))
                .map(|s| crate::modules::is_instrument(&s.plugin_name))
                .unwrap_or(false);

            if is_instrument_slot {
                for p in render_tracks[ri].instrument_params.iter_mut() {
                    if p.0 == param_id {
                        p.1 = *auto_val;
                    }
                }
            } else {
                let fx_slot_idx = project
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .and_then(|t| {
                        t.rack
                            .iter()
                            .filter(|s| s.enabled && crate::modules::is_effect(&s.plugin_name))
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
}

/// Compute song length in beats from the project's non-automation clips.
pub fn song_length_beats(project: &Project) -> f64 {
    project
        .tracks
        .iter()
        .filter(|t| t.track_type != TrackType::Automation)
        .flat_map(|t| t.clips.iter())
        .map(|c| c.start_time() + c.length())
        .fold(0.0_f64, f64::max)
        .max(1.0)
}

#[cfg(test)]
mod source_time_tests {
    use super::*;
    #[test]
    fn trimmed_automation_uses_source_beats_without_rescaling_curve() {
        let mut project = Project::default();
        let mut track = Track::new(1, "Auto", TrackType::Automation);
        let clip=serde_json::from_value(serde_json::json!({"Automation":{"points":[{"time":0.0,"value":0.0},{"time":4.0,"value":1.0}],"start_time":2.0,"content_offset":1.0,"length":2.0,"target_param":"2:1:gain","name":"Curve","color":[0,0,0,0]}})).unwrap();
        track.clips.push(clip);
        project.tracks.push(track);
        assert_eq!(
            evaluate_automation_at(&project, 2.0),
            vec![("2:1:gain".into(), 0.25)]
        );
        assert_eq!(
            evaluate_automation_at(&project, 3.0),
            vec![("2:1:gain".into(), 0.5)]
        );
    }
}
