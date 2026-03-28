// Eden DAW — Instrument Modules
//
// Each instrument lives in its own file for easy navigation and extension.
// This mod.rs re-exports everything so `use crate::modules::instruments::*` works.

mod analog;
mod heavy;
mod hypersaw;
mod sampler;

pub use analog::*;
pub use heavy::*;
pub use hypersaw::*;
pub use sampler::*;
