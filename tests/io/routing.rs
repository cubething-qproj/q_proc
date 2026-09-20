use bevy::ecs::schedule::ScheduleLabel;

use super::*;

#[derive(Resource)]
struct Emission(&'static [u8]);

#[derive(ScheduleLabel, Clone, Debug, Eq, Hash, PartialEq)]
struct CustomProgramSchedule;

fn emit_configured_write(
    In(process): In<Entity>,
    emission: Res<Emission>,
    mut writes: MessageWriter<ProcessWriteMsg<Vec<u8>>>,
) {
    writes.write(ProcessWriteMsg::<Vec<u8>>::stdout(
        process,
        emission.0.to_vec(),
    ));
}

fn emit_ordered_writes(
    In(process): In<Entity>,
    mut writes: MessageWriter<ProcessWriteMsg<Vec<u8>>>,
) {
    writes.write(ProcessWriteMsg::<Vec<u8>>::stdout(process, b"a".to_vec()));
    writes.write(ProcessWriteMsg::<Vec<u8>>::stderr(process, b"b".to_vec()));
    writes.write(ProcessWriteMsg::<Vec<u8>>::stdout(process, b"c".to_vec()));
}

fn drain_routed_writes(app: &mut App) -> Vec<EndpointWriteMsg<Vec<u8>>> {
    app.world_mut()
        .resource_mut::<Messages<EndpointWriteMsg<Vec<u8>>>>()
        .drain()
        .collect()
}

#[test]
fn program_writes_route_in_cross_descriptor_order() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.program::<IoProgram>()
        .add_system(Update, emit_ordered_writes);

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(
        &mut app,
        &[
            (FileDescriptor::STDOUT, endpoint),
            (FileDescriptor::STDERR, endpoint),
        ],
    );

    app.world_mut().run_schedule(Update);

    let writes = drain_routed_writes(&mut app);
    assert_eq!(writes.len(), 3);
    assert!(writes.iter().all(|write| write.process() == process));
    assert!(writes.iter().all(|write| write.endpoint() == endpoint));
    assert_eq!(
        writes
            .iter()
            .map(|write| (write.fd(), write.payload().as_slice()))
            .collect::<Vec<_>>(),
        [
            (FileDescriptor::STDOUT, b"a".as_slice()),
            (FileDescriptor::STDERR, b"b".as_slice()),
            (FileDescriptor::STDOUT, b"c".as_slice()),
        ]
    );
}

#[test]
fn routing_contract_is_installed_in_every_program_schedule() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.insert_resource(Emission(b"initial"));

    app.program::<IoProgram>()
        .add_system(PreUpdate, emit_configured_write)
        .add_system(Update, emit_configured_write)
        .add_system(PostUpdate, emit_configured_write)
        .add_system(FixedPreUpdate, emit_configured_write)
        .add_system(FixedUpdate, emit_configured_write)
        .add_system(FixedPostUpdate, emit_configured_write);

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);

    macro_rules! assert_schedule {
        ($schedule:ident, $bytes:literal) => {{
            app.world_mut().resource_mut::<Emission>().0 = $bytes;
            app.world_mut().run_schedule($schedule);
            let writes = drain_routed_writes(&mut app);
            assert_eq!(writes.len(), 1, "{} routed count", stringify!($schedule));
            assert_eq!(writes[0].process(), process);
            assert_eq!(writes[0].fd(), FileDescriptor::STDOUT);
            assert_eq!(writes[0].endpoint(), endpoint);
            assert_eq!(writes[0].payload(), $bytes);
        }};
    }

    assert_schedule!(PreUpdate, b"pre-update");
    assert_schedule!(Update, b"update");
    assert_schedule!(PostUpdate, b"post-update");
    assert_schedule!(FixedPreUpdate, b"fixed-pre-update");
    assert_schedule!(FixedUpdate, b"fixed-update");
    assert_schedule!(FixedPostUpdate, b"fixed-post-update");
}

#[test]
fn program_schedule_registration_is_lazy_and_supports_custom_schedules() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();
    app.insert_resource(Emission(b"custom"));
    app.program::<IoProgram>()
        .add_system(CustomProgramSchedule, emit_configured_write);

    let (_, endpoint) = first_endpoint(&mut app);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);

    app.world_mut().run_schedule(CustomProgramSchedule);

    let writes = drain_routed_writes(&mut app);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].process(), process);
    assert_eq!(writes[0].payload(), b"custom");
}

#[test]
fn registration_backfills_existing_endpoint_components() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    let endpoint = app.world_mut().spawn(FirstEndpoint).id();
    app.register_io_component::<FirstEndpoint>();
    let handle = endpoint_handle(&mut app, endpoint);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, handle)]);

    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            process,
            b"backfilled".to_vec(),
        ));
    app.world_mut().run_schedule(Update);

    let writes = drain_routed_writes(&mut app);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].endpoint(), handle);
    assert_eq!(writes[0].payload(), b"backfilled");
}

#[test]
fn shared_endpoint_writes_retain_their_source_processes() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();

    let (_, endpoint) = first_endpoint(&mut app);
    let first = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);
    let second = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);
    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(first, b"first".to_vec()));
    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            second,
            b"second".to_vec(),
        ));

    app.world_mut().run_schedule(Update);

    let writes = drain_routed_writes(&mut app);
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].process(), first);
    assert_eq!(writes[0].payload(), b"first");
    assert_eq!(writes[1].process(), second);
    assert_eq!(writes[1].payload(), b"second");
    assert!(writes.iter().all(|write| write.endpoint() == endpoint));
}

#[test]
fn missing_process_table_and_descriptor_do_not_route() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();

    let (_, endpoint) = first_endpoint(&mut app);
    let missing_descriptor = spawn_io_process(&mut app, &[]);
    let missing_table = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);
    app.world_mut()
        .entity_mut(missing_table)
        .remove::<ProcessFdTable>();
    let missing_process = app.world_mut().spawn_empty().id();

    for process in [missing_descriptor, missing_table, missing_process] {
        app.world_mut()
            .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
                process,
                b"discarded".to_vec(),
            ));
    }
    app.world_mut().run_schedule(Update);

    assert!(drain_routed_writes(&mut app).is_empty());
}

#[test]
fn closed_endpoints_do_not_retarget_or_route() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>()
        .register_io_component::<SecondEndpoint>();

    let multi_endpoint = app.world_mut().spawn((FirstEndpoint, SecondEndpoint)).id();
    let selected = endpoint_handle(&mut app, multi_endpoint);
    let other = {
        let world = app.world_mut();
        let mut endpoints = world.query_filtered::<(), With<SecondEndpoint>>();
        let endpoints = endpoints.query(world);
        world
            .resource::<IoComponentCache>()
            .handle::<SecondEndpoint>(multi_endpoint, &endpoints)
            .expect("the second registered endpoint should produce a handle")
    };
    let selected_process = spawn_io_process(
        &mut app,
        &[
            (FileDescriptor::STDOUT, selected),
            (FileDescriptor::STDERR, other),
        ],
    );
    let alias_process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, selected)]);

    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            selected_process,
            b"selected".to_vec(),
        ));
    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stderr(
            selected_process,
            b"other".to_vec(),
        ));
    app.world_mut().run_schedule(Update);
    let writes = drain_routed_writes(&mut app);
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].endpoint(), selected);
    assert_eq!(writes[1].endpoint(), other);

    let (despawned_entity, despawned) = first_endpoint(&mut app);
    let despawned_process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, despawned)]);

    app.world_mut()
        .entity_mut(multi_endpoint)
        .remove::<FirstEndpoint>();
    assert_eq!(
        app.world()
            .entity(selected_process)
            .get::<ProcessFdTable>()
            .and_then(|table| table.get(FileDescriptor::STDOUT)),
        None
    );
    assert_eq!(
        app.world()
            .entity(selected_process)
            .get::<ProcessFdTable>()
            .and_then(|table| table.get(FileDescriptor::STDERR)),
        Some(other)
    );
    assert_eq!(
        app.world()
            .entity(alias_process)
            .get::<ProcessFdTable>()
            .and_then(|table| table.get(FileDescriptor::STDOUT)),
        None
    );
    app.world_mut()
        .entity_mut(multi_endpoint)
        .insert(FirstEndpoint);

    assert!(app.world_mut().despawn(despawned_entity));
    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            selected_process,
            b"removed then reinserted".to_vec(),
        ));
    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            alias_process,
            b"alias".to_vec(),
        ));
    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stderr(
            selected_process,
            b"other remains open".to_vec(),
        ));
    app.world_mut()
        .write_message(ProcessWriteMsg::<Vec<u8>>::stdout(
            despawned_process,
            b"despawned".to_vec(),
        ));

    app.world_mut().run_schedule(Update);

    let writes = drain_routed_writes(&mut app);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].endpoint(), other);
    assert_eq!(writes[0].payload(), b"other remains open");
}
