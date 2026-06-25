//! Process signals.

use crate::prelude::*;

/// Process signal messages. Interpreted by [`Job`] entities.
#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Sig {
    /// Produced by: (^C), kill builtin.
    /// Polite request to stop. Can be caught.
    Int,
    /// Produced by: (^\), kill builtin.
    /// Polite request to stop. Can be caught. Produces a core dump (in our case, stack trace).
    Quit,
    /// Produced by: (^Z), kill builtin.
    /// Signifies that a job is being placed in the background.
    Tstp,
    /// Produced by: kill builtin.
    /// Polite request from another program to stop. Can be caught.
    Term,
    /// Produced by: kill builtin.
    /// Immediate kill for the process. The kernel (q_term) despawns the
    /// [Process] immediately. Cannot be caught.
    Kill,
    /// Produced by: pty close, kill builtin.
    /// Signifies that any listeners have 'hung up' and are no longer available.
    /// Typically used as a reload mechanism or to exit a repl.
    Hup,
    /// [SIGSTOP]
    Stop,
    /// [SIGCONT]
    Cont,
    /// [SIGTTIN]
    Ttin,
    /// [SIGTTOUT]
    Ttou,
    // Usr1, Usr2, Pipe, Chld, Winch as needed
}

/// Message to send a signal to a [`Job`].
/// These are also known as interrupts.
#[derive(Message, Clone, Copy, Debug, Reflect)]
pub struct SignalMsg {
    /// Source [`Terminal`]
    pub term: Entity,
    /// Message sink - the targeted job
    pub target: Entity,
    /// Signal kind
    pub signal: Sig,
}
