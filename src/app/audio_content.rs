//! Audio edits create immutable derived assets. Undo swaps project revisions;
//! it never overwrites an imported source or rewrites a historical asset.
use super::{models::Clip, shared_content::new_id, state::AppState};
use std::path::PathBuf;

/// Preserve stereo pairing while retaining Eden's existing nearest-neighbor
/// sample-rate conversion. Only differing destination channel layouts mix/expand.
pub(crate) fn clipboard_frames(
    samples: &[f32],
    input_channels: usize,
    output_channels: usize,
    input_rate: u32,
    output_rate: u32,
) -> Vec<f32> {
    if samples.is_empty()
        || input_channels == 0
        || output_channels == 0
        || input_rate == 0
        || output_rate == 0
    {
        return Vec::new();
    }
    let frames = samples.len() / input_channels;
    if frames == 0 {
        return Vec::new();
    }
    let ratio = output_rate as f64 / input_rate as f64;
    let output_frames = (frames as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(output_frames * output_channels);
    for frame in 0..output_frames {
        let source = ((frame as f64 / ratio) as usize).min(frames - 1) * input_channels;
        if output_channels == 1 && input_channels > 1 {
            output.push(
                samples[source..source + input_channels].iter().sum::<f32>()
                    / input_channels as f32,
            );
        } else {
            for channel in 0..output_channels {
                output.push(samples[source + channel.min(input_channels - 1)]);
            }
        }
    }
    output
}

impl AppState {
    pub(crate) fn edit_linked_audio_windows(
        &mut self,
        mut edit: impl FnMut(&mut super::models::AudioClip),
    ) {
        let content = self
            .selected_clip
            .and_then(|(tid, ci)| {
                self.project
                    .tracks
                    .iter()
                    .find(|t| t.id == tid)
                    .and_then(|t| t.clips.get(ci))
            })
            .map(|c| c.content_id());
        if let Some(content) = content {
            for clip in self.project.tracks.iter_mut().flat_map(|t| &mut t.clips) {
                if let Clip::Audio(audio) = clip {
                    if audio.content.content_id == content {
                        edit(audio);
                    }
                }
            }
        }
    }

    pub(crate) fn commit_audio_render(
        &mut self,
        samples: &[f32],
        channels: usize,
        sample_rate: u32,
        description: &str,
    ) -> Result<(), String> {
        let (track_id, index) = self.selected_clip.ok_or("No audio clip selected")?;
        if !matches!(
            self.project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| t.clips.get(index)),
            Some(Clip::Audio(_))
        ) {
            return Err("No audio clip selected".into());
        }
        if channels == 0
            || channels > 2
            || samples.len() % channels != 0
            || samples.iter().any(|s| !s.is_finite())
        {
            return Err("Invalid audio render".into());
        }
        let base = self
            .last_save_path
            .as_ref()
            .and_then(|p| std::path::Path::new(p).parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?);
        let base = if base.is_absolute() {
            base
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(base)
        };
        let assets = base.join(".eden-assets");
        std::fs::create_dir_all(&assets).map_err(|e| format!("Create audio asset folder: {e}"))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = assets.join(format!("edit-{stamp}-{}.wav", new_id()));
        let result = if channels == 2 {
            crate::engine::save_wav_stereo(&path, samples, sample_rate)
        } else {
            crate::engine::save_wav_mono(&path, samples, sample_rate)
        };
        if let Err(error) = result {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        self.project.reconcile_clip_content();
        let snapshot = self.project.clone();
        if let Clip::Audio(clip) = &mut self
            .project
            .tracks
            .iter_mut()
            .find(|t| t.id == track_id)
            .unwrap()
            .clips[index]
        {
            clip.source_file = path.to_string_lossy().into_owned();
        }
        self.project.reconcile_clip_content();
        self.commands.push_undo_snapshot(snapshot, description);
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::{Project, Track, TrackType};
    #[test]
    fn derived_audio_is_linked_undoable_and_never_overwrites_imports() {
        let root = std::env::temp_dir().join(format!(
            "eden-content-test-{}-{}",
            std::process::id(),
            new_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("original.wav");
        crate::engine::save_wav_stereo(&source, &[0.1, -0.2, 0.3, -0.4], 44100).unwrap();
        let original = std::fs::read(&source).unwrap();
        let mut permissions = std::fs::metadata(&source).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&source, permissions).unwrap();
        let mut state = AppState::new();
        state.project = Project::default();
        state.last_save_path = Some(
            root.join("project.eden.json")
                .to_string_lossy()
                .into_owned(),
        );
        let mut track = Track::new(1, "Audio", TrackType::Audio);
        let clip: Clip=serde_json::from_value(serde_json::json!({"Audio":{"source_file":source,"start_time":0.0,"offset":0.0,"length":4.0,"gain":1.0,"name":"Source","color":[0,0,0,0]}})).unwrap();
        track.clips.push(clip);
        state.project.tracks.push(track);
        state.project.reconcile_clip_content();
        let linked = state.project.tracks[0].clips[0].linked_clone();
        let mut unique = linked.linked_clone();
        unique.make_unique();
        state.project.tracks[0].clips.extend([linked, unique]);
        state.select_clip(Some((1, 0)));
        state
            .commit_audio_render(&[-0.4, 0.3, -0.2, 0.1], 2, 44100, "Reverse")
            .unwrap();
        let files: Vec<_> = state.project.tracks[0]
            .clips
            .iter()
            .map(|c| match c {
                Clip::Audio(a) => a.source_file.clone(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(files[0], files[1]);
        assert_ne!(files[0], files[2]);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        let (samples, channels, sr) =
            crate::engine::load_audio_interleaved(std::path::Path::new(&files[0])).unwrap();
        assert_eq!(channels, 2);
        assert_eq!(sr, 44100);
        assert!((samples[0] + 0.4).abs() < 0.001);
        assert!((samples[1] - 0.3).abs() < 0.001);
        state.apply_history(false);
        if let Clip::Audio(a) = &state.project.tracks[0].clips[0] {
            assert_eq!(a.source_file, files[2]);
        }
        state.apply_history(true);
        if let Clip::Audio(a) = &state.project.tracks[0].clips[0] {
            assert_eq!(a.source_file, files[0]);
        }
        state
            .save_project(root.join("project.eden.json").to_str().unwrap())
            .unwrap();
        let mut reopened = AppState::new();
        reopened
            .load_project(root.join("project.eden.json").to_str().unwrap())
            .unwrap();
        assert_eq!(
            reopened.project.tracks[0].clips[0].content_id(),
            reopened.project.tracks[0].clips[1].content_id()
        );
        assert_eq!(std::fs::read(&source).unwrap(), original);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn device_gesture_records_one_before_image_and_rounds_discrete_parameters() {
        let mut state = AppState::new();
        state.project = Project::demo();
        state.project.tracks[0].rack = vec![crate::app::models::RackSlot::arpeggiator(1)];
        state.set_device_parameter(Some(0), 0, 1, 2.4);
        state.set_device_parameter(Some(0), 0, 1, 3.2);
        let mut input = crate::app::input::InputState::default();
        input.mouse_released = true;
        state.finish_parameter_gestures(&input);
        assert_eq!(state.project.tracks[0].rack[0].params[1].value, 3.0);
        state.apply_history(false);
        assert_eq!(state.project.tracks[0].rack[0].params[1].value, 1.0);
        assert!(!state.commands.can_undo());
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::*;
    #[test]
    fn stereo_clipboard_preserves_channels_and_frame_alignment() {
        let input = [0.2, -0.6, 0.8, -0.4];
        assert_eq!(clipboard_frames(&input, 2, 2, 44100, 44100), input);
        assert_eq!(
            clipboard_frames(&input, 2, 2, 22050, 44100),
            vec![0.2, -0.6, 0.2, -0.6, 0.8, -0.4, 0.8, -0.4]
        );
        assert_eq!(
            clipboard_frames(&[0.25, 0.5], 1, 2, 44100, 44100),
            vec![0.25, 0.25, 0.5, 0.5]
        );
    }
}
