//! Process management primitives for Bevy.
//!
//! Provides [`Process`], [`ProgramLabel`], signals, process I/O messages, and
//! minimal pipe and tee adapters. Full pipe flow control and shell/terminal or
//! asset-state adapters remain external or deferred.

extern crate self as q_proc;

mod data;
mod plugins;
pub mod systems;

pub use q_proc_derive::ProgramLabel;

pub mod prelude {
    pub use super::ProgramLabel;
    pub use super::data::prelude::*;
    pub use super::plugins::*;
    pub use bevy::prelude::*;
    pub use tiny_bail::prelude::*;
}
