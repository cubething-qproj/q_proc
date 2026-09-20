//! Process management primitives for Bevy.
//!
//! Provides [`Process`], [`ProgramLabel`], signals, and process I/O messages.
//! Endpoint adapters and shell/terminal integration are left to consumer crates.

mod data;
mod plugins;
pub mod systems;

pub mod prelude {
    pub use super::data::prelude::*;
    pub use super::plugins::*;
    pub use bevy::prelude::*;
    pub use tiny_bail::prelude::*;
}
