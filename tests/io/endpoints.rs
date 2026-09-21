use super::*;

type BytePipe = PipeEndpoint<Vec<u8>>;
type ByteTee = TeeEndpoint<Vec<u8>>;
type ByteInputBuffer = ProcessInputBuffer<Vec<u8>>;
type ByteEndpointWrite = EndpointWriteMsg<Vec<u8>>;

#[test]
fn pipe_forwards_bytes_to_the_configured_process_descriptor() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);

    let reader = spawn_io_process(&mut app, &[]);
    let pipe_entity = app
        .world_mut()
        .spawn(BytePipe::new(reader, FileDescriptor::STDIN))
        .id();
    let pipe = io_handle::<BytePipe>(&mut app, pipe_entity);
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
        .get_mut::<ByteInputBuffer>()
        .expect("the reader should have an input buffer");
    assert_eq!(
        reader
            .remove(&FileDescriptor::STDIN)
            .expect("pipe bytes should reach the configured descriptor")
            .into_iter()
            .flat_map(|payload| payload.as_ref().clone())
            .collect::<Vec<_>>(),
        b"through pipe"
    );
}

#[test]
fn mixed_direct_and_tee_writes_preserve_pipe_order() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);

    let reader = spawn_io_process(&mut app, &[]);
    let pipe_entity = app
        .world_mut()
        .spawn(BytePipe::new(reader, FileDescriptor::STDIN))
        .id();
    let pipe = io_handle::<BytePipe>(&mut app, pipe_entity);
    app.world_mut()
        .entity_mut(reader)
        .get_mut::<ProcessFdTable>()
        .expect("the reader should have a descriptor table")
        .set(FileDescriptor::STDIN, pipe);
    let tee_entity = app.world_mut().spawn(ByteTee::new([pipe])).id();
    let tee = io_handle::<ByteTee>(&mut app, tee_entity);
    let writer = spawn_io_process(
        &mut app,
        &[
            (FileDescriptor::STDOUT, pipe),
            (FileDescriptor::STDERR, tee),
        ],
    );

    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(writer, b"a".to_vec()));
    app.world_mut()
        .write_message(ProcessWriteMsg::stderr(writer, b"b".to_vec()));
    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(writer, b"c".to_vec()));
    app.world_mut().run_schedule(Update);
    app.world_mut().run_schedule(First);

    let mut reader = app.world_mut().entity_mut(reader);
    let bytes = reader
        .get_mut::<ByteInputBuffer>()
        .expect("the reader should have an input buffer")
        .remove(&FileDescriptor::STDIN)
        .expect("all converging writes should reach the pipe")
        .into_iter()
        .flat_map(|payload| payload.iter().copied().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(bytes, b"abc");
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
    let tee_entity = app.world_mut().spawn(ByteTee::new([first, second])).id();
    let tee = io_handle::<ByteTee>(&mut app, tee_entity);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, tee)]);

    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(process, b"fan out".to_vec()));
    app.world_mut().run_schedule(Update);

    let writes = app
        .world_mut()
        .resource_mut::<Messages<ByteEndpointWrite>>()
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
    assert!(writes.iter().all(|write| write.payload() == b"fan out"));
}

#[test]
fn removed_tee_outputs_do_not_reopen_after_component_reinsertion() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_component::<FirstEndpoint>();

    let output_entity = app.world_mut().spawn(FirstEndpoint).id();
    let output = io_handle::<FirstEndpoint>(&mut app, output_entity);
    let reader = spawn_io_process(&mut app, &[]);
    let pipe_entity = app
        .world_mut()
        .spawn(BytePipe::new(reader, FileDescriptor::STDIN))
        .id();
    let pipe = io_handle::<BytePipe>(&mut app, pipe_entity);
    let tee_entity = app.world_mut().spawn(ByteTee::new([output, pipe])).id();
    let tee = io_handle::<ByteTee>(&mut app, tee_entity);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, tee)]);

    app.world_mut()
        .entity_mut(output_entity)
        .remove::<FirstEndpoint>();
    app.world_mut().entity_mut(pipe_entity).remove::<BytePipe>();
    app.world_mut()
        .entity_mut(output_entity)
        .insert(FirstEndpoint);
    app.world_mut()
        .entity_mut(pipe_entity)
        .insert(BytePipe::new(reader, FileDescriptor::STDIN));

    assert!(
        app.world()
            .entity(tee_entity)
            .get::<ByteTee>()
            .expect("the tee should remain live")
            .outputs()
            .is_empty()
    );

    app.world_mut()
        .write_message(ProcessWriteMsg::stdout(process, b"stale".to_vec()));
    app.world_mut().run_schedule(Update);
    app.world_mut().run_schedule(First);

    let writes = app
        .world_mut()
        .resource_mut::<Messages<ByteEndpointWrite>>()
        .drain()
        .filter(|write| write.endpoint() != tee)
        .count();
    assert_eq!(writes, 0);
    assert!(
        app.world()
            .entity(reader)
            .get::<ProcessInputBuffer<Vec<u8>>>()
            .expect("the reader should have an input buffer")
            .is_empty()
    );
}

#[derive(Debug, Eq, PartialEq)]
struct CustomMessage(&'static str);

impl IoMessage for CustomMessage {}

#[derive(Debug, Eq, PartialEq)]
struct OtherMessage;

impl IoMessage for OtherMessage {}

#[derive(Component)]
struct CustomEndpoint;

impl IoComponent for CustomEndpoint {
    type Stdin = ();
    type Stdout = CustomMessage;
}

#[test]
fn custom_messages_route_through_typed_pipes_and_share_tee_payloads() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);

    let first_reader = spawn_io_process(&mut app, &[]);
    app.register_io_msg::<CustomMessage>()
        .register_io_msg::<CustomMessage>();
    let second_reader = spawn_io_process(&mut app, &[]);
    assert!(
        app.world()
            .entity(first_reader)
            .contains::<ProcessInputBuffer<CustomMessage>>()
    );
    assert!(
        app.world()
            .entity(second_reader)
            .contains::<ProcessInputBuffer<CustomMessage>>()
    );

    let first_pipe_entity = app
        .world_mut()
        .spawn(PipeEndpoint::<CustomMessage>::new(
            first_reader,
            FileDescriptor::STDIN,
        ))
        .id();
    let first_pipe = io_handle::<PipeEndpoint<CustomMessage>>(&mut app, first_pipe_entity);
    let second_pipe_entity = app
        .world_mut()
        .spawn(PipeEndpoint::<CustomMessage>::new(
            second_reader,
            FileDescriptor::STDIN,
        ))
        .id();
    let second_pipe = io_handle::<PipeEndpoint<CustomMessage>>(&mut app, second_pipe_entity);
    for (reader, pipe) in [(first_reader, first_pipe), (second_reader, second_pipe)] {
        app.world_mut()
            .entity_mut(reader)
            .get_mut::<ProcessFdTable>()
            .expect("the reader should have a descriptor table")
            .set(FileDescriptor::STDIN, pipe);
    }

    let tee_entity = app
        .world_mut()
        .spawn(TeeEndpoint::<CustomMessage>::new([first_pipe, second_pipe]))
        .id();
    let tee = io_handle::<TeeEndpoint<CustomMessage>>(&mut app, tee_entity);
    let writer = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, tee)]);

    app.world_mut()
        .write_message(ProcessWriteMsg::<CustomMessage>::stdout(
            writer,
            CustomMessage("typed"),
        ));
    app.world_mut().run_schedule(Update);

    let routed = app
        .world_mut()
        .resource_mut::<Messages<EndpointWriteMsg<CustomMessage>>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(routed.len(), 3);
    assert!(
        routed[1..]
            .iter()
            .all(|message| std::ptr::eq(routed[0].payload(), message.payload()))
    );

    app.world_mut().run_schedule(First);
    let first_payload = app
        .world_mut()
        .entity_mut(first_reader)
        .get_mut::<ProcessInputBuffer<CustomMessage>>()
        .expect("the first reader should have a typed input buffer")
        .get_mut(&FileDescriptor::STDIN)
        .expect("the first pipe should receive typed input")
        .pop_front()
        .expect("the typed input queue should not be empty");
    let second_payload = app
        .world_mut()
        .entity_mut(second_reader)
        .get_mut::<ProcessInputBuffer<CustomMessage>>()
        .expect("the second reader should have a typed input buffer")
        .get_mut(&FileDescriptor::STDIN)
        .expect("the second pipe should receive typed input")
        .pop_front()
        .expect("the typed input queue should not be empty");
    assert_eq!(*first_payload, CustomMessage("typed"));
    assert!(std::sync::Arc::ptr_eq(&first_payload, &second_payload));

    app.world_mut().entity_mut(first_reader).remove::<Process>();
    assert!(
        !app.world()
            .entity(first_reader)
            .contains::<ProcessInputBuffer<CustomMessage>>()
    );
}

#[test]
fn typed_external_endpoint_rejects_a_different_message_lane() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_msg::<OtherMessage>()
        .register_io_component::<CustomEndpoint>();

    let endpoint_entity = app.world_mut().spawn(CustomEndpoint).id();
    let endpoint = io_handle::<CustomEndpoint>(&mut app, endpoint_entity);
    let process = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, endpoint)]);

    app.world_mut()
        .write_message(ProcessWriteMsg::<OtherMessage>::stdout(
            process,
            OtherMessage,
        ));
    app.world_mut().run_schedule(Update);
    assert!(
        app.world_mut()
            .resource_mut::<Messages<EndpointWriteMsg<OtherMessage>>>()
            .drain()
            .next()
            .is_none()
    );

    app.world_mut()
        .write_message(ProcessWriteMsg::<CustomMessage>::stdout(
            process,
            CustomMessage("accepted"),
        ));
    app.world_mut().run_schedule(Update);
    let accepted = app
        .world_mut()
        .resource_mut::<Messages<EndpointWriteMsg<CustomMessage>>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].payload(), &CustomMessage("accepted"));
}

#[test]
fn typed_pipe_rejects_a_different_message_lane() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_io_msg::<CustomMessage>()
        .register_io_msg::<OtherMessage>();

    let reader = spawn_io_process(&mut app, &[]);
    let pipe_entity = app
        .world_mut()
        .spawn(PipeEndpoint::<CustomMessage>::new(
            reader,
            FileDescriptor::STDIN,
        ))
        .id();
    let pipe = io_handle::<PipeEndpoint<CustomMessage>>(&mut app, pipe_entity);
    let incompatible_handle = {
        let world = app.world_mut();
        let mut endpoints = world.query_filtered::<(), With<PipeEndpoint<OtherMessage>>>();
        let endpoints = endpoints.query(world);
        world
            .resource::<IoComponentCache>()
            .handle::<PipeEndpoint<OtherMessage>>(pipe_entity, &endpoints)
    };
    assert!(incompatible_handle.is_none());

    let writer = spawn_io_process(&mut app, &[(FileDescriptor::STDOUT, pipe)]);
    app.world_mut()
        .write_message(ProcessWriteMsg::<OtherMessage>::stdout(
            writer,
            OtherMessage,
        ));
    app.world_mut().run_schedule(Update);
    app.world_mut().run_schedule(First);

    assert!(
        app.world_mut()
            .resource_mut::<Messages<EndpointWriteMsg<OtherMessage>>>()
            .drain()
            .next()
            .is_none()
    );
    assert!(
        app.world()
            .entity(reader)
            .get::<ProcessInputBuffer<OtherMessage>>()
            .expect("the reader should have an input buffer for the other lane")
            .is_empty()
    );
}
