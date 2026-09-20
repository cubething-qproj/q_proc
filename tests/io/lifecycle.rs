use super::*;

fn write_then_exit(
    In(process): In<Entity>,
    mut commands: Commands,
    mut writes: MessageWriter<ProcessWriteMsg>,
) {
    writes.write(ProcessWriteMsg::stdout(process, b"final".to_vec()));
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
        .resource_mut::<Messages<EndpointWriteMsg>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].process(), process);
    assert_eq!(writes[0].bytes(), b"final");

    let process_entity = app.world().entity(process);
    assert!(!process_entity.contains::<Process>());
    assert!(!process_entity.contains::<ProcessFdTable>());
    assert!(!process_entity.contains::<ProcessInputBuffer>());

    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(process, b"late".to_vec()));
    app.world_mut().run_schedule(Update);
    assert!(
        app.world_mut()
            .resource_mut::<Messages<EndpointWriteMsg>>()
            .drain()
            .next()
            .is_none()
    );
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
        .write_message(ProcessWriteMsg::stdout(process, b"late".to_vec()));
    app.world_mut().run_schedule(Update);

    assert!(app.world().get_entity(process).is_err());
    assert!(
        app.world_mut()
            .resource_mut::<Messages<EndpointWriteMsg>>()
            .drain()
            .next()
            .is_none()
    );
}
