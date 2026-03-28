// Eden DAW — Application core
//
// Contains the core application types: data models, application state,
// command/undo system, input handling, and user configuration.

pub mod commands;
pub mod config;
pub mod input;
pub mod models;
pub mod state;

// Re-export everything so `use crate::app::*` works as a drop-in
// replacement for the old flat imports.
#[allow(unused_imports)]
pub use commands::*;
#[allow(unused_imports)]
pub use config::*;
#[allow(unused_imports)]
pub use input::*;
#[allow(unused_imports)]
pub use models::*;
#[allow(unused_imports)]
pub use state::*;
