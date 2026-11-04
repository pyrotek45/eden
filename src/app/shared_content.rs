//! Copy-on-write content revisions shared by clip instances and history snapshots.
//!
//! Editors mutate a revision through `DerefMut`; `Project::reconcile_clip_content`
//! publishes that revision to linked instances at command/UI transaction boundaries.
//! Placement and trim metadata live outside this object and never propagate.
use serde::{Deserialize, Serialize};
use std::{
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub fn new_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
pub fn reserve_ids_through(id: u64) {
    NEXT_ID.fetch_max(id.saturating_add(1), Ordering::Relaxed);
}

#[derive(Debug, Clone)]
pub struct SharedContent<T> {
    pub content_id: u64,
    pub revision: u64,
    value: Arc<T>,
}
impl<T> SharedContent<T> {
    pub fn new(value: T) -> Self {
        Self {
            content_id: new_id(),
            revision: new_id(),
            value: Arc::new(value),
        }
    }
    pub fn detach(&mut self) {
        self.content_id = new_id();
        self.revision = new_id();
    }
    pub fn shares_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }
}
impl<T> Deref for SharedContent<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T: Clone> DerefMut for SharedContent<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.revision = new_id();
        Arc::make_mut(&mut self.value)
    }
}
impl<T: Serialize> Serialize for SharedContent<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a, T> {
            content_id: u64,
            #[serde(flatten)]
            value: &'a T,
        }
        Wire {
            content_id: self.content_id,
            value: &*self.value,
        }
        .serialize(serializer)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for SharedContent<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire<T> {
            #[serde(default)]
            content_id: u64,
            #[serde(default)]
            content_revision: u64,
            #[serde(flatten)]
            value: T,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            content_id: wire.content_id,
            revision: wire.content_revision,
            value: Arc::new(wire.value),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{models::*, state::AppState};
    #[test]
    fn linked_edits_unique_variations_and_history_share_buffers() {
        let mut state = AppState::new();
        state.project = Project::demo();
        state.project.reconcile_clip_content();
        let original = state.project.tracks[0].clips[0].clone();
        let linked = original.linked_clone();
        state.project.tracks[0].clips.push(linked);
        let snapshot = state.project.clone();
        if let (Clip::Midi(a), Clip::Midi(b)) = (&original, &snapshot.tracks[0].clips[1]) {
            assert!(a.content.shares_storage(&b.content));
        }
        if let Clip::Midi(c) = &mut state.project.tracks[0].clips[1] {
            c.notes[0].pitch = 80;
        }
        state.project.reconcile_clip_content();
        for clip in &state.project.tracks[0].clips {
            if let Clip::Midi(c) = clip {
                assert_eq!(c.notes[0].pitch, 80);
            }
        }
        state
            .commands
            .push_undo_snapshot(snapshot, "Edit linked notes");
        state.apply_history(false);
        if let Clip::Midi(c) = &state.project.tracks[0].clips[0] {
            assert_eq!(c.notes[0].pitch, 60);
        }
        state.apply_history(true);
        let mut unique = state.project.tracks[0].clips[1].linked_clone();
        unique.make_unique();
        if let Clip::Midi(c) = &mut unique {
            c.notes[0].pitch = 90;
        }
        state.project.tracks[0].clips.push(unique);
        state.project.reconcile_clip_content();
        if let Clip::Midi(c) = &state.project.tracks[0].clips[0] {
            assert_eq!(c.notes[0].pitch, 80);
        }
        let json = serde_json::to_string(&state.project).unwrap();
        let mut loaded: Project = serde_json::from_str(&json).unwrap();
        loaded.reconcile_clip_content();
        assert_eq!(
            loaded.tracks[0].clips[0].content_id(),
            loaded.tracks[0].clips[1].content_id()
        );
        assert_ne!(
            loaded.tracks[0].clips[0].content_id(),
            loaded.tracks[0].clips[2].content_id()
        );
        if let (Clip::Midi(a), Clip::Midi(b)) =
            (&loaded.tracks[0].clips[0], &loaded.tracks[0].clips[1])
        {
            assert!(a.content.shares_storage(&b.content));
        }
    }
    #[test]
    fn legacy_loader_preserves_data_without_linking_equal_midi_names() {
        // Use a real old-format clip inside the current project's compatible envelope.
        let mut project = Project::default();
        let mut track = Track::new(1, "MIDI", TrackType::Midi);
        let old_clip =
            r#"{"Midi":{"notes":[],"start_time":0,"length":4,"name":"Same","color":[0,0,0,0]}}"#;
        track.clips.push(serde_json::from_str(old_clip).unwrap());
        track.clips.push(serde_json::from_str(old_clip).unwrap());
        project.tracks.push(track);
        project.reconcile_clip_content();
        assert_ne!(
            project.tracks[0].clips[0].content_id(),
            project.tracks[0].clips[1].content_id()
        );
        assert_ne!(
            project.tracks[0].clips[0].instance_id(),
            project.tracks[0].clips[1].instance_id()
        );
    }
}

#[cfg(test)]
mod automation_sharing_tests {
    use crate::app::models::*;
    #[test]
    fn automation_points_and_target_stay_linked_until_detached() {
        let mut project = Project::demo();
        project.reconcile_clip_content();
        let index = project
            .tracks
            .iter()
            .position(|t| t.track_type == TrackType::Automation)
            .unwrap();
        let linked = project.tracks[index].clips[0].linked_clone();
        project.tracks[index].clips.push(linked);
        if let Clip::Automation(c) = &mut project.tracks[index].clips[1] {
            c.points[0].value = 0.65;
            c.target_param = "1:2:cutoff".into();
        }
        project.reconcile_clip_content();
        if let Clip::Automation(c) = &project.tracks[index].clips[0] {
            assert_eq!(c.points[0].value, 0.65);
            assert_eq!(c.target_param, "1:2:cutoff");
        }
        let mut copy = project.tracks[index].clips[1].linked_clone();
        copy.make_unique();
        if let Clip::Automation(c) = &mut copy {
            c.points[0].value = 0.1;
        }
        project.tracks[index].clips.push(copy);
        project.reconcile_clip_content();
        if let Clip::Automation(c) = &project.tracks[index].clips[0] {
            assert_eq!(c.points[0].value, 0.65);
        }
    }
    #[test]
    fn contradictory_saved_links_are_rejected() {
        let mut project = Project::demo();
        let linked = project.tracks[0].clips[0].linked_clone();
        project.tracks[0].clips.push(linked);
        if let Clip::Midi(c) = &mut project.tracks[0].clips[1] {
            c.notes[0].pitch = 80;
        }
        assert!(project
            .validate_content_links()
            .unwrap_err()
            .contains("Conflicting"));
        project.reconcile_clip_content();
        assert!(project.validate_content_links().is_ok());
    }
}

#[cfg(test)]
mod shelf_tests {
    use crate::app::{commands::DeleteClips, models::*, state::AppState};
    #[test]
    fn shelf_is_saved_and_undo_does_not_leave_ghost_duplicates() {
        let mut state = AppState::new();
        state.project = Project::demo();
        state.sync_clip_library();
        let count = state.clip_library.len();
        state.select_clip(Some((1, 0)));
        state.selected_clips.insert((1, 0));
        state.duplicate_linked_clips();
        state.sync_clip_library();
        assert_eq!(state.clip_library.len(), count + 1);
        state.apply_history(false);
        assert_eq!(state.clip_library.len(), count);
        let clip = state.project.tracks[0].clips[0].clone();
        state.commands.execute(
            Box::new(DeleteClips {
                clips: vec![(1, 0, clip)],
            }),
            &mut state.project,
        );
        state.sync_clip_library();
        assert_eq!(state.project.shelf_clips.len(), 1);
        assert_eq!(state.clip_library.len(), count);
        let json = serde_json::to_string(&state.project).unwrap();
        let mut restored: Project = serde_json::from_str(&json).unwrap();
        restored.reconcile_clip_content();
        assert_eq!(restored.shelf_clips.len(), 1);
        state.apply_history(false);
        assert!(state.project.shelf_clips.is_empty());
        assert_eq!(state.clip_library.len(), count);
    }
}

#[cfg(test)]
mod rack_stage_tests {
    use crate::app::models::*;
    #[test]
    fn legacy_rack_order_is_repaired_without_reordering_effects_within_a_stage() {
        let mut project = Project::demo();
        project.tracks[0].rack = vec![
            RackSlot::gain(1),
            RackSlot::transpose(2),
            RackSlot::subtractive_synth(3),
            RackSlot::velocity(4),
            RackSlot::limiter(5),
        ];
        project.reconcile_clip_content();
        assert_eq!(
            project.tracks[0]
                .rack
                .iter()
                .map(|slot| slot.plugin_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Transpose", "Velocity", "Analog", "Gain", "Limiter"]
        );
    }
}
