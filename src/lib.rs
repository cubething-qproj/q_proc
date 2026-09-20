//! Process management primitives for Bevy.
//!
//! Provides [`Process`], [`ProgramLabel`], signals, process I/O messages, and
//! minimal pipe and tee adapters. Full pipe flow control and shell/terminal or
//! asset-state adapters remain external or deferred.

mod data;
mod plugins;
pub mod systems;

pub mod prelude {
    pub use super::data::prelude::*;
    pub use super::plugins::*;
    pub use bevy::prelude::*;
    pub use tiny_bail::prelude::*;
}
