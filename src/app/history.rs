//! Toolbar and keyboard history use one project revision stack in every editor.
use super::state::AppState;
impl AppState {
    pub fn history_description(&self, redo: bool) -> Option<String> {
        if redo {
            self.commands.redo_description().map(str::to_owned)
        } else {
            self.commands.undo_description().map(str::to_owned)
        }
    }
    pub fn apply_history(&mut self, redo: bool) {
        self.cancel_clip_drag();
        self.project.reconcile_clip_content();
        let action = if redo { "Redo" } else { "Undo" };
        let Some(description) = self.history_description(redo) else {
            self.push_status(format!("Nothing to {}", action.to_lowercase()));
            return;
        };
        if redo {
            self.commands.redo(&mut self.project);
        } else {
            self.commands.undo(&mut self.project);
        }
        self.sync_clip_library();
        self.sync_editor_selection();
        self.dirty = true;
        self.push_status(format!("{}: {}", action, description));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_editor_and_arranger_share_history() {
        let mut state = AppState::new();
        let before = state.project.clone();
        state.project.name = "Changed".into();
        state.commands.push_undo_snapshot(before.clone(), "Rename");
        state.focused_panel = crate::app::state::FocusedPanel::AudioEditor;
        state.apply_history(false);
        assert_eq!(state.project.name, before.name);
        state.apply_history(true);
        assert_eq!(state.project.name, "Changed");
    }
}
