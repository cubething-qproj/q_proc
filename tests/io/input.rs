use std::collections::VecDeque;

use bevy::platform::collections::HashMap;

use super::*;

#[derive(Resource, Default)]
struct ConsumedInput(HashMap<Entity, Vec<u8>>);

fn consume_input(
    In(process): In<Entity>,
    mut buffers: Query<&mut ProcessInputBuffer<Vec<u8>>>,
    mut consumed: ResMut<ConsumedInput>,
) {
    let mut buffer = buffers
        .get_mut(process)
        .expect("the dispatched process should have its input buffer");
    consumed.0.insert(
        process,
        buffer
            .remove(&FileDescriptor::STDIN)
            .unwrap_or_default()
            .into_iter()
            .flat_map(|payload| payload.iter().copied().collect::<Vec<_>>())
            .collect(),
    );
}

#[test]
fn input_is_demultiplexed_before_shared_program_invocations() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.init_resource::<ConsumedInput>();
    app.program::<IoProgram>()
        .add_system(PreUpdate, consume_input);

    let (_, endpoint) = first_endpoint(&mut app);
    let first = spawn_io_process(&mut app, &[(FileDescriptor::STDIN, endpoint)]);
    let second = spawn_io_process(&mut app, &[(FileDescriptor::STDIN, endpoint)]);

    app.world_mut()
        .write_message(ProcessInputMsg::<Vec<u8>>::new(
            first,
            FileDescriptor::STDIN,
            endpoint,
            b"first".to_vec(),
        ));
    app.world_mut()
        .write_message(ProcessInputMsg::<Vec<u8>>::new(
            first,
            FileDescriptor::STDIN,
            endpoint,
            b"-continued".to_vec(),
        ));
    app.world_mut()
        .write_message(ProcessInputMsg::<Vec<u8>>::new(
            second,
            FileDescriptor::STDIN,
            endpoint,
            b"second".to_vec(),
        ));

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

    app.world_mut()
        .write_message(ProcessInputMsg::<Vec<u8>>::new(
            process,
            FileDescriptor::STDIN,
            stale_endpoint,
            b"stale".to_vec(),
        ));
    app.world_mut().run_schedule(First);

    let input = app
        .world()
        .entity(process)
        .get::<ProcessInputBuffer<Vec<u8>>>()
        .expect("the process input buffer should remain present");
    assert!(
        input
            .get(&FileDescriptor::STDIN)
            .is_none_or(VecDeque::is_empty)
    );
}
