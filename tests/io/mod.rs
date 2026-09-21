use std::any::TypeId;

use bevy::{
    platform::collections::HashMap,
    reflect::{FromReflect, PartialReflect, ReflectRef, structs::DynamicStruct},
};

use crate::prelude::*;

mod endpoints;
mod input;
mod lifecycle;
mod routing;

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
struct IoProgram;

q_proc::impl_program_label!(IoProgram, "io-program");

#[derive(Component)]
struct FirstEndpoint;

impl IoComponent for FirstEndpoint {
    type Stdin = ();
    type Stdout = Vec<u8>;
}

#[derive(Component)]
struct SecondEndpoint;

impl IoComponent for SecondEndpoint {
    type Stdin = ();
    type Stdout = Vec<u8>;
}

fn io_handle<T: IoComponent>(app: &mut App, entity: Entity) -> IoHandle {
    let world = app.world_mut();
    let mut endpoints = world.query_filtered::<(), With<T>>();
    let endpoints = endpoints.query(world);
    world
        .resource::<IoComponentCache>()
        .handle::<T>(entity, &endpoints)
        .expect("the registered component on the entity should produce a handle")
}

fn endpoint_handle(app: &mut App, entity: Entity) -> IoHandle {
    io_handle::<FirstEndpoint>(app, entity)
}

fn first_endpoint(app: &mut App) -> (Entity, IoHandle) {
    let endpoint = app.world_mut().spawn(FirstEndpoint).id();
    let handle = endpoint_handle(app, endpoint);
    (endpoint, handle)
}

fn spawn_io_process(app: &mut App, descriptors: &[(FileDescriptor, IoHandle)]) -> Entity {
    let mut table = ProcessFdTable::default();
    for (fd, handle) in descriptors {
        table.set(*fd, *handle);
    }

    app.world_mut()
        .spawn((
            Process {
                prog: IoProgram.intern(),
                signal_overrides: HashMap::new(),
                argv: Vec::new(),
                environ: HashMap::new(),
            },
            table,
            ProcessInputBuffer::<Vec<u8>>::default(),
        ))
        .id()
}

#[test]
fn handles_require_a_registered_component_on_the_entity() {
    let mut app = App::new();
    app.register_io_component::<FirstEndpoint>();

    let endpoint = app.world_mut().spawn((FirstEndpoint, SecondEndpoint)).id();
    let unrelated = app.world_mut().spawn_empty().id();

    let handle = endpoint_handle(&mut app, endpoint);
    assert_eq!(handle.entity(), endpoint);
    assert_eq!(handle.component_type_id(), TypeId::of::<FirstEndpoint>());

    let world = app.world_mut();
    let mut first_endpoints = world.query_filtered::<(), With<FirstEndpoint>>();
    let first_endpoints = first_endpoints.query(world);
    assert!(
        world
            .resource::<IoComponentCache>()
            .handle::<FirstEndpoint>(unrelated, &first_endpoints)
            .is_none()
    );

    let mut second_endpoints = world.query_filtered::<(), With<SecondEndpoint>>();
    let second_endpoints = second_endpoints.query(world);
    assert!(
        world
            .resource::<IoComponentCache>()
            .handle::<SecondEndpoint>(endpoint, &second_endpoints)
            .is_none()
    );
}

#[test]
fn handles_select_one_capability_on_a_multi_capability_entity() {
    let mut app = App::new();
    app.register_io_component::<FirstEndpoint>()
        .register_io_component::<SecondEndpoint>();
    let endpoint = app.world_mut().spawn((FirstEndpoint, SecondEndpoint)).id();

    let first = endpoint_handle(&mut app, endpoint);
    let second = {
        let world = app.world_mut();
        let mut endpoints = world.query_filtered::<(), With<SecondEndpoint>>();
        let endpoints = endpoints.query(world);
        world
            .resource::<IoComponentCache>()
            .handle::<SecondEndpoint>(endpoint, &endpoints)
            .expect("the second registered capability should produce a handle")
    };

    assert_ne!(first, second);
    assert_eq!(first.entity(), second.entity());
    assert_eq!(first.component_type_id(), TypeId::of::<FirstEndpoint>());
    assert_eq!(second.component_type_id(), TypeId::of::<SecondEndpoint>());
}

#[test]
fn handles_are_opaque_to_reflection() {
    let mut app = App::new();
    app.register_io_component::<FirstEndpoint>();
    let endpoint = app.world_mut().spawn(FirstEndpoint).id();
    let handle = endpoint_handle(&mut app, endpoint);

    assert!(matches!(handle.reflect_ref(), ReflectRef::Opaque(_)));

    let mut forged_handle = DynamicStruct::default();
    forged_handle.insert("entity", endpoint);
    forged_handle.insert("component", TypeId::of::<FirstEndpoint>());
    assert!(IoHandle::from_reflect(&forged_handle).is_none());
}

#[test]
fn descriptor_tables_support_assignment_closure_and_duplication() {
    let mut app = App::new();
    app.register_io_component::<FirstEndpoint>();
    let endpoint = app.world_mut().spawn(FirstEndpoint).id();
    let handle = endpoint_handle(&mut app, endpoint);

    let custom = FileDescriptor::new(9);
    assert_eq!(custom.number(), 9);

    let mut descriptors = ProcessFdTable::default();
    assert_eq!(descriptors.set(FileDescriptor::STDOUT, handle), None);
    assert_eq!(descriptors.get(FileDescriptor::STDOUT), Some(handle));

    descriptors
        .duplicate(FileDescriptor::STDOUT, custom)
        .expect("an open descriptor should duplicate");
    assert_eq!(descriptors.get(custom), Some(handle));
    assert_eq!(descriptors.close(FileDescriptor::STDOUT), Some(handle));
    assert_eq!(descriptors.get(FileDescriptor::STDOUT), None);
    assert_eq!(
        descriptors.duplicate(FileDescriptor::STDERR, custom),
        Err(BadFd(FileDescriptor::STDERR))
    );
}

#[test]
fn write_constructors_use_one_message_type_and_preserve_bytes() {
    let process = Entity::from_raw_u32(7).expect("the test entity index should be valid");

    let stdout = ProcessWriteMsg::<Vec<u8>>::stdout(process, b"out".to_vec());
    let stderr = ProcessWriteMsg::<Vec<u8>>::stderr(process, b"err".to_vec());
    let custom =
        ProcessWriteMsg::<Vec<u8>>::new(process, FileDescriptor::new(9), b"custom".to_vec());

    assert_eq!(stdout.process, process);
    assert_eq!(stdout.fd, FileDescriptor::STDOUT);
    assert_eq!(stdout.payload(), b"out");
    assert_eq!(stderr.fd, FileDescriptor::STDERR);
    assert_eq!(stderr.payload(), b"err");
    assert_eq!(custom.fd, FileDescriptor::new(9));
    assert_eq!(custom.payload(), b"custom");
}

#[test]
fn spawning_a_process_inserts_registered_default_io_components() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    let process = app
        .world_mut()
        .spawn(Process {
            prog: IoProgram.intern(),
            signal_overrides: HashMap::new(),
            argv: Vec::new(),
            environ: HashMap::new(),
        })
        .id();

    let process = app.world().entity(process);
    let descriptors = process
        .get::<ProcessFdTable>()
        .expect("ProcessFdTable should be inserted with Process");
    let input = process
        .get::<ProcessInputBuffer<Vec<u8>>>()
        .expect("the registered byte lane should insert ProcessInputBuffer");
    assert!(descriptors.get(FileDescriptor::STDIN).is_none());
    assert!(descriptors.get(FileDescriptor::STDOUT).is_none());
    assert!(descriptors.get(FileDescriptor::STDERR).is_none());
    assert!(input.is_empty());
}

#[test]
fn input_buffers_are_empty_until_the_demultiplexer_appends_bytes() {
    let input = ProcessInputBuffer::<Vec<u8>>::default();

    assert!(input.is_empty());
    assert!(input.get(&FileDescriptor::STDIN).is_none());
}
