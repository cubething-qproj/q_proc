use super::*;

#[test]
fn pipe_forwards_bytes_to_the_configured_process_descriptor() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);

    let reader = spawn_io_process(&mut app, &[]);
    let pipe_entity = app
        .world_mut()
        .spawn(PipeEndpoint::new(reader, FileDescriptor::STDIN))
        .id();
    let pipe = io_handle::<PipeEndpoint>(&mut app, pipe_entity);
    app.world_mut()
        .entity_mut(reader)
        .get_mut::<ProcessFdTable>()
        .expect("the process should have a descriptor table")
        .set(FileDescriptor::STDIN, pipe);
    let writer = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, pipe)]);

    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(writer, b"through ".to_vec()));
    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(writer, b"pipe".to_vec()));
    app.world_mut().run_schedule(Update);
    app.world_mut().run_schedule(PostUpdate);
    app.world_mut().run_schedule(First);

    let mut reader_entity = app.world_mut().entity_mut(reader);
    let mut reader = reader_entity
        .get_mut::<ProcessInputBuffer>()
        .expect("the reader should have an input buffer");
    assert_eq!(
        reader
            .remove(&FileDescriptor::STDIN)
            .expect("pipe bytes should reach the configured descriptor")
            .into_iter()
            .collect::<Vec<_>>(),
        b"through pipe"
    );
}

#[test]
fn tee_duplicates_writes_to_downstream_handles_in_order() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>()
        .register_io_component::<SecondEndpoint>();

    let first_entity = app.world_mut().spawn(FirstEndpoint).id();
    let first = io_handle::<FirstEndpoint>(&mut app, first_entity);
    let second_entity = app.world_mut().spawn(SecondEndpoint).id();
    let second = io_handle::<SecondEndpoint>(&mut app, second_entity);
    let tee_entity = app
        .world_mut()
        .spawn(TeeEndpoint::new([first, second]))
        .id();
    let tee = io_handle::<TeeEndpoint>(&mut app, tee_entity);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, tee)]);

    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(process, b"fan out".to_vec()));
    app.world_mut().run_schedule(Update);

    let writes = app
        .world_mut()
        .resource_mut::<Messages<EndpointWriteMsg>>()
        .drain()
        .filter(|write| write.endpoint() != tee)
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].endpoint(), first);
    assert_eq!(writes[1].endpoint(), second);
    assert!(writes.iter().all(|write| write.process() == process));
    assert!(
        writes
            .iter()
            .all(|write| write.fd() == FileDescriptor::STDOUT)
    );
    assert!(writes.iter().all(|write| write.bytes() == b"fan out"));
}
