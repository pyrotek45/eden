// Eden DAW — Effect Modules
//
// Each effect lives in its own file for easy navigation and extension.
// This mod.rs re-exports everything so `use crate::modules::effects::*` works.

mod autoduck;
mod chorus;
mod compressor;
mod cstrip2;
mod delay;
mod distortion;
mod eq;
mod gain;
mod hp_filter;
mod limiter;
mod lp_filter;
mod reverb;
mod utility;

pub use autoduck::*;
pub use chorus::*;
pub use compressor::*;
pub use cstrip2::*;
pub use delay::*;
pub use distortion::*;
pub use eq::*;
pub use gain::*;
pub use hp_filter::*;
pub use limiter::*;
pub use lp_filter::*;
pub use reverb::*;
pub use utility::*;
