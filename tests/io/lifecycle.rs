use super::*;

type ByteWrite = ProcessWriteMsg<Vec<u8>>;
type ByteEndpointWrite = EndpointWriteMsg<Vec<u8>>;
type ByteInputBuffer = ProcessInputBuffer<Vec<u8>>;

#[derive(Resource)]
struct ProcessToRemove(Entity);

fn remove_process(
    mut commands: Commands,
    process: Res<ProcessToRemove>,
    mut writes: MessageWriter<ByteWrite>,
) {
    writes.write(ByteWrite::stdout(process.0, b"final".to_vec()));
    commands.entity(process.0).remove::<Process>();
}

fn write_then_exit(
    processes: Query<Entity, With<IoProgram>>,
    mut commands: Commands,
    mut writes: MessageWriter<ByteWrite>,
) {
    for process in &processes {
        writes.write(ByteWrite::stdout(process, b"final".to_vec()));
        commands.entity(process).exit(3);
    }
}

#[derive(Resource, Default)]
struct Exits(Vec<(Entity, ExitStatus)>);

/// A process that writes and despawns in the same frame reports its exit status
/// and has its final write routed through its cached descriptors.
#[test]
fn final_write_routes_before_process_io_cleanup() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.init_resource::<Exits>();
    app.add_observer(|exited: On<ProcessExited>, mut exits: ResMut<Exits>| {
        exits.0.push((exited.entity, exited.status));
    });
    app.program::<IoProgram>()
        .add_systems(Update, write_then_exit);

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);

    app.world_mut().run_schedule(Update);

    let writes = app
        .world_mut()
        .resource_mut::<Messages<ByteEndpointWrite>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].process(), process);
    assert_eq!(writes[0].payload(), b"final");

    assert_eq!(
        app.world().resource::<Exits>().0,
        [(process, ExitStatus::Code(3))]
    );
    assert!(app.world().get_entity(process).is_err());

    app.world_mut()
        .write_message(ByteWrite::stdout(process, b"late".to_vec()));
    app.world_mut().run_schedule(Update);
    assert!(
        app.world_mut()
            .resource_mut::<Messages<ByteEndpointWrite>>()
            .drain()
            .next()
            .is_none()
    );
}

#[test]
fn removal_after_program_schedules_still_cleans_process_io() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.add_systems(Last, remove_process);

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);
    app.insert_resource(ProcessToRemove(process));

    app.update();

    let process_entity = app.world().entity(process);
    assert!(!process_entity.contains::<Process>());
    assert!(!process_entity.contains::<ProcessFdTable>());
    assert!(!process_entity.contains::<ByteInputBuffer>());

    app.update();

    assert!(app.world().get_entity(process).is_err());
    let writes = app
        .world_mut()
        .resource_mut::<Messages<ByteEndpointWrite>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].process(), process);
    assert_eq!(writes[0].payload(), b"final");
}

#[test]
fn process_despawn_needs_no_shell_cleanup() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.init_resource::<Exits>();
    app.add_observer(|exited: On<ProcessExited>, mut exits: ResMut<Exits>| {
        exits.0.push((exited.entity, exited.status));
    });

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);
    app.update();
    app.world_mut()
        .write_message(ByteWrite::stdout(process, b"final".to_vec()));
    assert!(app.world_mut().despawn(process));
    app.update();

    assert!(app.world().get_entity(process).is_err());
    assert_eq!(
        app.world().resource::<Exits>().0,
        [(process, ExitStatus::Terminated)]
    );
    let writes = app
        .world_mut()
        .resource_mut::<Messages<ByteEndpointWrite>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].payload(), b"final");
}
