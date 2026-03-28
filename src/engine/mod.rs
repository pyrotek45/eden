// Eden DAW — Engine module
//
// Contains the realtime audio engine (audio.rs) and the offline renderer
// (render.rs). Both use the shared DSP pipeline from dsp/ to guarantee
// identical sound output.

pub mod audio;
pub mod render;

// Re-export public items so callers can use `engine::AudioShared`, etc.
pub use audio::*;
pub use render::{render_to_wav, render_to_wav_with_progress, RenderSettings};

// render_to_buffer is only used by tests
#[cfg(test)]
pub use render::render_to_buffer;
