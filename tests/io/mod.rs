use std::any::TypeId;

use crate::prelude::*;

#[derive(Component)]
struct FirstEndpoint;

impl IoComponent for FirstEndpoint {}

#[derive(Component)]
struct SecondEndpoint;

impl IoComponent for SecondEndpoint {}

fn endpoint_handle(app: &mut App, entity: Entity) -> IoHandle {
    let world = app.world_mut();
    let mut endpoints = world.query_filtered::<(), With<FirstEndpoint>>();
    let endpoints = endpoints.query(world);
    world
        .resource::<IoComponentCache>()
        .handle::<FirstEndpoint>(entity, &endpoints)
        .expect("the registered component on the entity should produce a handle")
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

    let stdout = ProcessWriteMsg::stdout(process, b"out".to_vec());
    let stderr = ProcessWriteMsg::stderr(process, b"err".to_vec());
    let custom = ProcessWriteMsg::new(process, FileDescriptor::new(9), b"custom".to_vec());

    assert_eq!(stdout.process, process);
    assert_eq!(stdout.fd, FileDescriptor::STDOUT);
    assert_eq!(stdout.bytes, b"out");
    assert_eq!(stderr.fd, FileDescriptor::STDERR);
    assert_eq!(stderr.bytes, b"err");
    assert_eq!(custom.fd, FileDescriptor::new(9));
    assert_eq!(custom.bytes, b"custom");
}

#[test]
fn input_buffers_are_empty_until_the_demultiplexer_appends_bytes() {
    let mut input = ProcessInputBuffer::default();

    assert!(input.is_empty(FileDescriptor::STDIN));
    assert_eq!(input.len(FileDescriptor::STDIN), 0);
    assert_eq!(
        input.drain(FileDescriptor::STDIN).collect::<Vec<_>>(),
        Vec::<u8>::new()
    );
}
