use super::*;

#[derive(Resource)]
struct ProcessToRemove(Entity);

fn remove_process(mut commands: Commands, process: Res<ProcessToRemove>) {
    commands.entity(process.0).remove::<Process>();
}

fn write_then_exit(
    In(process): In<Entity>,
    mut commands: Commands,
    mut writes: MessageWriter<ProcessWriteMsg<Vec<u8>>>,
) {
    writes.write(ProcessWriteMsg::<Vec<u8>>::stdout(
        process,
        b"final".to_vec(),
    ));
    commands.entity(process).remove::<Process>();
}

#[test]
fn final_write_routes_before_process_io_cleanup() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.program::<IoProgram>()
        .add_system(Update, write_then_exit);

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);

    app.world_mut().run_schedule(Update);

    let writes = app
        .world_mut()
        .resource_mut::<Messages<EndpointWriteMsg<Vec<u8>>>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].process(), process);
    assert_eq!(writes[0].payload(), b"final");

    let process_entity = app.world().entity(process);
    assert!(!process_entity.contains::<Process>());
    assert!(!process_entity.contains::<ProcessFdTable>());
    assert!(!process_entity.contains::<ProcessInputBuffer<Vec<u8>>>());

    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            process,
            b"late".to_vec(),
        ));
    app.world_mut().run_schedule(Update);
    assert!(
        app.world_mut()
            .resource_mut::<Messages<EndpointWriteMsg<Vec<u8>>>>()
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

    let process = app.world().entity(process);
    assert!(!process.contains::<Process>());
    assert!(!process.contains::<ProcessFdTable>());
    assert!(!process.contains::<ProcessInputBuffer<Vec<u8>>>());
}

#[test]
fn process_despawn_needs_no_shell_cleanup() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);
    assert!(app.world_mut().despawn(process));

    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            process,
            b"late".to_vec(),
        ));
    app.world_mut().run_schedule(Update);

    assert!(app.world().get_entity(process).is_err());
    assert!(
        app.world_mut()
            .resource_mut::<Messages<EndpointWriteMsg<Vec<u8>>>>()
            .drain()
            .next()
            .is_none()
    );
}
