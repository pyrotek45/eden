//! One history action for toolbar buttons and keyboard shortcuts.
use super::state::{AppState, FocusedPanel};

impl AppState {
    pub fn history_description(&self, redo: bool) -> Option<String> {
        if self.focused_panel == FocusedPanel::AudioEditor {
            let stack = if redo { &self.audio_redo_stack } else { &self.audio_undo_stack };
            stack.last().map(|entry| entry.2.clone())
        } else if redo {
            self.commands.redo_description().map(str::to_owned)
        } else {
            self.commands.undo_description().map(str::to_owned)
        }
    }

    pub fn apply_history(&mut self, redo: bool) {
        let action = if redo { "Redo" } else { "Undo" };
        let Some(description) = self.history_description(redo) else {
            self.push_status(format!("Nothing to {}", action.to_lowercase()));
            return;
        };
        if self.focused_panel == FocusedPanel::AudioEditor {
            let stack = if redo { &self.audio_redo_stack } else { &self.audio_undo_stack };
            let (src, backup, _, project) = stack.last().unwrap().clone();
            let path = std::path::Path::new(&src);
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos()).unwrap_or(0);
            let reverse = path.with_file_name(format!(".eden_history_{}_{}", std::process::id(), stamp));
            // Never consume a recovery entry unless both copies succeeded.
            // Missing backups must not cause a source-file write.
            let result = std::fs::metadata(&backup)
                .and_then(|_| std::fs::copy(&src, &reverse))
                .and_then(|_| std::fs::copy(&backup, &src));
            if let Err(error) = result {
                self.push_status(format!("{} failed: {} (history retained)", action, error));
                return;
            }
            let current_project = project.as_ref().map(|_| self.project.clone());
            let opposite = (src.clone(), reverse.to_string_lossy().into_owned(), description.clone(), current_project);
            if redo {
                self.audio_redo_stack.pop();
                self.audio_undo_stack.push(opposite);
            } else {
                self.audio_undo_stack.pop();
                self.audio_redo_stack.push(opposite);
            }
            let _ = std::fs::remove_file(&backup);
            self.waveform_cache.remove(&src);
            self.waveform_stereo_cache.remove(&src);
            self.waveform_raw_cache.remove(&src);
            self.audio_sample_invalidate.push(src);
            if let Some(project) = project { self.project = project; }
        } else if redo {
            self.commands.redo(&mut self.project);
        } else {
            self.commands.undo(&mut self.project);
        }
        self.dirty = true;
        self.push_status(format!("{}: {}", action, description));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_audio_history_keeps_recovery_entry() {
        let mut state = AppState::new();
        state.focused_panel = FocusedPanel::AudioEditor;
        state.audio_undo_stack.push(("/no/eden/source".into(), "/no/eden/backup".into(), "Trim".into(), None));
        state.apply_history(false);
        assert_eq!(state.audio_undo_stack.len(), 1);
        assert!(state.audio_redo_stack.is_empty());
        assert!(!state.dirty);
    }
    #[test]
    fn history_uses_focused_editor_and_marks_project_dirty() {
        let mut state = AppState::new();
        let snapshot = state.project.clone();
        state.project.name = "Changed".into();
        state.commands.push_undo_snapshot(snapshot.clone(), "Rename");
        state.focused_panel = FocusedPanel::AudioEditor;
        assert!(state.history_description(false).is_none());
        state.focused_panel = FocusedPanel::Arrangement;
        state.apply_history(false);
        assert_eq!(state.project.name, snapshot.name);
        assert!(state.dirty);
        state.apply_history(true);
        assert_eq!(state.project.name, "Changed");
    }
}
