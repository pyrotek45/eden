// Eden DAW — Input system
// Tracks mouse state, keyboard modifiers, active widget, drag state.

use sdl2::keyboard::Mod;

/// Unique identifier for a widget so we know what's being dragged.
/// Use `input.next_id()` to get a unique, deterministic ID each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidgetId {
    None,
    /// Auto-generated unique ID from the per-frame counter.
    Auto(u32),
    TrackHeader(u32),
    TrackResize(u32),           // track id — drag bottom edge to resize track height
    ClipBody(u32, usize),       // track id, clip index — drag to move
    ClipLeftHandle(u32, usize), // track id, clip index — drag to resize left
    ClipRightHandle(u32, usize), // track id, clip index — drag to resize right
    LoopStart,
    LoopEnd,
    LoopBar,
    Playhead,
    TimelineScroll,
    Rubberband,
    LeftPanelScrollbar,
    LeftPanelResize,
    ClipManagerScrollbar,
}

/// What kind of click was it?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickType {
    Single,
    Double,
    RightClick,
}

#[derive(Debug, Clone)]
pub struct InputState {
    // Raw physical mouse coords (as reported by SDL2, unscaled)
    pub raw_mouse_x: i32,
    pub raw_mouse_y: i32,
    // Logical mouse coords (divided by ui_scale) — use these in all widgets
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_prev_x: i32,
    pub mouse_prev_y: i32,
    pub mouse_dx: i32,
    pub mouse_dy: i32,
    pub mouse_down: bool,
    pub mouse_pressed: bool,
    pub mouse_released: bool,
    pub right_mouse_down: bool,
    pub right_mouse_pressed: bool,
    pub right_mouse_released: bool,
    pub middle_mouse_down: bool,
    pub middle_mouse_pressed: bool,
    pub middle_mouse_released: bool,
    pub scroll_x: i32,
    pub scroll_y: i32,

    // Click detection
    pub click_type: Option<ClickType>,
    pub last_click_time: u64,
    pub last_click_pos: (i32, i32),
    pub double_click_threshold: u64,

    // Drag state
    pub dragging: bool,
    pub drag_start_x: i32,
    pub drag_start_y: i32,
    pub drag_widget: WidgetId,
    pub drag_start_value: f64,
    /// Secondary drag-start value (e.g. original clip start when resizing left edge,
    /// original track height when resizing a track).
    pub drag_start_value2: f64,

    // Active / Hot widget
    pub active_widget: WidgetId,
    pub hot_widget: WidgetId,

    /// Widget captured by a middle-click drag (for fine-tune knobs/sliders).
    /// Stays set while middle mouse is held so the widget keeps receiving
    /// drag deltas even when the cursor moves off the widget area.
    pub middle_drag_widget: WidgetId,

    /// Set to `true` once any widget claims this frame's mouse press.
    /// Lower-layer widgets must check `!input.consumed` before processing
    /// a click so that higher-layer UI elements (rulers, transport, etc.)
    /// can intercept presses without them leaking to clips underneath.
    pub consumed: bool,

    /// Set to `true` once any widget claims this frame's scroll event.
    /// Lower-layer widgets/panels must check `!input.scroll_consumed` before
    /// processing scroll so that higher-layer widgets (knobs, dropdowns, etc.)
    /// don't leak scroll to underlying scrollable areas.
    pub scroll_consumed: bool,

    // Keyboard
    pub key_mod: Mod,
    pub keys_pressed: Vec<sdl2::keyboard::Keycode>,
    pub keys_held: Vec<sdl2::keyboard::Keycode>,

    /// Set of keys that have been claimed by a handler this frame.
    /// Other handlers must call `key_available()` before acting on a key
    /// and `consume_key()` after handling it, preventing cross-bleed
    /// between panels (e.g. piano roll vs arrangement).
    pub keys_consumed: std::collections::HashSet<sdl2::keyboard::Keycode>,

    // Text input (from SDL2 TextInput events)
    pub text_input_chars: Vec<char>,

    // Hover hint / tooltip (set by widgets each frame)
    pub hover_hint_text: Option<String>,
    pub hover_hint_widget: WidgetId,

    // File drag-and-drop
    pub dropped_file: Option<String>,

    /// Per-frame widget ID counter. Resets to 0 in begin_frame().
    /// Use next_id() to get a unique WidgetId::Auto(N) each call.
    pub widget_counter: u32,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            raw_mouse_x: 0,
            raw_mouse_y: 0,
            mouse_x: 0,
            mouse_y: 0,
            mouse_prev_x: 0,
            mouse_prev_y: 0,
            mouse_dx: 0,
            mouse_dy: 0,
            mouse_down: false,
            mouse_pressed: false,
            mouse_released: false,
            right_mouse_down: false,
            right_mouse_pressed: false,
            right_mouse_released: false,
            middle_mouse_down: false,
            middle_mouse_pressed: false,
            middle_mouse_released: false,
            scroll_x: 0,
            scroll_y: 0,
            click_type: None,
            last_click_time: 0,
            last_click_pos: (0, 0),
            double_click_threshold: 300,
            dragging: false,
            drag_start_x: 0,
            drag_start_y: 0,
            drag_widget: WidgetId::None,
            drag_start_value: 0.0,
            drag_start_value2: 0.0,
            active_widget: WidgetId::None,
            hot_widget: WidgetId::None,
            middle_drag_widget: WidgetId::None,
            consumed: false,
            scroll_consumed: false,
            key_mod: Mod::empty(),
            keys_pressed: Vec::new(),
            keys_held: Vec::new(),
            keys_consumed: std::collections::HashSet::new(),
            text_input_chars: Vec::new(),
            hover_hint_text: None,
            hover_hint_widget: WidgetId::None,
            dropped_file: None,
            widget_counter: 0,
        }
    }
}

impl InputState {
    /// Called at the start of each frame to reset per-frame event flags.
    /// Does NOT touch mouse coordinates — apply_scale() handles those.
    pub fn begin_frame(&mut self) {
        self.mouse_pressed = false;
        self.mouse_released = false;
        self.right_mouse_pressed = false;
        self.right_mouse_released = false;
        self.middle_mouse_pressed = false;
        self.middle_mouse_released = false;
        self.scroll_x = 0;
        self.scroll_y = 0;
        self.click_type = None;
        self.keys_pressed.clear();
        self.keys_consumed.clear();
        self.text_input_chars.clear();
        self.hot_widget = WidgetId::None;
        self.hover_hint_text = None;
        self.dropped_file = None;
        self.consumed = false;
        self.scroll_consumed = false;
        self.widget_counter = 0;
        // Clear active_widget at frame start when mouse is not held down.
        // This means active_widget survives through the released frame (so buttons can
        // detect click = pressed+released on same widget), but is cleared next frame.
        if !self.mouse_down {
            self.active_widget = WidgetId::None;
        }
        // Clear middle drag widget when middle mouse is released
        if !self.middle_mouse_down {
            self.middle_drag_widget = WidgetId::None;
        }
    }

    /// Called at end of frame — no-op now; kept for API compatibility.
    pub fn end_frame(&mut self) {}

    /// Get a unique, deterministic widget ID for this frame.
    /// The counter resets each frame in begin_frame(), so widgets drawn in
    /// the same order each frame get the same ID.
    pub fn next_id(&mut self) -> WidgetId {
        let id = self.widget_counter;
        self.widget_counter += 1;
        WidgetId::Auto(id)
    }

    /// Mark this frame's mouse press as consumed so lower-layer widgets skip it.
    pub fn consume(&mut self) {
        self.consumed = true;
    }

    /// Mark a keyboard key as consumed this frame. Other handlers should call
    /// `key_available()` before acting on a key to prevent cross-bleed between
    /// panels (e.g. Shift+Up in piano roll vs arrangement).
    pub fn consume_key(&mut self, key: sdl2::keyboard::Keycode) {
        self.keys_consumed.insert(key);
    }

    /// Returns `true` if the key was pressed this frame AND has not been consumed
    /// by another handler. Always use this instead of raw `keys_pressed.contains()`
    /// for shortcut handlers that could overlap between panels.
    pub fn key_available(&self, key: sdl2::keyboard::Keycode) -> bool {
        self.keys_pressed.contains(&key) && !self.keys_consumed.contains(&key)
    }

    pub fn on_scroll(&mut self, x: i32, y: i32) {
        self.scroll_x = x;
        self.scroll_y = y;
    }

    pub fn on_mouse_down(&mut self, x: i32, y: i32, btn: sdl2::mouse::MouseButton, ticks: u64) {
        self.raw_mouse_x = x;
        self.raw_mouse_y = y;
        // mouse_x/y (logical) will be set by apply_scale() in the render loop
        match btn {
            sdl2::mouse::MouseButton::Left => {
                self.mouse_down = true;
                self.mouse_pressed = true;
                // drag_start_x/y set in apply_scale after logical coords are ready
                let dt = ticks.saturating_sub(self.last_click_time);
                let dist =
                    ((x - self.last_click_pos.0).abs() + (y - self.last_click_pos.1).abs()) as u64;
                if dt < self.double_click_threshold && dist < 20 {
                    self.click_type = Some(ClickType::Double);
                } else {
                    self.click_type = Some(ClickType::Single);
                }
                self.last_click_time = ticks;
                self.last_click_pos = (x, y);
            }
            sdl2::mouse::MouseButton::Right => {
                self.right_mouse_down = true;
                self.right_mouse_pressed = true;
                self.click_type = Some(ClickType::RightClick);
            }
            sdl2::mouse::MouseButton::Middle => {
                self.middle_mouse_down = true;
                self.middle_mouse_pressed = true;
            }
            _ => {}
        }
    }

    pub fn on_mouse_up(&mut self, x: i32, y: i32, btn: sdl2::mouse::MouseButton) {
        self.raw_mouse_x = x;
        self.raw_mouse_y = y;
        match btn {
            sdl2::mouse::MouseButton::Left => {
                self.mouse_down = false;
                self.mouse_released = true;
            }
            sdl2::mouse::MouseButton::Right => {
                self.right_mouse_down = false;
                self.right_mouse_released = true;
            }
            sdl2::mouse::MouseButton::Middle => {
                self.middle_mouse_down = false;
                self.middle_mouse_released = true;
            }
            _ => {}
        }
    }

    pub fn on_mouse_move(&mut self, x: i32, y: i32) {
        self.raw_mouse_x = x;
        self.raw_mouse_y = y;
    }

    /// Call once per frame, before drawing, to convert raw physical coords → logical.
    /// This is the ONLY place division by scale happens.
    pub fn apply_scale(&mut self, scale: f32) {
        let lx = (self.raw_mouse_x as f32 / scale) as i32;
        let ly = (self.raw_mouse_y as f32 / scale) as i32;

        self.mouse_prev_x = self.mouse_x;
        self.mouse_prev_y = self.mouse_y;
        self.mouse_x = lx;
        self.mouse_y = ly;
        self.mouse_dx = lx - self.mouse_prev_x;
        self.mouse_dy = ly - self.mouse_prev_y;

        // Set drag start in logical coords the moment the press happens
        if self.mouse_pressed {
            self.drag_start_x = lx;
            self.drag_start_y = ly;
        }

        if self.mouse_down && !self.dragging {
            let dx = (lx - self.drag_start_x).abs();
            let dy = (ly - self.drag_start_y).abs();
            if dx > 3 || dy > 3 {
                self.dragging = true;
            }
        }

        if self.mouse_released {
            self.dragging = false;
            self.drag_widget = WidgetId::None;
            // NOTE: active_widget is intentionally NOT cleared here.
            // It must survive into the drawing phase of the released frame so
            // that widgets can detect click completion (pressed + released on same widget).
            // active_widget is cleared at the START of the next frame in begin_frame().
        }
    }

    pub fn on_key_down(&mut self, key: sdl2::keyboard::Keycode) {
        self.keys_pressed.push(key);
        if !self.keys_held.contains(&key) {
            self.keys_held.push(key);
        }
    }

    pub fn on_key_up(&mut self, key: sdl2::keyboard::Keycode) {
        self.keys_held.retain(|k| *k != key);
    }

    pub fn key_held(&self, key: sdl2::keyboard::Keycode) -> bool {
        self.keys_held.contains(&key)
    }

    pub fn ctrl(&self) -> bool {
        self.key_mod.contains(Mod::LCTRLMOD) || self.key_mod.contains(Mod::RCTRLMOD)
    }

    pub fn shift(&self) -> bool {
        self.key_mod.contains(Mod::LSHIFTMOD) || self.key_mod.contains(Mod::RSHIFTMOD)
    }

    pub fn alt(&self) -> bool {
        self.key_mod.contains(Mod::LALTMOD) || self.key_mod.contains(Mod::RALTMOD)
    }

    pub fn mouse_in_rect(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        self.mouse_x >= x && self.mouse_x < x + w && self.mouse_y >= y && self.mouse_y < y + h
    }
}
