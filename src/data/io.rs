//! Process-local descriptors and endpoint-neutral I/O messages.

use std::{any::TypeId, collections::VecDeque, sync::Arc};

use bevy::platform::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::prelude::*;

/// A payload type carried by a process I/O lane.
///
/// Messages retain their order within one `T` lane. Different message types use
/// independent Bevy resources and have no ordering guarantee relative to each other.
pub trait IoMessage: Send + Sync + 'static {}

impl IoMessage for Vec<u8> {}

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
#[derive(Resource, Debug, Default)]
pub struct IoComponentCache(HashMap<TypeId, Option<TypeId>>);

impl IoComponentCache {
    /// Creates a handle when `T` is registered and `entity` currently carries it.
    pub fn handle<T: IoComponent>(
        &self,
        entity: Entity,
        endpoints: &Query<(), With<T>>,
    ) -> Option<IoHandle> {
        (self.0.contains_key(&TypeId::of::<T>()) && endpoints.contains(entity)).then_some(
            IoHandle {
                entity,
                component: TypeId::of::<T>(),
            },
        )
    }

    pub(crate) fn is_open(&self, endpoint: IoHandle, endpoints: &Query<&IoCapabilities>) -> bool {
        self.0.contains_key(&endpoint.component_type_id())
            && endpoints
                .get(endpoint.entity())
                .is_ok_and(|capabilities| capabilities.contains(&endpoint.component_type_id()))
    }

    pub(crate) fn accepts<T: IoMessage>(&self, endpoint: IoHandle) -> bool {
        self.0
            .get(&endpoint.component_type_id())
            .is_some_and(|message| message.is_none_or(|message| message == TypeId::of::<T>()))
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

#[derive(Event)]
pub(crate) struct EndpointClosed {
    entity: Entity,
    component: TypeId,
}

fn add_endpoint_capability<T: IoComponent>(added: On<Add, T>, mut commands: Commands) {
    let component = TypeId::of::<T>();
    commands
        .entity(added.entity)
        .entry::<IoCapabilities>()
        .or_default()
        .and_modify(move |mut capabilities| {
            capabilities.insert(component);
        });
}

fn close_removed_endpoint<T: IoComponent>(
    removed: On<Remove, T>,
    mut commands: Commands,
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
    commands.trigger(EndpointClosed {
        entity: endpoint,
        component,
    });
}

pub(crate) fn close_tee_outputs<T: IoMessage>(
    closed: On<EndpointClosed>,
    mut tees: Query<&mut TeeEndpoint<T>>,
) {
    let closed = closed.event();
    for mut tee in &mut tees {
        tee.close_output(closed.entity, closed.component);
    }
}

fn register_io_component<T: IoComponent>(app: &mut App, message: Option<TypeId>) {
    app.init_resource::<IoComponentCache>();
    app.init_resource::<ClosingProcessIo>();
    let inserted = {
        let mut components = app.world_mut().resource_mut::<IoComponentCache>();
        match components.0.entry(TypeId::of::<T>()) {
            bevy::platform::collections::hash_map::Entry::Occupied(mut entry) => {
                match (*entry.get(), message) {
                    (None, _) | (Some(_), None) => *entry.get_mut() = None,
                    (Some(existing), Some(message)) if existing == message => {}
                    (Some(existing), Some(message)) => panic!(
                        "I/O component {:?} is already registered for message type {existing:?}, not {message:?}",
                        TypeId::of::<T>()
                    ),
                }
                false
            }
            bevy::platform::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(message);
                true
            }
        }
    };
    if inserted {
        app.add_observer(add_endpoint_capability::<T>);
        app.add_observer(close_removed_endpoint::<T>);

        let endpoints = {
            let world = app.world_mut();
            let mut query = world.query_filtered::<Entity, With<T>>();
            query.iter(world).collect::<Vec<_>>()
        };
        for endpoint in endpoints {
            let mut endpoint = app.world_mut().entity_mut(endpoint);
            if !endpoint.contains::<IoCapabilities>() {
                endpoint.insert(IoCapabilities::default());
            }
            endpoint
                .get_mut::<IoCapabilities>()
                .expect("IoCapabilities was just inserted")
                .insert(TypeId::of::<T>());
        }
    }
}

pub(crate) fn register_typed_io_component<C: IoComponent, T: IoMessage>(app: &mut App) {
    register_io_component::<C>(app, Some(TypeId::of::<T>()));
}

/// Registers component types that may act as I/O endpoint capabilities.
pub trait RegisterIoAppExt {
    /// Registers `T` as an I/O endpoint capability accepting every I/O message type.
    fn register_io_component<T: IoComponent>(&mut self) -> &mut Self;

    /// Registers endpoint component `C` for exactly one I/O message lane `T`.
    fn register_io_component_for<C: IoComponent, T: IoMessage>(&mut self) -> &mut Self;
}

impl RegisterIoAppExt for App {
    fn register_io_component<T: IoComponent>(&mut self) -> &mut Self {
        register_io_component::<T>(self, None);
        self
    }

    fn register_io_component_for<C: IoComponent, T: IoMessage>(&mut self) -> &mut Self {
        register_typed_io_component::<C, T>(self);
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
#[derive(Message, Clone, Debug)]
pub struct ProcessWriteMsg<T: IoMessage = Vec<u8>> {
    /// Process requesting the write.
    pub process: Entity,
    /// Descriptor through which to write.
    pub fd: FileDescriptor,
    payload: Arc<T>,
}

impl<T: IoMessage> ProcessWriteMsg<T> {
    /// Creates a process write through `fd`.
    pub fn new(process: Entity, fd: FileDescriptor, payload: T) -> Self {
        Self::from_shared(process, fd, Arc::new(payload))
    }

    /// Creates a process write through `fd` from an existing shared payload.
    pub fn from_shared(process: Entity, fd: FileDescriptor, payload: Arc<T>) -> Self {
        Self {
            process,
            fd,
            payload,
        }
    }

    /// Creates a standard-output write.
    pub fn stdout(process: Entity, payload: T) -> Self {
        Self::new(process, FileDescriptor::STDOUT, payload)
    }

    /// Creates a standard-error write.
    pub fn stderr(process: Entity, payload: T) -> Self {
        Self::new(process, FileDescriptor::STDERR, payload)
    }

    /// Returns the payload.
    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Returns a shared pointer to the payload.
    pub fn shared(&self) -> Arc<T> {
        Arc::clone(&self.payload)
    }

    pub(crate) fn into_shared(self) -> Arc<T> {
        self.payload
    }
}

/// A process write whose descriptor has been resolved to an endpoint.
#[derive(Message, Clone, Debug)]
pub struct EndpointWriteMsg<T: IoMessage = Vec<u8>> {
    process: Entity,
    fd: FileDescriptor,
    endpoint: IoHandle,
    payload: Arc<T>,
}

impl<T: IoMessage> EndpointWriteMsg<T> {
    pub(crate) fn new(
        process: Entity,
        fd: FileDescriptor,
        endpoint: IoHandle,
        payload: Arc<T>,
    ) -> Self {
        Self {
            process,
            fd,
            endpoint,
            payload,
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

    /// Returns the payload.
    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Returns a shared pointer to the payload.
    pub fn shared(&self) -> Arc<T> {
        Arc::clone(&self.payload)
    }
}

/// A message made available to one process descriptor by an endpoint adapter.
#[derive(Message, Clone, Debug)]
pub struct ProcessInputMsg<T: IoMessage = Vec<u8>> {
    /// Process receiving the message.
    pub process: Entity,
    /// Descriptor receiving the message.
    pub fd: FileDescriptor,
    /// Endpoint capability that supplied the message.
    pub endpoint: IoHandle,
    payload: Arc<T>,
}

impl<T: IoMessage> ProcessInputMsg<T> {
    /// Creates process input from an owned payload.
    pub fn new(process: Entity, fd: FileDescriptor, endpoint: IoHandle, payload: T) -> Self {
        Self::from_shared(process, fd, endpoint, Arc::new(payload))
    }

    /// Creates process input from an existing shared payload.
    pub fn from_shared(
        process: Entity,
        fd: FileDescriptor,
        endpoint: IoHandle,
        payload: Arc<T>,
    ) -> Self {
        Self {
            process,
            fd,
            endpoint,
            payload,
        }
    }

    /// Returns the payload.
    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Returns a shared pointer to the payload.
    pub fn shared(&self) -> Arc<T> {
        Arc::clone(&self.payload)
    }

    pub(crate) fn into_shared(self) -> Arc<T> {
        self.payload
    }
}

/// Process-local input queues populated before program execution.
///
/// Queued payloads remain immutable and shared so tee fanout does not copy `T`.
#[derive(Component, Clone, Debug, Deref, DerefMut)]
pub struct ProcessInputBuffer<T: IoMessage = Vec<u8>>(HashMap<FileDescriptor, VecDeque<Arc<T>>>);

impl<T: IoMessage> Default for ProcessInputBuffer<T> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}
