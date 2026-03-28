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
pub use commands::*;
pub use config::*;
pub use input::*;
pub use models::*;
pub use state::*;
