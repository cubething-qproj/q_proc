//! Generic process I/O endpoints.

use crate::prelude::*;

/// A pipe write endpoint that forwards bytes to one process descriptor.
///
/// This initial adapter does not yet model bounded capacity, blocking, wakeups,
/// or EOF.
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[component(immutable)]
pub struct PipeEndpoint {
    process: Entity,
    fd: FileDescriptor,
}

impl PipeEndpoint {
    /// Creates a pipe endpoint for `process` and `fd`.
    pub const fn new(process: Entity, fd: FileDescriptor) -> Self {
        Self { process, fd }
    }

    /// Returns the process that receives pipe input.
    pub const fn process(&self) -> Entity {
        self.process
    }

    /// Returns the descriptor that receives pipe input.
    pub const fn fd(&self) -> FileDescriptor {
        self.fd
    }
}

impl IoComponent for PipeEndpoint {}

/// An endpoint that duplicates each write to configured downstream handles.
///
/// Outputs are visited in declaration order. Tee cycles are unsupported.
#[derive(Component, Clone, Debug, Default, Reflect)]
#[component(immutable)]
pub struct TeeEndpoint {
    outputs: Vec<IoHandle>,
}

impl TeeEndpoint {
    /// Creates a tee endpoint with outputs in forwarding order.
    pub fn new(outputs: impl IntoIterator<Item = IoHandle>) -> Self {
        Self {
            outputs: outputs.into_iter().collect(),
        }
    }

    /// Returns downstream handles in forwarding order.
    pub fn outputs(&self) -> &[IoHandle] {
        &self.outputs
    }
}

impl IoComponent for TeeEndpoint {}
