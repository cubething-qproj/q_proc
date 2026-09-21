//! Generic process I/O endpoints.

use std::{any::TypeId, marker::PhantomData};

use crate::prelude::*;

/// A pipe write endpoint that forwards messages of type `T` to one process descriptor.
///
/// This initial adapter does not yet model bounded capacity, blocking, wakeups,
/// or EOF.
#[derive(Component, Clone, Copy, Debug)]
#[component(immutable)]
pub struct PipeEndpoint<T: IoMessage> {
    process: Entity,
    fd: FileDescriptor,
    marker: PhantomData<T>,
}

impl<T: IoMessage> PipeEndpoint<T> {
    /// Creates a pipe endpoint for `process` and `fd`.
    pub const fn new(process: Entity, fd: FileDescriptor) -> Self {
        Self {
            process,
            fd,
            marker: PhantomData,
        }
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

impl<T: IoMessage> IoComponent for PipeEndpoint<T> {
    type Stdin = T;
    type Stdout = T;
}

/// An endpoint that duplicates each message of type `T` to configured downstream handles.
///
/// Outputs are visited in declaration order. Nested tees are not yet supported.
#[derive(Component, Clone, Debug)]
pub struct TeeEndpoint<T: IoMessage> {
    outputs: Vec<IoHandle>,
    marker: PhantomData<T>,
}

impl<T: IoMessage> Default for TeeEndpoint<T> {
    fn default() -> Self {
        Self {
            outputs: Vec::new(),
            marker: PhantomData,
        }
    }
}

impl<T: IoMessage> TeeEndpoint<T> {
    /// Creates a tee endpoint with outputs in forwarding order.
    pub fn new(outputs: impl IntoIterator<Item = IoHandle>) -> Self {
        Self {
            outputs: outputs.into_iter().collect(),
            marker: PhantomData,
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

impl<T: IoMessage> IoComponent for TeeEndpoint<T> {
    type Stdin = T;
    type Stdout = T;
}
