//! Generic process I/O endpoints.

use std::any::TypeId;

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
/// Outputs are visited in declaration order. Nested tees are not yet supported.
#[derive(Component, Clone, Debug, Default, Reflect)]
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

    pub(crate) fn close_output(&mut self, entity: Entity, component: TypeId) {
        self.outputs
            .retain(|handle| handle.entity() != entity || handle.component_type_id() != component);
    }
}

impl IoComponent for TeeEndpoint {}
