//! Process management primitives for Bevy.
//!
//! Provides [`Process`], [`ProgramLabel`], signals, and process I/O messages.
//! Bridging stdio to a terminal display is left to consumer crates
//! (e.g. `q_term`).

mod data;
mod plugins;
pub mod systems;

pub mod prelude {
    pub use super::data::prelude::*;
    pub use super::plugins::*;
    pub use bevy::prelude::*;
    pub use tiny_bail::prelude::*;
}
