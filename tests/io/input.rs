use bevy::platform::collections::HashMap;

use super::*;

#[derive(Resource, Default)]
struct ConsumedInput(HashMap<Entity, Vec<u8>>);

fn consume_input(
    In(process): In<Entity>,
    mut buffers: Query<&mut ProcessInputBuffer>,
    mut consumed: ResMut<ConsumedInput>,
) {
    let mut buffer = buffers
        .get_mut(process)
        .expect("the dispatched process should have its input buffer");
    consumed
        .0
        .insert(process, buffer.drain(FileDescriptor::STDIN).collect());
}

#[test]
fn input_is_demultiplexed_before_shared_program_invocations() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.init_resource::<ConsumedInput>();
    app.add_program_system(IoProgram, PreUpdate, consume_input);

    let (_, endpoint) = first_endpoint(&mut app);
    let first = spawn_io_process(&mut app, &[(FileDescriptor::STDIN, endpoint)]);
    let second = spawn_io_process(&mut app, &[(FileDescriptor::STDIN, endpoint)]);

    app.world_mut().write_message(ProcessInputMsg {
        process: first,
        fd: FileDescriptor::STDIN,
        endpoint,
        bytes: b"first".to_vec(),
    });
    app.world_mut().write_message(ProcessInputMsg {
        process: first,
        fd: FileDescriptor::STDIN,
        endpoint,
        bytes: b"-continued".to_vec(),
    });
    app.world_mut().write_message(ProcessInputMsg {
        process: second,
        fd: FileDescriptor::STDIN,
        endpoint,
        bytes: b"second".to_vec(),
    });

    app.world_mut().run_schedule(First);
    app.world_mut().run_schedule(PreUpdate);

    let consumed = app.world().resource::<ConsumedInput>();
    assert_eq!(
        consumed.0.get(&first).map(Vec::as_slice),
        Some(b"first-continued".as_slice())
    );
    assert_eq!(
        consumed.0.get(&second).map(Vec::as_slice),
        Some(b"second".as_slice())
    );
}

#[test]
fn input_requires_the_current_descriptor_endpoint() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();

    let (_, current_endpoint) = first_endpoint(&mut app);
    let (_, stale_endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDIN, current_endpoint)]);

    app.world_mut().write_message(ProcessInputMsg {
        process,
        fd: FileDescriptor::STDIN,
        endpoint: stale_endpoint,
        bytes: b"stale".to_vec(),
    });
    app.world_mut().run_schedule(First);

    let input = app
        .world()
        .entity(process)
        .get::<ProcessInputBuffer>()
        .expect("the process input buffer should remain present");
    assert!(input.is_empty(FileDescriptor::STDIN));
}
