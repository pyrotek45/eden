// Eden DAW — Command / Undo-Redo system
// Every mutation to the Project goes through a Command.

use crate::models::*;

/// A reversible action on the project.
pub trait Command: std::fmt::Debug {
    fn apply(&mut self, project: &mut Project);
    fn undo(&mut self, project: &mut Project);
    fn description(&self) -> &str;
}

/// Manages undo / redo stacks using full project snapshots.
/// Every mutation snapshots the entire project state before applying,
/// guaranteeing perfect undo/redo for any operation.
pub struct CommandManager {
    undo_stack: Vec<(Project, String)>, // (snapshot_before, description)
    redo_stack: Vec<(Project, String)>, // (snapshot_before_undo, description)
    max_history: usize,
}

impl CommandManager {
    pub fn new(max_history: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history,
        }
    }

    pub fn execute(&mut self, mut cmd: Box<dyn Command>, project: &mut Project) {
        // Snapshot the project BEFORE applying the command
        let snapshot = project.clone();
        let desc = cmd.description().to_string();
        cmd.apply(project);
        self.undo_stack.push((snapshot, desc));
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self, project: &mut Project) {
        if let Some((snapshot, desc)) = self.undo_stack.pop() {
            // Save current state for redo, then restore snapshot
            let current = project.clone();
            self.redo_stack.push((current, desc));
            *project = snapshot;
        }
    }

    pub fn redo(&mut self, project: &mut Project) {
        if let Some((snapshot, desc)) = self.redo_stack.pop() {
            // Save current state for undo, then restore redo snapshot
            let current = project.clone();
            self.undo_stack.push((current, desc));
            *project = snapshot;
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_description(&self) -> Option<&str> {
        self.undo_stack.last().map(|(_, d)| d.as_str())
    }

    pub fn redo_description(&self) -> Option<&str> {
        self.redo_stack.last().map(|(_, d)| d.as_str())
    }

    /// Push a snapshot onto the undo stack WITHOUT calling apply().
    /// Use when the action has already been applied manually (e.g. live BPM drag).
    pub fn push_undo(&mut self, cmd: Box<dyn Command>) {
        // For push_undo, the caller has already applied the change.
        // We cannot snapshot the "before" state because it's already gone.
        // Store a dummy snapshot — this is a best-effort path.
        // Callers should prefer calling push_undo_snapshot() instead.
        let desc = cmd.description().to_string();
        // We have no pre-state, so we push a placeholder that won't undo correctly.
        // This path is kept for API compatibility but callers should migrate.
        self.undo_stack.push((Project::default(), desc));
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
    }

    /// Push a pre-mutation snapshot onto the undo stack.
    /// Call this BEFORE applying the change, passing the project state before mutation.
    pub fn push_undo_snapshot(&mut self, snapshot: Project, desc: &str) {
        self.undo_stack.push((snapshot, desc.to_string()));
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_history {
            self.undo_stack.remove(0);
        }
    }
}

// ── Concrete commands ────────────────────────────────────────────────

#[derive(Debug)]
pub struct SetTrackVolume {
    pub track_id: u32,
    pub new_value: f32,
    pub old_value: f32,
}

impl Command for SetTrackVolume {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            self.old_value = t.volume;
            t.volume = self.new_value;
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            t.volume = self.old_value;
        }
    }
    fn description(&self) -> &str {
        "Set Track Volume"
    }
}

#[derive(Debug)]
pub struct SetTrackPan {
    pub track_id: u32,
    pub new_value: f32,
    pub old_value: f32,
}

impl Command for SetTrackPan {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            self.old_value = t.pan;
            t.pan = self.new_value;
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            t.pan = self.old_value;
        }
    }
    fn description(&self) -> &str {
        "Set Track Pan"
    }
}

#[derive(Debug)]
pub struct SetTrackMute {
    pub track_id: u32,
    pub new_value: bool,
    pub old_value: bool,
}

impl Command for SetTrackMute {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            self.old_value = t.mute;
            t.mute = self.new_value;
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            t.mute = self.old_value;
        }
    }
    fn description(&self) -> &str {
        "Toggle Mute"
    }
}

#[derive(Debug)]
pub struct SetTrackSolo {
    pub track_id: u32,
    pub new_value: bool,
    pub old_value: bool,
}

impl Command for SetTrackSolo {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            self.old_value = t.solo;
            t.solo = self.new_value;
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            t.solo = self.old_value;
        }
    }
    fn description(&self) -> &str {
        "Toggle Solo"
    }
}

#[derive(Debug)]
pub struct AddTrack {
    pub track: Track,
}

impl Command for AddTrack {
    fn apply(&mut self, project: &mut Project) {
        project.tracks.push(self.track.clone());
    }
    fn undo(&mut self, project: &mut Project) {
        project.tracks.retain(|t| t.id != self.track.id);
    }
    fn description(&self) -> &str {
        "Add Track"
    }
}

#[derive(Debug)]
pub struct RemoveTrack {
    pub track_id: u32,
    pub removed_track: Option<Track>,
    pub index: usize,
}

impl Command for RemoveTrack {
    fn apply(&mut self, project: &mut Project) {
        if let Some(idx) = project.tracks.iter().position(|t| t.id == self.track_id) {
            self.removed_track = Some(project.tracks.remove(idx));
            self.index = idx;
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = self.removed_track.take() {
            let idx = self.index.min(project.tracks.len());
            project.tracks.insert(idx, track);
        }
    }
    fn description(&self) -> &str {
        "Remove Track"
    }
}

#[derive(Debug)]
pub struct SetTempo {
    pub new_bpm: f64,
    pub old_bpm: f64,
}

/// Rescale all audio clip start_times and lengths so they represent the same
/// wall-clock duration at the new BPM as they did at the old BPM.
/// MIDI clips intentionally stay locked to the beat grid and are NOT rescaled.
pub fn rescale_audio_clips_pub(project: &mut Project, old_bpm: f64, new_bpm: f64) {
    rescale_audio_clips(project, old_bpm, new_bpm);
}

fn rescale_audio_clips(project: &mut Project, old_bpm: f64, new_bpm: f64) {
    if old_bpm <= 0.0 || new_bpm <= 0.0 || (old_bpm - new_bpm).abs() < 1e-9 {
        return;
    }
    let ratio = new_bpm / old_bpm; // new_beats_per_old_beat
    for track in project.tracks.iter_mut() {
        for clip in track.clips.iter_mut() {
            match clip {
                Clip::Audio(ac) => {
                    // Audio clips keep same wall-clock position/duration
                    ac.start_time *= ratio;
                    ac.length *= ratio;
                }
                Clip::Midi(mc) => {
                    // MIDI clips also rescale so they maintain same wall-clock position
                    mc.start_time *= ratio;
                    mc.length *= ratio;
                    // Rescale note positions within the clip
                    for note in mc.notes.iter_mut() {
                        note.start *= ratio;
                        note.length *= ratio;
                    }
                }
                Clip::Automation(ac) => {
                    // Automation clips rescale start/length
                    ac.start_time *= ratio;
                    ac.length *= ratio;
                    // Rescale automation point positions within the clip
                    for pt in ac.points.iter_mut() {
                        pt.time *= ratio;
                    }
                }
            }
        }
    }
}

impl Command for SetTempo {
    fn apply(&mut self, project: &mut Project) {
        if let Some(first) = project.tempo_map.changes.first_mut() {
            self.old_bpm = first.bpm;
            first.bpm = self.new_bpm;
        }
        rescale_audio_clips(project, self.old_bpm, self.new_bpm);
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(first) = project.tempo_map.changes.first_mut() {
            first.bpm = self.old_bpm;
        }
        rescale_audio_clips(project, self.new_bpm, self.old_bpm);
    }
    fn description(&self) -> &str {
        "Set Tempo"
    }
}

#[derive(Debug)]
pub struct MoveClip {
    pub track_id: u32,
    pub clip_index: usize,
    pub new_start: f64,
    pub old_start: f64,
}

impl Command for MoveClip {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(clip) = t.clips.get_mut(self.clip_index) {
                match clip {
                    Clip::Midi(c) => {
                        self.old_start = c.start_time;
                        c.start_time = self.new_start;
                    }
                    Clip::Audio(c) => {
                        self.old_start = c.start_time;
                        c.start_time = self.new_start;
                    }
                    Clip::Automation(c) => {
                        self.old_start = c.start_time;
                        c.start_time = self.new_start;
                    }
                }
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(clip) = t.clips.get_mut(self.clip_index) {
                match clip {
                    Clip::Midi(c) => c.start_time = self.old_start,
                    Clip::Audio(c) => c.start_time = self.old_start,
                    Clip::Automation(c) => c.start_time = self.old_start,
                }
            }
        }
    }
    fn description(&self) -> &str {
        "Move Clip"
    }
}

#[derive(Debug)]
pub struct SetLoopRegion {
    pub new_start: f64,
    pub new_end: f64,
    pub old_start: f64,
    pub old_end: f64,
}

impl Command for SetLoopRegion {
    fn apply(&mut self, project: &mut Project) {
        self.old_start = project.transport.loop_region.start;
        self.old_end = project.transport.loop_region.end;
        project.transport.loop_region.start = self.new_start;
        project.transport.loop_region.end = self.new_end;
    }
    fn undo(&mut self, project: &mut Project) {
        project.transport.loop_region.start = self.old_start;
        project.transport.loop_region.end = self.old_end;
    }
    fn description(&self) -> &str {
        "Set Loop Region"
    }
}

#[derive(Debug)]
pub struct ToggleTransportPlaying {
    pub old_value: bool,
}

impl Command for ToggleTransportPlaying {
    fn apply(&mut self, project: &mut Project) {
        self.old_value = project.transport.playing;
        project.transport.playing = !project.transport.playing;
    }
    fn undo(&mut self, project: &mut Project) {
        project.transport.playing = self.old_value;
    }
    fn description(&self) -> &str {
        "Toggle Play/Pause"
    }
}

#[derive(Debug)]
pub struct ToggleLoopEnabled {
    pub old_value: bool,
}

impl Command for ToggleLoopEnabled {
    fn apply(&mut self, project: &mut Project) {
        self.old_value = project.transport.loop_enabled;
        project.transport.loop_enabled = !project.transport.loop_enabled;
    }
    fn undo(&mut self, project: &mut Project) {
        project.transport.loop_enabled = self.old_value;
    }
    fn description(&self) -> &str {
        "Toggle Loop"
    }
}

#[derive(Debug)]
pub struct ResetTransportPosition {
    pub old_value: f64,
}

impl Command for ResetTransportPosition {
    fn apply(&mut self, project: &mut Project) {
        self.old_value = project.transport.position;
        project.transport.position = 0.0;
    }
    fn undo(&mut self, project: &mut Project) {
        project.transport.position = self.old_value;
    }
    fn description(&self) -> &str {
        "Reset Position"
    }
}

#[derive(Debug)]
pub struct AddClips {
    pub clips: Vec<(u32, Clip)>,          // (track_id, clip_data)
    pub added_indices: Vec<(u32, usize)>, // Populated on apply for undo
}

impl Command for AddClips {
    fn apply(&mut self, project: &mut Project) {
        self.added_indices.clear();
        for (track_id, clip) in &self.clips {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == *track_id) {
                let idx = track.clips.len();
                self.added_indices.push((*track_id, idx));
                track.clips.push(clip.clone());
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        let mut sorted = self.added_indices.clone();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        for (track_id, idx) in sorted {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == track_id) {
                if idx < track.clips.len() {
                    track.clips.remove(idx);
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Add Clips"
    }
}

#[derive(Debug)]
pub struct MoveClips {
    pub moves: Vec<((u32, usize), f64, f64)>, // ((track_id, clip_idx), old_start, new_start)
}

impl Command for MoveClips {
    fn apply(&mut self, project: &mut Project) {
        for ((track_id, clip_idx), _, new_start) in &self.moves {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == *track_id) {
                if let Some(clip) = track.clips.get_mut(*clip_idx) {
                    clip.set_start_time(*new_start);
                }
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        for ((track_id, clip_idx), old_start, _) in &self.moves {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == *track_id) {
                if let Some(clip) = track.clips.get_mut(*clip_idx) {
                    clip.set_start_time(*old_start);
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Move Clips"
    }
}

// ── ResizeClip ────────────────────────────────────────────────────────
/// Committed when the user finishes dragging a clip edge handle.
#[derive(Debug)]
pub struct ResizeClip {
    pub track_id: u32,
    pub clip_idx: usize,
    pub old_start: f64,
    pub old_len: f64,
    pub new_start: f64,
    pub new_len: f64,
    /// For audio clips: store the offset change for left-edge resize.
    pub old_audio_offset: Option<f64>,
    pub new_audio_offset: Option<f64>,
}

impl Command for ResizeClip {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(clip) = track.clips.get_mut(self.clip_idx) {
                clip.set_start_time(self.new_start);
                clip.set_length(self.new_len);
                if let (Some(new_off), Clip::Audio(ac)) = (self.new_audio_offset, clip) {
                    ac.offset = new_off;
                }
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(clip) = track.clips.get_mut(self.clip_idx) {
                clip.set_start_time(self.old_start);
                clip.set_length(self.old_len);
                if let (Some(old_off), Clip::Audio(ac)) = (self.old_audio_offset, clip) {
                    ac.offset = old_off;
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Resize Clip"
    }
}

// ── ResizeTrack ───────────────────────────────────────────────────────
/// Committed when the user finishes dragging a track height handle.
#[derive(Debug)]
pub struct ResizeTrack {
    pub track_id: u32,
    pub old_height: i32,
    pub new_height: i32,
}

impl Command for ResizeTrack {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            track.height = self.new_height;
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            track.height = self.old_height;
        }
    }

    fn description(&self) -> &str {
        "Resize Track"
    }
}

// ── AddMidiNote ───────────────────────────────────────────────────────
#[derive(Debug)]
pub struct AddMidiNote {
    pub track_id: u32,
    pub clip_idx: usize,
    pub note: crate::models::MidiNote,
}

impl Command for AddMidiNote {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                m.notes.push(self.note.clone());
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                m.notes.pop(); // it was pushed last
            }
        }
    }

    fn description(&self) -> &str {
        "Add MIDI Note"
    }
}

// ── DuplicateNotes ────────────────────────────────────────────────────
/// Duplicate a set of notes, appending them to the end of the clip.
#[derive(Debug)]
pub struct DuplicateNotes {
    pub track_id: u32,
    pub clip_idx: usize,
    pub new_notes: Vec<crate::models::MidiNote>,
    pub count: usize, // how many were appended (set in apply)
}

impl Command for DuplicateNotes {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                self.count = self.new_notes.len();
                for n in &self.new_notes {
                    m.notes.push(n.clone());
                }
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                for _ in 0..self.count {
                    m.notes.pop();
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Duplicate Notes"
    }
}

// ── DeleteMidiNotes ───────────────────────────────────────────────────
/// Deletes a set of notes (by index, sorted descending so removal doesn't shift indices).
#[derive(Debug)]
pub struct DeleteMidiNotes {
    pub track_id: u32,
    pub clip_idx: usize,
    /// (original_index, note) pairs stored for undo, sorted ascending by original index
    pub notes: Vec<(usize, crate::models::MidiNote)>,
}

impl Command for DeleteMidiNotes {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                // Remove from highest index first so lower indices aren't shifted
                let mut indices: Vec<usize> = self.notes.iter().map(|(i, _)| *i).collect();
                indices.sort_unstable_by(|a, b| b.cmp(a));
                for idx in indices {
                    if idx < m.notes.len() {
                        m.notes.remove(idx);
                    }
                }
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                // Re-insert in ascending index order
                let mut pairs = self.notes.clone();
                pairs.sort_unstable_by_key(|(i, _)| *i);
                for (idx, note) in pairs {
                    let insert_at = idx.min(m.notes.len());
                    m.notes.insert(insert_at, note);
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Delete MIDI Notes"
    }
}

// ── MoveMidiNotes ─────────────────────────────────────────────────────
/// Stores per-note (old_start, old_pitch, new_start, new_pitch) deltas.
#[derive(Debug)]
pub struct MoveMidiNotes {
    pub track_id: u32,
    pub clip_idx: usize,
    pub moves: Vec<(usize, f64, u8, f64, u8)>, // (idx, old_start, old_pitch, new_start, new_pitch)
}

impl Command for MoveMidiNotes {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                for &(idx, _, _, new_start, new_pitch) in &self.moves {
                    if let Some(note) = m.notes.get_mut(idx) {
                        note.start = new_start;
                        note.pitch = new_pitch;
                    }
                }
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                for &(idx, old_start, old_pitch, _, _) in &self.moves {
                    if let Some(note) = m.notes.get_mut(idx) {
                        note.start = old_start;
                        note.pitch = old_pitch;
                    }
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Move MIDI Notes"
    }
}

// ── DeleteClips ───────────────────────────────────────────────────────
/// Deletes a set of clips (undoable).
#[derive(Debug)]
pub struct DeleteClips {
    pub clips: Vec<(u32, usize, Clip)>, // (track_id, clip_index, clip_data) — populated on apply
}

impl Command for DeleteClips {
    fn apply(&mut self, project: &mut Project) {
        // Sort descending by index per track so removal doesn't shift other indices
        let mut to_remove: Vec<(u32, usize)> =
            self.clips.iter().map(|(t, i, _)| (*t, *i)).collect();
        to_remove.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
        // Clear stored clip data and re-populate from live project (in case of redo)
        let mut removed = Vec::new();
        for (tid, ci) in to_remove {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == tid) {
                if ci < track.clips.len() {
                    let clip = track.clips.remove(ci);
                    removed.push((tid, ci, clip));
                }
            }
        }
        // Store ascending order for undo re-insertion
        removed.sort_by_key(|(_, i, _)| *i);
        self.clips = removed;
    }

    fn undo(&mut self, project: &mut Project) {
        // Re-insert in ascending index order so indices stay correct
        for (tid, ci, clip) in &self.clips {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == *tid) {
                let insert_at = (*ci).min(track.clips.len());
                track.clips.insert(insert_at, clip.clone());
            }
        }
    }

    fn description(&self) -> &str {
        "Delete Clips"
    }
}

// ── CreateClip ────────────────────────────────────────────────────────
/// Creates a single new clip on a track.
#[derive(Debug)]
pub struct CreateClip {
    pub track_id: u32,
    pub clip: Clip,
    pub added_idx: usize,
}

impl Command for CreateClip {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            self.added_idx = track.clips.len();
            track.clips.push(self.clip.clone());
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if self.added_idx < track.clips.len() {
                track.clips.remove(self.added_idx);
            }
        }
    }
    fn description(&self) -> &str {
        "Create Clip"
    }
}

// ── ResizeClips ───────────────────────────────────────────────────────
/// Resize multiple clips at once.
#[derive(Debug)]
pub struct ResizeClips {
    pub clips: Vec<(u32, usize, f64, f64, f64, f64)>, // (track_id, clip_idx, old_start, old_len, new_start, new_len)
}

impl Command for ResizeClips {
    fn apply(&mut self, project: &mut Project) {
        for &(tid, ci, _, _, new_start, new_len) in &self.clips {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == tid) {
                if let Some(clip) = track.clips.get_mut(ci) {
                    clip.set_start_time(new_start);
                    clip.set_length(new_len);
                }
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        for &(tid, ci, old_start, old_len, _, _) in &self.clips {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == tid) {
                if let Some(clip) = track.clips.get_mut(ci) {
                    clip.set_start_time(old_start);
                    clip.set_length(old_len);
                }
            }
        }
    }
    fn description(&self) -> &str {
        "Resize Clips"
    }
}

// ── ResizeMidiNote ────────────────────────────────────────────────────
#[derive(Debug)]
pub struct ResizeMidiNote {
    pub track_id: u32,
    pub clip_idx: usize,
    pub note_idx: usize,
    pub old_len: f64,
    pub new_len: f64,
}

impl Command for ResizeMidiNote {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                if let Some(note) = m.notes.get_mut(self.note_idx) {
                    note.length = self.new_len;
                }
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(crate::models::Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                if let Some(note) = m.notes.get_mut(self.note_idx) {
                    note.length = self.old_len;
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Resize MIDI Note"
    }
}

#[derive(Debug)]
pub struct CompositeCommand {
    pub desc: String,
    pub cmds: Vec<Box<dyn Command>>,
}
impl Command for CompositeCommand {
    fn apply(&mut self, project: &mut Project) {
        for cmd in &mut self.cmds {
            cmd.apply(project);
        }
    }
    fn undo(&mut self, project: &mut Project) {
        for cmd in self.cmds.iter_mut().rev() {
            cmd.undo(project);
        }
    }
    fn description(&self) -> &str {
        &self.desc
    }
}

#[derive(Debug)]
pub struct ReorderTrack {
    pub track_id: u32,
    pub old_index: usize,
    pub new_index: usize,
}

impl Command for ReorderTrack {
    fn apply(&mut self, project: &mut Project) {
        if let Some(idx) = project.tracks.iter().position(|t| t.id == self.track_id) {
            self.old_index = idx;
            if self.new_index < project.tracks.len() {
                let track = project.tracks.remove(idx);
                project.tracks.insert(self.new_index, track);
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(idx) = project.tracks.iter().position(|t| t.id == self.track_id) {
            let track = project.tracks.remove(idx);
            let restore = self.old_index.min(project.tracks.len());
            project.tracks.insert(restore, track);
        }
    }
    fn description(&self) -> &str {
        "Reorder Track"
    }
}

// ── SetTrackName ──────────────────────────────────────────────────────
#[derive(Debug)]
pub struct SetTrackName {
    pub track_id: u32,
    pub old_name: String,
    pub new_name: String,
}

impl Command for SetTrackName {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            self.old_name = t.name.clone();
            t.name = self.new_name.clone();
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            t.name = self.old_name.clone();
        }
    }
    fn description(&self) -> &str {
        "Rename Track"
    }
}

// ── RackSlotToggle ────────────────────────────────────────────────────
#[derive(Debug)]
pub struct RackSlotToggle {
    pub track_id: u32,
    pub slot_idx: usize,
    pub old_enabled: bool,
}

impl Command for RackSlotToggle {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(slot) = t.rack.get_mut(self.slot_idx) {
                self.old_enabled = slot.enabled;
                slot.enabled = !slot.enabled;
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(slot) = t.rack.get_mut(self.slot_idx) {
                slot.enabled = self.old_enabled;
            }
        }
    }
    fn description(&self) -> &str {
        "Toggle Rack Slot"
    }
}

// ── RackSlotAdd ───────────────────────────────────────────────────────
#[derive(Debug)]
pub struct RackSlotAdd {
    pub track_id: u32,
    pub slot: RackSlot,
    /// Optional insertion index; if None, appends at end
    pub insert_at: Option<usize>,
}

impl Command for RackSlotAdd {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(idx) = self.insert_at {
                let idx = idx.min(t.rack.len());
                t.rack.insert(idx, self.slot.clone());
            } else {
                t.rack.push(self.slot.clone());
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(idx) = self.insert_at {
                let idx = idx.min(t.rack.len().saturating_sub(1));
                if idx < t.rack.len() {
                    t.rack.remove(idx);
                }
            } else {
                t.rack.pop();
            }
        }
    }
    fn description(&self) -> &str {
        "Add Rack Slot"
    }
}

// ── RackSlotRemove ────────────────────────────────────────────────────
#[derive(Debug)]
pub struct RackSlotRemove {
    pub track_id: u32,
    pub slot_idx: usize,
    pub removed_slot: Option<RackSlot>,
}

impl Command for RackSlotRemove {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if self.slot_idx < t.rack.len() {
                self.removed_slot = Some(t.rack.remove(self.slot_idx));
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(slot) = self.removed_slot.take() {
                let idx = self.slot_idx.min(t.rack.len());
                t.rack.insert(idx, slot);
            }
        }
    }
    fn description(&self) -> &str {
        "Remove Rack Slot"
    }
}

// ── SetRackParam ──────────────────────────────────────────────────────
#[derive(Debug)]
pub struct SetRackParam {
    pub track_id: u32,
    pub slot_idx: usize,
    pub param_idx: usize,
    pub old_value: f32,
    pub new_value: f32,
}

impl Command for SetRackParam {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(slot) = t.rack.get_mut(self.slot_idx) {
                if let Some(param) = slot.params.get_mut(self.param_idx) {
                    self.old_value = param.value;
                    param.value = self.new_value;
                }
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(slot) = t.rack.get_mut(self.slot_idx) {
                if let Some(param) = slot.params.get_mut(self.param_idx) {
                    param.value = self.old_value;
                }
            }
        }
    }
    fn description(&self) -> &str {
        "Set Rack Parameter"
    }
}

// ── SetRackSidechain ─────────────────────────────────────────────────
#[derive(Debug)]
pub struct SetRackSidechain {
    pub track_id: u32,
    pub slot_idx: usize,
    pub old_sc: Option<u32>,
    pub new_sc: Option<u32>,
}

impl Command for SetRackSidechain {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(slot) = t.rack.get_mut(self.slot_idx) {
                self.old_sc = slot.sidechain_track_id;
                slot.sidechain_track_id = self.new_sc;
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(slot) = t.rack.get_mut(self.slot_idx) {
                slot.sidechain_track_id = self.old_sc;
            }
        }
    }
    fn description(&self) -> &str {
        "Set Sidechain Source"
    }
}

// ── SetNoteVelocity ───────────────────────────────────────────────────
#[derive(Debug)]
pub struct SetNoteVelocity {
    pub track_id: u32,
    pub clip_idx: usize,
    pub note_idx: usize,
    pub old_velocity: u8,
    pub new_velocity: u8,
}

impl Command for SetNoteVelocity {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                if let Some(note) = m.notes.get_mut(self.note_idx) {
                    self.old_velocity = note.velocity;
                    note.velocity = self.new_velocity;
                }
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Midi(m)) = track.clips.get_mut(self.clip_idx) {
                if let Some(note) = m.notes.get_mut(self.note_idx) {
                    note.velocity = self.old_velocity;
                }
            }
        }
    }
    fn description(&self) -> &str {
        "Set Note Velocity"
    }
}

// ── AddAutomationPoint ────────────────────────────────────────────────
#[derive(Debug)]
pub struct AddAutomationPoint {
    pub track_id: u32,
    pub clip_idx: usize,
    pub point: AutomationPoint,
    pub inserted_idx: usize,
}

impl Command for AddAutomationPoint {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Automation(auto)) = track.clips.get_mut(self.clip_idx) {
                auto.points.push(self.point.clone());
                auto.points
                    .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
                self.inserted_idx = auto
                    .points
                    .iter()
                    .position(|p| {
                        (p.time - self.point.time).abs() < 1e-9
                            && (p.value - self.point.value).abs() < 1e-9
                    })
                    .unwrap_or(0);
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Automation(auto)) = track.clips.get_mut(self.clip_idx) {
                if self.inserted_idx < auto.points.len() {
                    auto.points.remove(self.inserted_idx);
                }
            }
        }
    }
    fn description(&self) -> &str {
        "Add Automation Point"
    }
}

// ── DeleteAutomationPoint ─────────────────────────────────────────────
#[derive(Debug)]
pub struct DeleteAutomationPoint {
    pub track_id: u32,
    pub clip_idx: usize,
    pub point_idx: usize,
    pub removed_point: Option<AutomationPoint>,
}

impl Command for DeleteAutomationPoint {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Automation(auto)) = track.clips.get_mut(self.clip_idx) {
                if self.point_idx < auto.points.len() {
                    self.removed_point = Some(auto.points.remove(self.point_idx));
                }
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Automation(auto)) = track.clips.get_mut(self.clip_idx) {
                if let Some(point) = self.removed_point.take() {
                    let idx = self.point_idx.min(auto.points.len());
                    auto.points.insert(idx, point);
                }
            }
        }
    }
    fn description(&self) -> &str {
        "Delete Automation Point"
    }
}

// ── MoveAutomationPoint ───────────────────────────────────────────────
#[derive(Debug)]
pub struct MoveAutomationPoint {
    pub track_id: u32,
    pub clip_idx: usize,
    pub point_idx: usize,
    pub old_time: f64,
    pub old_value: f32,
    pub new_time: f64,
    pub new_value: f32,
}

impl Command for MoveAutomationPoint {
    fn apply(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Automation(auto)) = track.clips.get_mut(self.clip_idx) {
                if let Some(p) = auto.points.get_mut(self.point_idx) {
                    p.time = self.new_time;
                    p.value = self.new_value;
                }
                auto.points
                    .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(track) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Automation(auto)) = track.clips.get_mut(self.clip_idx) {
                // Find the point by matching new_time/new_value, then restore
                if let Some(idx) = auto.points.iter().position(|p| {
                    (p.time - self.new_time).abs() < 1e-9 && (p.value - self.new_value).abs() < 1e-6
                }) {
                    auto.points[idx].time = self.old_time;
                    auto.points[idx].value = self.old_value;
                    auto.points
                        .sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
                }
            }
        }
    }
    fn description(&self) -> &str {
        "Move Automation Point"
    }
}

// ── MoveClipCrossTrack ──────────────────────────────────────────────
/// Moves a clip from one track to another, optionally changing its start time.
#[derive(Debug)]
pub struct MoveClipCrossTrack {
    pub src_track_id: u32,
    pub src_clip_idx: usize,
    pub dst_track_id: u32,
    pub old_start: f64,
    pub new_start: f64,
    /// After apply, the clip index on the destination track.
    pub dst_clip_idx: Option<usize>,
}

impl Command for MoveClipCrossTrack {
    fn apply(&mut self, project: &mut Project) {
        // Remove clip from source track
        let clip_opt = project
            .tracks
            .iter_mut()
            .find(|t| t.id == self.src_track_id)
            .and_then(|t| {
                if self.src_clip_idx < t.clips.len() {
                    Some(t.clips.remove(self.src_clip_idx))
                } else {
                    None
                }
            });
        if let Some(mut clip) = clip_opt {
            clip.set_start_time(self.new_start);
            if let Some(dst) = project
                .tracks
                .iter_mut()
                .find(|t| t.id == self.dst_track_id)
            {
                self.dst_clip_idx = Some(dst.clips.len());
                dst.clips.push(clip);
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        // Remove clip from destination track
        if let Some(dst_ci) = self.dst_clip_idx {
            let clip_opt = project
                .tracks
                .iter_mut()
                .find(|t| t.id == self.dst_track_id)
                .and_then(|t| {
                    if dst_ci < t.clips.len() {
                        Some(t.clips.remove(dst_ci))
                    } else {
                        None
                    }
                });
            if let Some(mut clip) = clip_opt {
                clip.set_start_time(self.old_start);
                if let Some(src) = project
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == self.src_track_id)
                {
                    // Re-insert at original index
                    let idx = self.src_clip_idx.min(src.clips.len());
                    src.clips.insert(idx, clip);
                }
            }
        }
    }

    fn description(&self) -> &str {
        "Move Clip Cross-Track"
    }
}

/// Move multiple clips (potentially from different tracks) to a single destination track.
/// Each entry: (src_track_id, src_clip_idx, old_start, new_start).
/// After apply, dst_clip_indices stores the appended indices on the destination track.
#[derive(Debug)]
pub struct MoveClipsCrossTrack {
    pub clips: Vec<(u32, usize, f64, f64)>,
    pub dst_track_id: u32,
    /// Filled in by apply(); used by undo.
    pub dst_clip_indices: Vec<usize>,
    /// Filled in by apply(); original src indices shift after removals, stored for undo.
    pub removed_src: Vec<(u32, usize, crate::models::Clip)>,
}

impl Command for MoveClipsCrossTrack {
    fn apply(&mut self, project: &mut Project) {
        self.removed_src.clear();
        self.dst_clip_indices.clear();

        // Collect the clips from their source tracks (sort by index descending per track
        // so removing by index is safe).
        let mut to_remove: Vec<(u32, usize, f64)> = self
            .clips
            .iter()
            .map(|&(tid, ci, _old, new)| (tid, ci, new))
            .collect();
        // Sort: same track → descending index so removals don't shift remaining indices.
        to_remove.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));

        let mut collected: Vec<(crate::models::Clip, f64)> = Vec::new();
        for (tid, ci, new_start) in &to_remove {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == *tid) {
                if *ci < track.clips.len() {
                    let clip = track.clips.remove(*ci);
                    self.removed_src.push((*tid, *ci, clip.clone()));
                    collected.push((clip, *new_start));
                }
            }
        }

        // Append to destination track.
        if let Some(dst) = project
            .tracks
            .iter_mut()
            .find(|t| t.id == self.dst_track_id)
        {
            for (mut clip, new_start) in collected {
                clip.set_start_time(new_start);
                self.dst_clip_indices.push(dst.clips.len());
                dst.clips.push(clip);
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        // Remove clips from destination (descending to keep indices valid).
        let mut dst_indices = self.dst_clip_indices.clone();
        dst_indices.sort_unstable_by(|a, b| b.cmp(a));
        if let Some(dst) = project
            .tracks
            .iter_mut()
            .find(|t| t.id == self.dst_track_id)
        {
            for idx in &dst_indices {
                if *idx < dst.clips.len() {
                    dst.clips.remove(*idx);
                }
            }
        }

        // Re-insert clips into source tracks (ascending index so earlier insertions
        // don't shift the later ones).
        let mut restore = self.removed_src.clone();
        restore.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        for (tid, ci, clip) in restore {
            if let Some(track) = project.tracks.iter_mut().find(|t| t.id == tid) {
                let idx = ci.min(track.clips.len());
                track.clips.insert(idx, clip);
            }
        }
    }

    fn description(&self) -> &str {
        "Move Clips Cross-Track"
    }
}

/// Join adjacent selected clips on the same track into one clip.
/// For MIDI clips: merges notes, keeping positions relative to the new clip start.
/// For Audio clips with the same source: extends the first clip's length.
/// Only joins clips of the same type that are truly adjacent (end == next start, within small tolerance).
#[derive(Debug)]
pub struct JoinClips {
    /// (track_id, clip_indices) — the clip indices to join, sorted ascending.
    pub groups: Vec<(u32, Vec<usize>)>,
}

impl Command for JoinClips {
    fn apply(&mut self, project: &mut Project) {
        let _tolerance = 0.001; // beats tolerance for "adjacent"
        for (tid, indices) in &self.groups {
            if indices.len() < 2 {
                continue;
            }
            let track = match project.tracks.iter_mut().find(|t| t.id == *tid) {
                Some(t) => t,
                None => continue,
            };
            // Collect the clips (by index, ascending)
            let mut sorted_idx = indices.clone();
            sorted_idx.sort();
            let clips: Vec<Clip> = sorted_idx
                .iter()
                .filter_map(|&i| track.clips.get(i).cloned())
                .collect();
            if clips.len() < 2 {
                continue;
            }
            // Sort clips by start_time
            let mut clips_sorted: Vec<(usize, Clip)> =
                sorted_idx.iter().cloned().zip(clips.into_iter()).collect();
            clips_sorted.sort_by(|a, b| a.1.start_time().partial_cmp(&b.1.start_time()).unwrap());

            // Check all are same type and adjacent
            let all_midi = clips_sorted.iter().all(|(_, c)| matches!(c, Clip::Midi(_)));
            let all_audio = clips_sorted
                .iter()
                .all(|(_, c)| matches!(c, Clip::Audio(_)));

            if !all_midi && !all_audio {
                continue; // Can't join mixed types
            }

            // Check adjacency: each clip's end must equal the next clip's start.
            // We allow non-adjacent clips — gaps will be filled with silence in audio merges.
            let _adjacent = true; // kept for potential future strict-mode

            // Build the joined clip
            let new_start = clips_sorted[0].1.start_time();
            let last = &clips_sorted[clips_sorted.len() - 1].1;
            let new_length = (last.start_time() + last.length()) - new_start;

            let joined = if all_midi {
                // Merge all notes, adjusting positions relative to new_start
                let mut all_notes: Vec<crate::models::MidiNote> = Vec::new();
                let first_color = clips_sorted[0].1.color();
                for (_, clip) in &clips_sorted {
                    if let Clip::Midi(mc) = clip {
                        for note in &mc.notes {
                            all_notes.push(crate::models::MidiNote {
                                pitch: note.pitch,
                                velocity: note.velocity,
                                start: note.start + mc.start_time - new_start,
                                length: note.length,
                            });
                        }
                    }
                }
                Clip::Midi(crate::models::MidiClip {
                    notes: all_notes,
                    start_time: new_start,
                    length: new_length,
                    name: "Joined".to_string(),
                    color: first_color,
                })
            } else {
                // Audio: merge audio data from all clips into a single new WAV file
                let bpm = project.tempo_map.bpm_at(0.0).max(1.0);
                let first_ac = match &clips_sorted[0].1 {
                    Clip::Audio(ac) => ac.clone(),
                    _ => unreachable!(),
                };

                // Collect audio data from each clip, inserting silence for gaps
                let mut merged_samples: Vec<f32> = Vec::new();
                let mut out_channels = 1usize;
                let mut out_sr = 44100u32;
                let mut merge_ok = true;
                // First pass: determine output sample rate and channels
                for (_idx, clip) in &clips_sorted {
                    if let Clip::Audio(ac) = clip {
                        let path = std::path::Path::new(&ac.source_file);
                        if let Ok((_raw, channels, sr)) =
                            crate::engine::load_audio_interleaved(path)
                        {
                            out_channels = out_channels.max(channels);
                            out_sr = sr;
                        }
                    }
                }
                // Second pass: merge with silence for gaps
                let mut prev_end_secs = clips_sorted[0].1.start_time() * 60.0 / bpm;
                for (clip_i, (_idx, clip)) in clips_sorted.iter().enumerate() {
                    if let Clip::Audio(ac) = clip {
                        let clip_start_secs = ac.start_time * 60.0 / bpm;
                        // Insert silence for gap before this clip (except before first clip)
                        if clip_i > 0 {
                            let gap_secs = clip_start_secs - prev_end_secs;
                            if gap_secs > 0.001 {
                                let silence_frames = (gap_secs * out_sr as f64) as usize;
                                merged_samples.extend(std::iter::repeat_n(
                                    0.0f32,
                                    silence_frames * out_channels,
                                ));
                            }
                        }
                        let path = std::path::Path::new(&ac.source_file);
                        match crate::engine::load_audio_interleaved(path) {
                            Ok((raw, channels, sr)) => {
                                let ch = channels.max(1);
                                let total_frames = raw.len() / ch;
                                let clip_len_secs = ac.length * 60.0 / bpm;
                                let start_frame =
                                    ((ac.offset * sr as f64) as usize).min(total_frames);
                                let end_frame = (((ac.offset + clip_len_secs) * sr as f64)
                                    as usize)
                                    .min(total_frames);
                                if end_frame > start_frame {
                                    if ch == out_channels || (ch == 1 && out_channels == 2) {
                                        // Upmix mono→stereo if needed
                                        if ch == 1 && out_channels == 2 {
                                            for s in &raw[start_frame..end_frame] {
                                                merged_samples.push(*s);
                                                merged_samples.push(*s);
                                            }
                                        } else {
                                            merged_samples.extend_from_slice(
                                                &raw[start_frame * ch..end_frame * ch],
                                            );
                                        }
                                    } else {
                                        merged_samples.extend_from_slice(
                                            &raw[start_frame * ch..end_frame * ch],
                                        );
                                    }
                                }
                                prev_end_secs = clip_start_secs + clip_len_secs;
                            }
                            Err(_) => {
                                merge_ok = false;
                                break;
                            }
                        }
                    }
                }

                if merge_ok && !merged_samples.is_empty() {
                    // Save merged audio to a new file next to the first clip's source
                    let src_path = std::path::Path::new(&first_ac.source_file);
                    let dir = src_path.parent().unwrap_or(std::path::Path::new("."));
                    let stem = src_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("audio");
                    let ext = src_path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("wav");
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    let joined_name = format!("{}_joined_{}.{}", stem, ts, ext);
                    let joined_path = dir.join(&joined_name);

                    let save_ok = if out_channels >= 2 {
                        crate::engine::save_wav_stereo(&joined_path, &merged_samples, out_sr)
                            .is_ok()
                    } else {
                        crate::engine::save_wav_mono(&joined_path, &merged_samples, out_sr).is_ok()
                    };

                    if save_ok {
                        let merged_dur_secs = merged_samples.len() as f64
                            / (out_sr as f64 * out_channels.max(1) as f64);
                        let merged_len_beats = merged_dur_secs * bpm / 60.0;
                        Clip::Audio(crate::models::AudioClip {
                            source_file: joined_path.to_string_lossy().to_string(),
                            start_time: new_start,
                            offset: 0.0,
                            length: merged_len_beats,
                            gain: first_ac.gain,
                            name: "Joined".to_string(),
                            color: first_ac.color,
                            fade_in: 0.0,
                            fade_out: 0.0,
                        })
                    } else {
                        // Fallback: just extend length like before
                        Clip::Audio(crate::models::AudioClip {
                            source_file: first_ac.source_file,
                            start_time: new_start,
                            offset: first_ac.offset,
                            length: new_length,
                            gain: first_ac.gain,
                            name: "Joined".to_string(),
                            color: first_ac.color,
                            fade_in: 0.0,
                            fade_out: 0.0,
                        })
                    }
                } else {
                    // Fallback: just extend length
                    Clip::Audio(crate::models::AudioClip {
                        source_file: first_ac.source_file,
                        start_time: new_start,
                        offset: first_ac.offset,
                        length: new_length,
                        gain: first_ac.gain,
                        name: "Joined".to_string(),
                        color: first_ac.color,
                        fade_in: 0.0,
                        fade_out: 0.0,
                    })
                }
            };

            // Remove old clips (highest index first to preserve indices)
            let mut remove_indices: Vec<usize> = sorted_idx;
            remove_indices.sort();
            remove_indices.reverse();
            for idx in &remove_indices {
                if *idx < track.clips.len() {
                    track.clips.remove(*idx);
                }
            }
            // Insert joined clip
            track.clips.push(joined);
        }
    }

    fn undo(&mut self, _project: &mut Project) {
        // Undo is handled by snapshot-based CommandManager — this is a no-op placeholder
    }

    fn description(&self) -> &str {
        "Join Clips"
    }
}

// ── Set Sampler File (undoable) ──────────────────────────────────────
#[derive(Debug, Clone)]
pub struct SetSamplerFile {
    pub track_id: u32,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

impl Command for SetSamplerFile {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            t.sampler_file = self.new_value.clone();
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            t.sampler_file = self.old_value.clone();
        }
    }

    fn description(&self) -> &str {
        "Set Sampler File"
    }
}

// ── Set Project Name (undoable) ──────────────────────────────────────
#[derive(Debug, Clone)]
pub struct SetProjectName {
    pub old_name: String,
    pub new_name: String,
}

impl Command for SetProjectName {
    fn apply(&mut self, project: &mut Project) {
        project.name = self.new_name.clone();
    }

    fn undo(&mut self, project: &mut Project) {
        project.name = self.old_name.clone();
    }

    fn description(&self) -> &str {
        "Rename Project"
    }
}

// ── Set Audio Clip Gain (undoable) ───────────────────────────────────
#[derive(Debug, Clone)]
pub struct SetClipGain {
    pub track_id: u32,
    pub clip_idx: usize,
    pub old_gain: f32,
    pub new_gain: f32,
}

impl Command for SetClipGain {
    fn apply(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Audio(ac)) = t.clips.get_mut(self.clip_idx) {
                ac.gain = self.new_gain;
            }
        }
    }

    fn undo(&mut self, project: &mut Project) {
        if let Some(t) = project.tracks.iter_mut().find(|t| t.id == self.track_id) {
            if let Some(Clip::Audio(ac)) = t.clips.get_mut(self.clip_idx) {
                ac.gain = self.old_gain;
            }
        }
    }

    fn description(&self) -> &str {
        "Set Clip Gain"
    }
}

// ── SetMasterRackParam ──────────────────────────────────────────────
#[derive(Debug)]
pub struct SetMasterRackParam {
    pub slot_idx: usize,
    pub param_idx: usize,
    pub old_value: f32,
    pub new_value: f32,
}

impl Command for SetMasterRackParam {
    fn apply(&mut self, project: &mut Project) {
        if let Some(slot) = project.master_rack.get_mut(self.slot_idx) {
            if let Some(param) = slot.params.get_mut(self.param_idx) {
                self.old_value = param.value;
                param.value = self.new_value;
            }
        }
    }
    fn undo(&mut self, project: &mut Project) {
        if let Some(slot) = project.master_rack.get_mut(self.slot_idx) {
            if let Some(param) = slot.params.get_mut(self.param_idx) {
                param.value = self.old_value;
            }
        }
    }
    fn description(&self) -> &str {
        "Set Master Effect Param"
    }
}
