//! Descriptor routing, process input, and process-local I/O teardown.

use std::any::TypeId;

use crate::prelude::*;

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
    tees: Query<&TeeEndpoint>,
    pipes: Query<&PipeEndpoint>,
    mut routed: MessageWriter<EndpointWriteMsg>,
    mut inputs: MessageWriter<ProcessInputMsg>,
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
        route_endpoint(
            message.process,
            message.fd,
            endpoint,
            &message.bytes,
            true,
            &components,
            &endpoints,
            &tees,
            &pipes,
            &mut routed,
            &mut inputs,
        );
    }
}

fn route_endpoint(
    process: Entity,
    fd: FileDescriptor,
    endpoint: IoHandle,
    bytes: &[u8],
    expand_tee: bool,
    capabilities: &IoComponentCache,
    endpoints: &Query<&IoCapabilities>,
    tees: &Query<&TeeEndpoint>,
    pipes: &Query<&PipeEndpoint>,
    routed: &mut MessageWriter<EndpointWriteMsg>,
    inputs: &mut MessageWriter<ProcessInputMsg>,
) {
    if !capabilities.is_open(endpoint, endpoints) {
        warn!("Discarding write to closed endpoint {endpoint:?}");
        return;
    }

    let component = endpoint.component_type_id();
    if component == TypeId::of::<TeeEndpoint>() {
        if !expand_tee {
            warn!("Discarding nested tee output {endpoint:?}; nested tees are not yet supported");
            return;
        }
        let Ok(tee) = tees.get(endpoint.entity()) else {
            warn!("Discarding write to missing tee endpoint {endpoint:?}");
            return;
        };
        routed.write(EndpointWriteMsg::new(process, fd, endpoint, bytes.to_vec()));
        for output in tee.outputs() {
            route_endpoint(
                process,
                fd,
                *output,
                bytes,
                false,
                capabilities,
                endpoints,
                tees,
                pipes,
                routed,
                inputs,
            );
        }
        return;
    }

    routed.write(EndpointWriteMsg::new(process, fd, endpoint, bytes.to_vec()));
    if component == TypeId::of::<PipeEndpoint>() {
        let Ok(pipe) = pipes.get(endpoint.entity()) else {
            warn!("Discarding write to missing pipe endpoint {endpoint:?}");
            return;
        };
        inputs.write(ProcessInputMsg {
            process: pipe.process(),
            fd: pipe.fd(),
            endpoint,
            bytes: bytes.to_vec(),
        });
    }
}

pub(crate) fn cleanup_process_io(mut closing: ResMut<ClosingProcessIo>) {
    closing.clear();
}
