//! Process-local descriptors and endpoint-neutral I/O messages.

use std::{any::TypeId, collections::VecDeque};

use bevy::platform::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::prelude::*;

/// A process-local file descriptor number.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Reflect)]
pub struct FileDescriptor(u16);

impl FileDescriptor {
    /// Standard input.
    pub const STDIN: Self = Self(0);
    /// Standard output.
    pub const STDOUT: Self = Self(1);
    /// Standard error.
    pub const STDERR: Self = Self(2);

    /// Creates a file descriptor from its process-local number.
    pub const fn new(number: u16) -> Self {
        Self(number)
    }

    /// Returns the process-local descriptor number.
    pub const fn number(self) -> u16 {
        self.0
    }
}

/// Implemented by component types that make an entity I/O-capable.
pub trait IoComponent: Component {}

/// Registered component types that may be selected by an [`IoHandle`].
#[derive(Resource, Debug, Default, Deref)]
pub struct IoComponentCache(HashSet<TypeId>);

impl IoComponentCache {
    /// Creates a handle when `T` is registered and `entity` currently carries it.
    pub fn handle<T: IoComponent>(
        &self,
        entity: Entity,
        endpoints: &Query<(), With<T>>,
    ) -> Option<IoHandle> {
        (self.0.contains(&TypeId::of::<T>()) && endpoints.contains(entity)).then_some(IoHandle {
            entity,
            component: TypeId::of::<T>(),
        })
    }

    pub(crate) fn is_open(&self, endpoint: IoHandle, endpoints: &Query<&IoCapabilities>) -> bool {
        self.contains(&endpoint.component_type_id())
            && endpoints
                .get(endpoint.entity())
                .is_ok_and(|capabilities| capabilities.contains(&endpoint.component_type_id()))
    }
}

/// Registered endpoint component types currently present on an entity.
///
/// Each type identifies one adapter that can consume writes addressed to that
/// capability; an [`IoHandle`] selects exactly one such type on the entity.
#[derive(Component, Default, Deref, DerefMut)]
pub(crate) struct IoCapabilities(HashSet<TypeId>);

/// Descriptor tables retained until final writes from removed processes are routed.
#[derive(Resource, Default, Deref, DerefMut)]
pub(crate) struct ClosingProcessIo(HashMap<Entity, ProcessFdTable>);

fn add_endpoint_capability<T: IoComponent>(
    added: On<Add, T>,
    mut commands: Commands,
    mut endpoints: Query<&mut IoCapabilities>,
) {
    if let Ok(mut capabilities) = endpoints.get_mut(added.entity) {
        capabilities.insert(TypeId::of::<T>());
    } else {
        commands
            .entity(added.entity)
            .insert(IoCapabilities(HashSet::from([TypeId::of::<T>()])));
    }
}

fn close_removed_endpoint<T: IoComponent>(
    removed: On<Remove, T>,
    mut endpoints: Query<&mut IoCapabilities>,
    mut descriptors: Query<&mut ProcessFdTable>,
    mut closing: ResMut<ClosingProcessIo>,
) {
    let endpoint = removed.entity;
    let component = TypeId::of::<T>();
    if let Ok(mut capabilities) = endpoints.get_mut(endpoint) {
        capabilities.remove(&component);
    }
    for mut table in &mut descriptors {
        table.close_endpoint(endpoint, component);
    }
    for table in closing.values_mut() {
        table.close_endpoint(endpoint, component);
    }
}

/// Registers component types that may act as I/O endpoint capabilities.
pub trait RegisterIoAppExt {
    /// Registers `T` as an I/O endpoint capability.
    fn register_io_component<T: IoComponent>(&mut self) -> &mut Self;
}

impl RegisterIoAppExt for App {
    fn register_io_component<T: IoComponent>(&mut self) -> &mut Self {
        self.init_resource::<IoComponentCache>();
        self.init_resource::<ClosingProcessIo>();
        let inserted = self
            .world_mut()
            .resource_mut::<IoComponentCache>()
            .0
            .insert(TypeId::of::<T>());
        if inserted {
            self.add_observer(add_endpoint_capability::<T>);
            self.add_observer(close_removed_endpoint::<T>);
        }
        self
    }
}

/// A runtime-typed reference to one registered I/O capability on an entity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Reflect)]
#[reflect(opaque)]
pub struct IoHandle {
    entity: Entity,
    component: TypeId,
}

impl IoHandle {
    /// Returns the endpoint entity.
    pub const fn entity(self) -> Entity {
        self.entity
    }

    /// Returns the component type selected as the endpoint capability.
    pub const fn component_type_id(self) -> TypeId {
        self.component
    }
}

/// An error returned when an operation requires an open file descriptor.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("file descriptor {0:?} is not open")]
pub struct BadFd(pub FileDescriptor);

/// The mutable file descriptor table for one process.
#[derive(Component, Clone, Debug, Default, Reflect)]
pub struct ProcessFdTable {
    descriptors: HashMap<FileDescriptor, IoHandle>,
}

impl ProcessFdTable {
    /// Returns the endpoint assigned to `fd`.
    pub fn get(&self, fd: FileDescriptor) -> Option<IoHandle> {
        self.descriptors.get(&fd).copied()
    }

    /// Assigns `handle` to `fd`, returning the previous endpoint if present.
    pub fn set(&mut self, fd: FileDescriptor, handle: IoHandle) -> Option<IoHandle> {
        self.descriptors.insert(fd, handle)
    }

    /// Closes `fd`, returning its previous endpoint if present.
    pub fn close(&mut self, fd: FileDescriptor) -> Option<IoHandle> {
        self.descriptors.remove(&fd)
    }

    /// Assigns `to` to the same endpoint as `from`.
    pub fn duplicate(&mut self, from: FileDescriptor, to: FileDescriptor) -> Result<(), BadFd> {
        let handle = self.get(from).ok_or(BadFd(from))?;
        self.set(to, handle);
        Ok(())
    }

    pub(crate) fn close_endpoint(&mut self, entity: Entity, component: TypeId) {
        self.descriptors.retain(|_, handle| {
            handle.entity() != entity || handle.component_type_id() != component
        });
    }
}

/// A write requested by a program through one of its descriptors.
#[derive(Message, Clone, Debug, Reflect)]
pub struct ProcessWriteMsg {
    /// Process requesting the write.
    pub process: Entity,
    /// Descriptor through which to write.
    pub fd: FileDescriptor,
    /// Uninterpreted bytes to write.
    pub bytes: Vec<u8>,
}

impl ProcessWriteMsg {
    /// Creates a process write through `fd`.
    pub fn new(process: Entity, fd: FileDescriptor, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            process,
            fd,
            bytes: bytes.into(),
        }
    }

    /// Creates a standard-output write.
    pub fn stdout(process: Entity, bytes: impl Into<Vec<u8>>) -> Self {
        Self::new(process, FileDescriptor::STDOUT, bytes)
    }

    /// Creates a standard-error write.
    pub fn stderr(process: Entity, bytes: impl Into<Vec<u8>>) -> Self {
        Self::new(process, FileDescriptor::STDERR, bytes)
    }
}

/// A process write whose descriptor has been resolved to an endpoint.
#[derive(Message, Clone, Debug, Reflect)]
#[reflect(opaque)]
pub struct EndpointWriteMsg {
    process: Entity,
    fd: FileDescriptor,
    endpoint: IoHandle,
    bytes: Vec<u8>,
}

impl EndpointWriteMsg {
    pub(crate) fn new(
        process: Entity,
        fd: FileDescriptor,
        endpoint: IoHandle,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            process,
            fd,
            endpoint,
            bytes,
        }
    }

    /// Returns the process that requested the write.
    pub const fn process(&self) -> Entity {
        self.process
    }

    /// Returns the descriptor through which the process wrote.
    pub const fn fd(&self) -> FileDescriptor {
        self.fd
    }

    /// Returns the resolved endpoint capability.
    pub const fn endpoint(&self) -> IoHandle {
        self.endpoint
    }

    /// Returns the uninterpreted bytes to write.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Bytes made available to one process descriptor by an endpoint adapter.
#[derive(Message, Clone, Debug, Reflect)]
pub struct ProcessInputMsg {
    /// Process receiving the bytes.
    pub process: Entity,
    /// Descriptor receiving the bytes.
    pub fd: FileDescriptor,
    /// Endpoint capability that supplied the bytes.
    pub endpoint: IoHandle,
    /// Uninterpreted input bytes.
    pub bytes: Vec<u8>,
}

/// Process-local input queues populated before program execution.
#[derive(Component, Clone, Debug, Default, Deref, DerefMut, Reflect)]
pub struct ProcessInputBuffer(HashMap<FileDescriptor, VecDeque<u8>>);
