// Eden DAW — View-level Input Layer Manager

use crate::input::InputState;

pub struct ViewLayers<'a> {
    real: &'a mut InputState,
    dead: InputState,
}

impl<'a> ViewLayers<'a> {
    pub fn new(input: &'a mut InputState) -> Self {
        let dead = InputState {
            mouse_x: input.mouse_x,
            mouse_y: input.mouse_y,
            ..Default::default()
        };
        ViewLayers { real: input, dead }
    }

    /// Return the real InputState — use for the top-most (highest-z) region.
    pub fn top(&mut self) -> &mut InputState {
        self.real
    }

    /// Return the real InputState only if input has not yet been consumed;
    /// otherwise return a dead InputState (mouse position preserved, no events).
    pub fn below(&mut self) -> &mut InputState {
        if self.real.consumed {
            self.dead.mouse_x = self.real.mouse_x;
            self.dead.mouse_y = self.real.mouse_y;
            &mut self.dead
        } else {
            self.real
        }
    }
}
