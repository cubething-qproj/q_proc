//! Process stdio messages.
//!
//! `StdOut` / `StdErr` are process-level concerns. Bridging them to a
//! terminal display lives outside this crate (e.g. in examples that
//! depend on `q_term`).

use crate::prelude::*;

/// Flows from a [`Job`] or [`Shell`] into a [`Terminal`]'s
/// ANSI parser. Parameterised by output channel (`1` = stdout,
/// `2` = stderr, matching POSIX fd numbers).
///
/// Use the [`StdOut`] and [`StdErr`] aliases at call sites. The generic
/// exists so a single impl serves both channels while keeping them as
/// distinct Bevy message types (separate [`Message`] resources,
/// separate [`MessageReader`]s).
#[derive(Message, Debug, Clone, Reflect)]
pub struct ProgOutputChannel<const CHANNEL: u8> {
    /// Target terminal entity.
    pub term: Entity,
    /// Spans to write into the buffer. ANSI compatible.
    pub writes: Vec<String>,
}

/// Stdout writes. (POSIX fd 1).
pub type StdOut = ProgOutputChannel<1>;
/// Stderr writes. (POSIX fd 2).
pub type StdErr = ProgOutputChannel<2>;

impl<const CHANNEL: u8> ProgOutputChannel<CHANNEL> {
    /// Construct with arbitrary write spans.
    pub fn new(term: Entity, writes: Vec<String>) -> Self {
        Self { term, writes }
    }
    /// Writes text directly to the buffer. Supports ANSI.
    pub fn write(term: Entity, value: impl ToString) -> Self {
        let line = value.to_string();
        Self {
            term,
            writes: vec![line],
        }
    }
}
