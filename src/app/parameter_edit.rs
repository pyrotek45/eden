//! Parameter drags own one before-image, including the prior dirty state.
use super::{input::InputState, models::Project, state::AppState};
pub(crate) struct ParameterGesture {
    pub before: Project,
    pub was_dirty: bool,
    pub description: &'static str,
}
impl AppState {
    pub(crate) fn set_device_parameter(
        &mut self,
        track: Option<usize>,
        slot: usize,
        parameter: usize,
        value: f32,
    ) {
        let rack = if let Some(ti) = track {
            &self.project.tracks[ti].rack
        } else {
            &self.project.master_rack
        };
        let slot_ref = &rack[slot];
        let id = &slot_ref.params[parameter].id;
        let discrete = crate::modules::get_param_descs(&slot_ref.plugin_name)
            .iter()
            .any(|d| d.id == id && d.options.is_some())
            || (crate::modules::is_midi_effect(&slot_ref.plugin_name)
                && matches!(
                    id.as_str(),
                    "octave" | "octaves" | "semitones" | "min_vel" | "max_vel"
                ));
        let value = if discrete { value.round() } else { value };
        let current = if let Some(ti) = track {
            self.project.tracks[ti].rack[slot].params[parameter].value
        } else {
            self.project.master_rack[slot].params[parameter].value
        };
        if current == value {
            return;
        }
        if self.project_parameter_snapshot.is_none() {
            self.project_parameter_snapshot = Some(ParameterGesture {
                before: self.project.clone(),
                was_dirty: self.dirty,
                description: "Adjust Device Parameter",
            });
        }
        if let Some(ti) = track {
            self.project.tracks[ti].rack[slot].params[parameter].value = value;
        } else {
            self.project.master_rack[slot].params[parameter].value = value;
        }
        self.dirty = true;
    }

    pub(crate) fn begin_project_parameter_gesture(&mut self, description: &'static str) {
        if self.project_parameter_snapshot.is_none() {
            self.project_parameter_snapshot = Some(ParameterGesture {
                before: self.project.clone(),
                was_dirty: self.dirty,
                description,
            });
        }
        self.dirty = true;
    }

    pub(crate) fn begin_audio_parameter_gesture(&mut self) {
        if self.audio_parameter_snapshot.is_none() {
            self.audio_parameter_snapshot = Some(ParameterGesture {
                before: self.project.clone(),
                was_dirty: self.dirty,
                description: "Adjust Audio Clip",
            });
        }
    }
    pub(crate) fn cancel_parameter_gestures(&mut self) {
        for gesture in [
            self.project_parameter_snapshot.take(),
            self.audio_parameter_snapshot.take(),
        ]
        .into_iter()
        .flatten()
        {
            self.project = gesture.before;
            self.dirty = gesture.was_dirty;
        }
    }
    pub(crate) fn finish_parameter_gestures(&mut self, input: &InputState) {
        if input
            .keys_pressed
            .contains(&sdl2::keyboard::Keycode::Escape)
        {
            self.cancel_parameter_gestures();
            return;
        }
        if input.mouse_released || (!input.mouse_down && !input.middle_mouse_down) {
            for gesture in [
                self.project_parameter_snapshot.take(),
                self.audio_parameter_snapshot.take(),
            ] {
                if let Some(gesture) = gesture {
                    self.project.reconcile_clip_content();
                    self.commands
                        .push_undo_snapshot(gesture.before, gesture.description);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escape_restores_parameter_without_dirtying_or_adding_history() {
        let mut state = AppState::new();
        state.project = Project::demo();
        state.dirty = false;
        let value = state.project.tracks[0].rack[0].params[0].value;
        state.set_device_parameter(Some(0), 0, 0, value + 1.0);
        let mut input = InputState::default();
        input.keys_pressed.push(sdl2::keyboard::Keycode::Escape);
        state.finish_parameter_gestures(&input);
        assert_eq!(state.project.tracks[0].rack[0].params[0].value, value);
        assert!(!state.dirty);
        assert!(!state.commands.can_undo());
    }
}
