// Eden DAW — Shared DSP processing pipeline
//
// ╔══════════════════════════════════════════════════════════════════╗
// ║  SINGLE SOURCE OF TRUTH                                         ║
// ║  This module contains the ONLY copy of the audio processing     ║
// ║  pipeline. Both the realtime engine (audio.rs) and the offline  ║
// ║  renderer (render.rs) call these same functions. There must be  ║
// ║  ZERO duplication of DSP logic anywhere else in the codebase.   ║
// ╚══════════════════════════════════════════════════════════════════╝

mod automation;
mod core_helpers;
mod mixing;
mod named_chain;
mod track_setup;
mod voice;

pub use automation::*;
pub use core_helpers::*;
pub use mixing::*;
pub use named_chain::*;
pub use track_setup::*;
pub use voice::*;
