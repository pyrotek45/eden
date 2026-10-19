//! Arrangement gestures use immutable metadata, not the previous preview frame.
//! Content remains owned by the project; a drag does not clone note/sample data.
use super::{
    input::{InputState, WidgetId},
    models::*,
    state::AppState,
};

#[derive(Clone, Debug)]
pub(crate) struct ClipBaseline {
    pub track: u32,
    pub index: usize,
    pub start: f64,
    pub length: f64,
    pub offset: Option<f64>,
    /// Source end, captured once so a waveform becoming ready cannot change a drag.
    pub source_beats: Option<f64>,
}

impl AppState {
    /// Duplicate a selection by its complete span, then point Edit at the copies.
    /// Current legacy semantics: MIDI/automation data is independent, audio shares storage.
    pub(crate) fn duplicate_selected_clips(&mut self) {
        self.cancel_clip_drag();
        let mut ids: Vec<_> = self.selected_clips.iter().copied().collect();
        if ids.is_empty() {
            ids.extend(self.selected_clip);
        }
        ids.sort_unstable();
        let mut clips: Vec<_> = ids
            .iter()
            .filter_map(|&(tid, ci)| {
                self.project
                    .tracks
                    .iter()
                    .find(|t| t.id == tid)
                    .and_then(|t| t.clips.get(ci))
                    .map(|c| (tid, c.clone()))
            })
            .collect();
        if clips.is_empty() {
            return;
        }
        let start = clips
            .iter()
            .map(|(_, c)| c.start_time())
            .fold(f64::INFINITY, f64::min);
        let end = clips
            .iter()
            .map(|(_, c)| c.start_time() + c.length())
            .fold(f64::NEG_INFINITY, f64::max);
        let before = self.project.clone();
        self.selected_clips.clear();
        for (tid, mut c) in clips.drain(..) {
            c.set_start_time(c.start_time() + end - start);
            let t = self
                .project
                .tracks
                .iter_mut()
                .find(|t| t.id == tid)
                .unwrap();
            let key = (tid, t.clips.len());
            t.clips.push(c);
            self.selected_clips.insert(key);
            self.selected_clip = Some(key);
        }
        self.commands.push_undo_snapshot(before, "Duplicate Clips");
        self.dirty = true;
    }

    /// Editors read project data directly, but their index selections are view state.
    /// Never carry a previous clip's note/point/sample selection into another clip.
    pub(crate) fn sync_editor_selection(&mut self) {
        self.selected_clips.retain(|&(tid, ci)| {
            self.project
                .tracks
                .iter()
                .find(|t| t.id == tid)
                .is_some_and(|t| ci < t.clips.len())
        });
        let context = self.selected_clip.and_then(|(tid, ci)| {
            self.project
                .tracks
                .iter()
                .find(|t| t.id == tid)
                .and_then(|t| t.clips.get(ci))
                .map(|c| {
                    let source = match c {
                        Clip::Audio(a) => a.source_file.clone(),
                        Clip::Midi(_) => "MIDI".into(),
                        Clip::Automation(_) => "AUTOMATION".into(),
                    };
                    (tid, ci, source)
                })
        });
        if context.is_none() {
            self.selected_clip = None;
        }
        if context != self.editor_clip_context {
            self.piano_roll_selected_notes.clear();
            self.piano_roll_drag_note = None;
            self.piano_roll_moving = false;
            self.automation_selected.clear();
            self.automation_drag_idx = None;
            self.automation_drag_orig = None;
            self.audio_editor_selection = None;
            self.editor_clip_context = context;
        }
    }

    pub(crate) fn begin_clip_drag(&mut self, track: u32, index: usize) {
        self.clip_drag_baseline.clear();
        self.clip_drag_moved = false;
        let mut selection: Vec<_> = if self.selected_clips.contains(&(track, index)) {
            self.selected_clips.iter().copied().collect()
        } else {
            vec![(track, index)]
        };
        selection.sort_unstable();
        let bpm = self.project.tempo_map.bpm_at(0.0).max(1.0);
        for (tid, ci) in selection {
            if let Some(c) = self
                .project
                .tracks
                .iter()
                .find(|t| t.id == tid)
                .and_then(|t| t.clips.get(ci))
            {
                let (offset, source_beats) = if let Clip::Audio(a) = c {
                    let duration = self
                        .waveform_cache
                        .get(&a.source_file)
                        .map(|(_, d)| *d)
                        // WAV headers provide bounds even before the waveform worker finishes.
                        .or_else(|| {
                            hound::WavReader::open(&a.source_file)
                                .ok()
                                .map(|r| r.duration() as f64 / r.spec().sample_rate as f64)
                        });
                    (Some(a.offset), duration.map(|d| d * bpm / 60.0))
                } else {
                    (None, None)
                };
                self.clip_drag_baseline.push(ClipBaseline {
                    track: tid,
                    index: ci,
                    start: c.start_time(),
                    length: c.length(),
                    offset,
                    source_beats,
                });
            }
        }
    }

    fn restore_clip_baseline(&mut self) {
        for b in &self.clip_drag_baseline {
            if let Some(c) = self
                .project
                .tracks
                .iter_mut()
                .find(|t| t.id == b.track)
                .and_then(|t| t.clips.get_mut(b.index))
            {
                c.set_start_time(b.start);
                c.set_length(b.length);
                if let (Clip::Audio(a), Some(offset)) = (c, b.offset) {
                    a.offset = offset;
                }
            }
        }
    }

    pub(crate) fn cancel_clip_drag(&mut self) {
        self.restore_clip_baseline();
        self.clip_drag_baseline.clear();
        self.clip_drag_ghost_positions.clear();
        self.clip_drag_target_track = None;
        self.clip_drag_target_valid = false;
        self.clip_drag_is_copy = false;
    }

    pub(crate) fn update_clip_drag(&mut self, input: &InputState, zoom: f64) {
        let widget = if input.dragging {
            input.drag_widget
        } else {
            input.active_widget
        };
        let (tid, ci) = match widget {
            WidgetId::ClipBody(t, c)
            | WidgetId::ClipLeftHandle(t, c)
            | WidgetId::ClipRightHandle(t, c) => (t, c),
            _ => return,
        };
        if (!input.dragging && !input.mouse_released) || self.clip_drag_baseline.is_empty() {
            return;
        }
        self.clip_drag_moved |= input.dragging
            || (input.mouse_x - input.drag_start_x).abs() > 3
            || (input.mouse_y - input.drag_start_y).abs() > 3;
        if !self.clip_drag_moved {
            if input.mouse_released {
                self.cancel_clip_drag();
            }
            return;
        }
        let Some(anchor) = self
            .clip_drag_baseline
            .iter()
            .find(|b| b.track == tid && b.index == ci)
            .cloned()
        else {
            return;
        };
        let raw_delta = (input.mouse_x - input.drag_start_x) as f64 / zoom;
        // Snap the edited timeline boundary, never a length relative to beat zero.
        let left = matches!(widget, WidgetId::ClipLeftHandle(_, _));
        let moving = matches!(widget, WidgetId::ClipBody(_, _));
        let boundary = if moving || left {
            anchor.start
        } else {
            anchor.start + anchor.length
        };
        let mut delta = self.snap.snap(boundary + raw_delta) - boundary;
        let bpm = self.project.tempo_map.bpm_at(0.0).max(1.0);
        // A minimum is a safety bound, not the grid: do not grow existing short clips.
        const MIN_CLIP_BEATS: f64 = 1.0 / 128.0;
        let mut low = f64::NEG_INFINITY;
        let mut high = f64::INFINITY;
        for b in &self.clip_drag_baseline {
            let minimum = MIN_CLIP_BEATS.min(b.length);
            if moving || left {
                low = low.max(-b.start);
            }
            if left {
                high = high.min(b.length - minimum);
                if let Some(offset) = b.offset {
                    low = low.max(-offset * bpm / 60.0);
                }
            } else if !moving {
                low = low.max(minimum - b.length);
                if let (Some(source), Some(offset)) = (b.source_beats, b.offset) {
                    // Already oversized legacy clips can shrink, but cannot grow farther.
                    high = high.min((source - offset * bpm / 60.0 - b.length).max(0.0));
                }
            }
        }
        delta = delta.clamp(low, high);
        let mut destinations = Vec::new();
        let mut valid = true;
        if moving {
            let mut y = self.track_area_top() - self.arrangement.scroll_y;
            let target = self.project.tracks.iter().position(|t| {
                let inside = input.mouse_y >= y && input.mouse_y < y + t.height;
                y += t.height;
                inside
            });
            let source = self.project.tracks.iter().position(|t| t.id == tid);
            let shift = match (source, target) {
                (Some(s), Some(t)) => t as isize - s as isize,
                _ => {
                    valid = false;
                    0
                }
            };
            self.clip_drag_target_track = target.map(|i| self.project.tracks[i].id);
            self.clip_drag_ghost_positions.clear();
            for b in &self.clip_drag_baseline {
                let source_index = self
                    .project
                    .tracks
                    .iter()
                    .position(|t| t.id == b.track)
                    .unwrap();
                let dest = (source_index as isize + shift)
                    .try_into()
                    .ok()
                    .and_then(|i: usize| self.project.tracks.get(i));
                let dest_id = dest.map(|t| t.id).unwrap_or(b.track);
                valid &= dest
                    .map(|t| t.track_type == self.project.tracks[source_index].track_type)
                    .unwrap_or(false);
                destinations.push(dest_id);
                self.clip_drag_ghost_positions
                    .push((dest_id, b.track, b.index, b.start + delta));
            }
            self.clip_drag_target_valid = valid;
        }
        // Every preview and release is computed from pointer-down, including invalid drops.
        self.restore_clip_baseline();
        if !valid {
            if input.mouse_released {
                self.push_status(
                    "Drop cancelled: every selected clip needs a compatible destination lane",
                );
                self.cancel_clip_drag();
            }
            return;
        }
        let crossed = moving
            && self
                .clip_drag_baseline
                .iter()
                .zip(&destinations)
                .any(|(b, d)| b.track != *d);
        let changed = delta.abs() > 1e-9 || crossed;
        // A click is never a duplicate, even when Ctrl is held.
        let copy = moving && self.clip_drag_is_copy;
        if input.mouse_released && !changed {
            self.cancel_clip_drag();
            return;
        }
        // Snapshot only after restoring originals, and only once, at commit.
        let before = input.mouse_released.then(|| self.project.clone());
        if moving && input.mouse_released && (copy || crossed) {
            let mut additions = Vec::new();
            for (b, dest) in self.clip_drag_baseline.iter().zip(&destinations) {
                let mut c = self
                    .project
                    .tracks
                    .iter()
                    .find(|t| t.id == b.track)
                    .unwrap()
                    .clips[b.index]
                    .clone();
                c.set_start_time(b.start + delta);
                additions.push((*dest, c));
            }
            if !copy {
                // Descending removals preserve indices when several clips share a lane.
                for b in self.clip_drag_baseline.iter().rev() {
                    self.project
                        .tracks
                        .iter_mut()
                        .find(|t| t.id == b.track)
                        .unwrap()
                        .clips
                        .remove(b.index);
                }
            }
            self.selected_clips.clear();
            for (dest, clip) in additions {
                let t = self
                    .project
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == dest)
                    .unwrap();
                let index = t.clips.len();
                t.clips.push(clip);
                self.selected_clips.insert((dest, index));
                self.selected_clip = Some((dest, index));
            }
            self.selected_track = self.selected_clip.map(|(tid, _)| tid);
            self.selected_tracks.clear();
            self.selected_tracks.extend(self.selected_track);
        } else if !copy {
            for b in &self.clip_drag_baseline {
                let c = &mut self
                    .project
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == b.track)
                    .unwrap()
                    .clips[b.index];
                if moving {
                    c.set_start_time(b.start + delta);
                } else if left {
                    c.set_start_time(b.start + delta);
                    c.set_length(b.length - delta);
                    if let (Clip::Audio(a), Some(offset)) = (c, b.offset) {
                        a.offset = (offset + delta * 60.0 / bpm).max(0.0);
                    }
                } else {
                    c.set_length(b.length + delta);
                }
            }
        }
        if let Some(before) = before {
            self.commands.push_undo_snapshot(
                before,
                if copy {
                    "Copy Clips"
                } else if moving {
                    "Move Clips"
                } else {
                    "Trim Clips"
                },
            );
            self.dirty = true;
            self.clip_drag_baseline.clear();
            self.clip_drag_ghost_positions.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> AppState {
        let mut s = AppState::new();
        s.project.tracks.clear();
        s.project.tempo_map.changes[0].bpm = 120.0;
        let mut t = Track::new(1, "MIDI", TrackType::Midi);
        for (start, length) in [(2.0, 4.0), (5.0, 6.0)] {
            t.clips.push(Clip::Midi(MidiClip {
                notes: vec![],
                start_time: start,
                length,
                name: "Same name".into(),
                color: [0; 4],
            }));
        }
        s.project.tracks.push(t);
        s.selected_clips.extend([(1, 0), (1, 1)]);
        s.snap.enabled = false;
        s
    }
    fn frame(s: &mut AppState, widget: WidgetId, dx: i32, release: bool, lane: usize) {
        let mut i = InputState::default();
        i.mouse_x = dx;
        i.drag_start_x = 0;
        i.mouse_y = s.track_area_top()
            + s.project
                .tracks
                .iter()
                .take(lane)
                .map(|t| t.height)
                .sum::<i32>()
            + 10;
        i.drag_start_y = i.mouse_y;
        i.drag_widget = widget;
        i.active_widget = widget;
        i.dragging = !release;
        i.mouse_released = release;
        s.update_clip_drag(&i, 100.0);
    }
    fn positions(s: &AppState) -> Vec<(f64, f64)> {
        s.project.tracks[0]
            .clips
            .iter()
            .map(|c| (c.start_time(), c.length()))
            .collect()
    }
    #[test]
    fn repeated_left_trim_is_stable_and_one_undo_restores_entire_group() {
        let mut s = setup();
        s.begin_clip_drag(1, 0);
        for _ in 0..3 {
            frame(&mut s, WidgetId::ClipLeftHandle(1, 0), 100, false, 0);
            assert_eq!(positions(&s), vec![(3.0, 3.0), (6.0, 5.0)]);
        }
        frame(&mut s, WidgetId::ClipLeftHandle(1, 0), 150, true, 0);
        assert_eq!(positions(&s), vec![(3.5, 2.5), (6.5, 4.5)]);
        s.commands.undo(&mut s.project);
        assert_eq!(positions(&s), vec![(2.0, 4.0), (5.0, 6.0)]);
        assert!(!s.commands.can_undo());
        s.commands.redo(&mut s.project);
        assert_eq!(positions(&s), vec![(3.5, 2.5), (6.5, 4.5)]);
    }
    #[test]
    fn right_trim_undo_uses_pointer_down_not_last_preview() {
        let mut s = setup();
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipRightHandle(1, 0), 100, false, 0);
        frame(&mut s, WidgetId::ClipRightHandle(1, 0), 150, true, 0);
        assert_eq!(positions(&s), vec![(2.0, 5.5), (5.0, 7.5)]);
        s.commands.undo(&mut s.project);
        assert_eq!(positions(&s), vec![(2.0, 4.0), (5.0, 6.0)]);
    }
    #[test]
    fn move_against_zero_preserves_group_spacing_and_undo() {
        let mut s = setup();
        s.begin_clip_drag(1, 1);
        frame(&mut s, WidgetId::ClipBody(1, 1), -1000, false, 0);
        frame(&mut s, WidgetId::ClipBody(1, 1), -1000, true, 0);
        assert_eq!(positions(&s), vec![(0.0, 4.0), (3.0, 6.0)]);
        s.commands.undo(&mut s.project);
        assert_eq!(positions(&s), vec![(2.0, 4.0), (5.0, 6.0)]);
    }
    #[test]
    fn escape_restores_preview_and_does_not_create_history_or_dirty_project() {
        let mut s = setup();
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipLeftHandle(1, 0), 100, false, 0);
        s.cancel_clip_drag();
        assert_eq!(positions(&s), vec![(2.0, 4.0), (5.0, 6.0)]);
        assert!(!s.commands.can_undo());
        assert!(!s.dirty);
        frame(&mut s, WidgetId::ClipLeftHandle(1, 0), 200, true, 0);
        assert!(!s.commands.can_undo());
    }
    #[test]
    fn invalid_copy_and_move_are_atomic_even_after_valid_preview() {
        for copy in [false, true] {
            let mut s = setup();
            s.project
                .tracks
                .push(Track::new(2, "Audio", TrackType::Audio));
            s.clip_drag_is_copy = copy;
            s.begin_clip_drag(1, 0);
            frame(&mut s, WidgetId::ClipBody(1, 0), 100, false, 0);
            frame(&mut s, WidgetId::ClipBody(1, 0), 200, true, 1);
            assert_eq!(positions(&s), vec![(2.0, 4.0), (5.0, 6.0)]);
            assert!(s.project.tracks[1].clips.is_empty());
            assert!(!s.commands.can_undo());
            assert!(!s.dirty);
        }
    }
    #[test]
    fn group_drop_rejects_secondary_incompatible_lane() {
        let mut s = setup();
        let mut t = Track::new(2, "MIDI2", TrackType::Midi);
        t.clips.push(s.project.tracks[0].clips.pop().unwrap());
        s.project.tracks.push(t);
        s.project
            .tracks
            .push(Track::new(3, "Audio", TrackType::Audio));
        s.selected_clips.clear();
        s.selected_clips.extend([(1, 0), (2, 0)]);
        s.clip_drag_is_copy = true;
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipBody(1, 0), 100, true, 1);
        assert_eq!(
            s.project
                .tracks
                .iter()
                .map(|t| t.clips.len())
                .collect::<Vec<_>>(),
            vec![1, 1, 0]
        );
        assert!(!s.commands.can_undo());
    }
    #[test]
    fn valid_cross_lane_copy_keeps_originals_and_selects_both_copies() {
        let mut s = setup();
        s.project
            .tracks
            .push(Track::new(2, "MIDI2", TrackType::Midi));
        s.clip_drag_is_copy = true;
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipBody(1, 0), 100, true, 1);
        assert_eq!(positions(&s), vec![(2.0, 4.0), (5.0, 6.0)]);
        assert_eq!(s.project.tracks[1].clips[0].start_time(), 3.0);
        assert_eq!(s.project.tracks[1].clips[1].start_time(), 6.0);
        assert_eq!(s.selected_clips.len(), 2);
        s.commands.undo(&mut s.project);
        assert!(s.project.tracks[1].clips.is_empty());
    }
    #[test]
    fn click_with_ctrl_is_not_a_copy_or_history_entry() {
        let mut s = setup();
        s.clip_drag_is_copy = true;
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipBody(1, 0), 0, true, 0);
        assert_eq!(s.project.tracks[0].clips.len(), 2);
        assert!(!s.commands.can_undo());
    }
    #[test]
    fn right_trim_snaps_absolute_end_not_duration() {
        let mut s = setup();
        s.selected_clips.clear();
        s.project.tracks[0].clips[0].set_start_time(2.25);
        s.snap.enabled = true;
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipRightHandle(1, 0), 60, true, 0);
        let c = &s.project.tracks[0].clips[0];
        assert_eq!(c.start_time() + c.length(), 7.0);
    }
    #[test]
    fn audio_group_trim_preserves_offsets_and_source_bounds_through_history() {
        let mut s = setup();
        s.project.tracks[0].track_type = TrackType::Audio;
        s.project.tracks[0].clips = vec![Clip::Audio(AudioClip {
            source_file: "bounds.wav".into(),
            start_time: 2.0,
            offset: 0.5,
            length: 4.0,
            gain: 1.0,
            name: "A".into(),
            color: [0; 4],
            fade_in: 0.0,
            fade_out: 0.0,
        })];
        s.selected_clips.clear();
        s.selected_clips.insert((1, 0));
        s.waveform_cache.insert("bounds.wav".into(), (vec![], 3.0));
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipLeftHandle(1, 0), -400, false, 0);
        frame(&mut s, WidgetId::ClipLeftHandle(1, 0), -400, true, 0);
        assert_eq!(positions(&s), vec![(1.0, 5.0)]);
        if let Clip::Audio(a) = &s.project.tracks[0].clips[0] {
            assert_eq!(a.offset, 0.0)
        }
        s.commands.undo(&mut s.project);
        assert_eq!(positions(&s), vec![(2.0, 4.0)]);
        if let Clip::Audio(a) = &s.project.tracks[0].clips[0] {
            assert_eq!(a.offset, 0.5)
        }
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipRightHandle(1, 0), 1000, true, 0);
        assert_eq!(positions(&s), vec![(2.0, 5.0)]);
    }
    #[test]
    fn duplicate_preserves_group_span_and_editor_targets_new_variation() {
        let mut s = setup();
        s.selected_clip = Some((1, 0));
        s.duplicate_selected_clips();
        assert_eq!(
            positions(&s),
            vec![(2.0, 4.0), (5.0, 6.0), (11.0, 4.0), (14.0, 6.0)]
        );
        assert_eq!(s.selected_clip, Some((1, 3)));
        assert_eq!(s.selected_clips.len(), 2);
        s.commands.undo(&mut s.project);
        s.sync_editor_selection();
        assert!(s.selected_clip.is_none());
        assert!(s.selected_clips.is_empty());
        assert_eq!(positions(&s), vec![(2.0, 4.0), (5.0, 6.0)]);
    }
    #[test]
    fn switching_clip_clears_editor_selections_but_keeps_view_zoom() {
        let mut s = setup();
        s.selected_clip = Some((1, 0));
        s.sync_editor_selection();
        s.piano_roll_selected_notes.insert(0);
        s.automation_selected.push(0);
        s.audio_editor_selection = Some((1.0, 2.0));
        s.audio_editor_zoom = 3.0;
        s.selected_clip = Some((1, 1));
        s.sync_editor_selection();
        assert!(s.piano_roll_selected_notes.is_empty());
        assert!(s.automation_selected.is_empty());
        assert!(s.audio_editor_selection.is_none());
        assert_eq!(s.audio_editor_zoom, 3.0);
    }
    #[test]
    fn ctrl_click_off_grid_does_not_snap_or_copy() {
        let mut s = setup();
        s.project.tracks[0].clips[0].set_start_time(2.25);
        s.snap.enabled = true;
        s.clip_drag_is_copy = true;
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipBody(1, 0), 0, true, 0);
        assert_eq!(positions(&s), vec![(2.25, 4.0), (5.0, 6.0)]);
        assert!(!s.commands.can_undo());
    }
    #[test]
    fn cold_waveform_wav_header_still_limits_audio_trim() {
        let path =
            std::env::temp_dir().join(format!("eden-clip-bounds-{}.wav", std::process::id()));
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 1000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..6000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        let mut s = setup();
        s.project.tracks[0].clips = vec![Clip::Audio(AudioClip {
            source_file: path.to_string_lossy().into(),
            start_time: 2.0,
            offset: 0.5,
            length: 4.0,
            gain: 1.0,
            name: "A".into(),
            color: [0; 4],
            fade_in: 0.0,
            fade_out: 0.0,
        })];
        s.selected_clips.clear();
        s.begin_clip_drag(1, 0);
        frame(&mut s, WidgetId::ClipRightHandle(1, 0), 1000, true, 0);
        assert_eq!(positions(&s), vec![(2.0, 5.0)]);
        std::fs::remove_file(path).unwrap();
    }
}
