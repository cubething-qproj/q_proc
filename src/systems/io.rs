//! Descriptor routing, process input, and process-local I/O teardown.

use std::any::TypeId;

use bevy::ecs::message::MessageCursor;

use crate::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct PipeWriteCursor(MessageCursor<EndpointWriteMsg>);

#[derive(Resource, Default)]
pub(crate) struct TeeWriteCursor(MessageCursor<EndpointWriteMsg>);

pub(crate) fn demux_input(
    mut messages: ResMut<Messages<ProcessInputMsg>>,
    components: Res<IoComponentCache>,
    mut queries: ParamSet<(
        Query<(&ProcessFdTable, &mut ProcessInputBuffer), With<Process>>,
        Query<&IoCapabilities>,
    )>,
) {
    for message in messages.drain() {
        if !components.is_open(message.endpoint, &queries.p1()) {
            warn!(
                "Discarding input from closed endpoint {:?}",
                message.endpoint
            );
            continue;
        }

        let mut process_io = queries.p0();
        let Ok((descriptors, mut input)) = process_io.get_mut(message.process) else {
            warn!(
                "Discarding input for missing process or I/O state {:?}",
                message.process
            );
            continue;
        };
        if descriptors.get(message.fd) != Some(message.endpoint) {
            warn!(
                "Discarding input for mismatched descriptor {:?}:{:?}",
                message.process, message.fd
            );
            continue;
        }

        input.entry(message.fd).or_default().extend(message.bytes);
    }
}

pub(crate) fn route_writes(
    mut messages: ResMut<Messages<ProcessWriteMsg>>,
    descriptors: Query<&ProcessFdTable>,
    closing: Res<ClosingProcessIo>,
    entities: Query<()>,
    endpoints: Query<&IoCapabilities>,
    components: Res<IoComponentCache>,
    mut routed: MessageWriter<EndpointWriteMsg>,
) {
    for message in messages.drain() {
        let Some(endpoint) = descriptors
            .get(message.process)
            .ok()
            .or_else(|| {
                entities
                    .contains(message.process)
                    .then(|| closing.get(&message.process))
                    .flatten()
            })
            .and_then(|descriptors| descriptors.get(message.fd))
        else {
            warn!(
                "Discarding write for missing process or descriptor {:?}:{:?}",
                message.process, message.fd
            );
            continue;
        };
        if !components.is_open(endpoint, &endpoints) {
            warn!("Discarding write to closed endpoint {endpoint:?}");
            continue;
        }

        routed.write(EndpointWriteMsg::new(
            message.process,
            message.fd,
            endpoint,
            message.bytes,
        ));
    }
}

pub(crate) fn route_tee_writes(
    mut cursor: ResMut<TeeWriteCursor>,
    mut messages: ParamSet<(
        Res<Messages<EndpointWriteMsg>>,
        MessageWriter<EndpointWriteMsg>,
    )>,
    tees: Query<&TeeEndpoint>,
    capabilities: Res<IoComponentCache>,
    endpoints: Query<&IoCapabilities>,
) {
    let forwarded = {
        let messages = messages.p0();
        cursor
            .0
            .read(&messages)
            .filter(|write| write.endpoint().component_type_id() == TypeId::of::<TeeEndpoint>())
            .flat_map(|write| {
                let Ok(tee) = tees.get(write.endpoint().entity()) else {
                    warn!(
                        "Discarding write to missing tee endpoint {:?}",
                        write.endpoint()
                    );
                    return Vec::new();
                };
                tee.outputs()
                    .iter()
                    .filter(|output| {
                        let open = capabilities.is_open(**output, &endpoints);
                        if !open {
                            warn!("Discarding tee output to closed endpoint {output:?}");
                        }
                        open
                    })
                    .map(|output| {
                        EndpointWriteMsg::new(
                            write.process(),
                            write.fd(),
                            *output,
                            write.bytes().to_vec(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    messages.p1().write_batch(forwarded);
}

pub(crate) fn route_pipe_writes(
    mut cursor: ResMut<PipeWriteCursor>,
    messages: Res<Messages<EndpointWriteMsg>>,
    pipes: Query<&PipeEndpoint>,
    mut inputs: MessageWriter<ProcessInputMsg>,
) {
    for write in cursor
        .0
        .read(&messages)
        .filter(|write| write.endpoint().component_type_id() == TypeId::of::<PipeEndpoint>())
    {
        let Ok(pipe) = pipes.get(write.endpoint().entity()) else {
            warn!(
                "Discarding write to missing pipe endpoint {:?}",
                write.endpoint()
            );
            continue;
        };
        inputs.write(ProcessInputMsg {
            process: pipe.process(),
            fd: pipe.fd(),
            endpoint: write.endpoint(),
            bytes: write.bytes().to_vec(),
        });
    }
}

pub(crate) fn cleanup_process_io(mut closing: ResMut<ClosingProcessIo>) {
    closing.clear();
}
