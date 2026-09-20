//! Descriptor routing, process input, and process-local I/O teardown.

use std::{any::TypeId, sync::Arc};

use crate::prelude::*;

pub(crate) fn demux_input<T: IoMessage>(
    mut messages: ResMut<Messages<ProcessInputMsg<T>>>,
    components: Res<IoComponentCache>,
    mut queries: ParamSet<(
        Query<(&ProcessFdTable, &mut ProcessInputBuffer<T>), With<Process>>,
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

        let fd = message.fd;
        input
            .entry(fd)
            .or_default()
            .push_back(message.into_shared());
    }
}

pub(crate) fn route_writes<T: IoMessage>(
    mut messages: ResMut<Messages<ProcessWriteMsg<T>>>,
    descriptors: Query<&ProcessFdTable>,
    closing: Res<ClosingProcessIo>,
    entities: Query<()>,
    endpoints: Query<&IoCapabilities>,
    components: Res<IoComponentCache>,
    tees: Query<&TeeEndpoint<T>>,
    pipes: Query<&PipeEndpoint<T>>,
    mut routed: MessageWriter<EndpointWriteMsg<T>>,
    mut inputs: MessageWriter<ProcessInputMsg<T>>,
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
        let process = message.process;
        let fd = message.fd;
        let payload = message.into_shared();
        route_endpoint(
            process,
            fd,
            endpoint,
            &payload,
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

fn route_endpoint<T: IoMessage>(
    process: Entity,
    fd: FileDescriptor,
    endpoint: IoHandle,
    payload: &Arc<T>,
    expand_tee: bool,
    capabilities: &IoComponentCache,
    endpoints: &Query<&IoCapabilities>,
    tees: &Query<&TeeEndpoint<T>>,
    pipes: &Query<&PipeEndpoint<T>>,
    routed: &mut MessageWriter<EndpointWriteMsg<T>>,
    inputs: &mut MessageWriter<ProcessInputMsg<T>>,
) {
    if !capabilities.is_open(endpoint, endpoints) {
        warn!("Discarding write to closed endpoint {endpoint:?}");
        return;
    }
    if !capabilities.accepts::<T>(endpoint) {
        warn!("Discarding write to endpoint on an incompatible I/O message lane {endpoint:?}");
        return;
    }

    let component = endpoint.component_type_id();
    if component == TypeId::of::<TeeEndpoint<T>>() {
        if !expand_tee {
            warn!("Discarding nested tee output {endpoint:?}; nested tees are not yet supported");
            return;
        }
        let Ok(tee) = tees.get(endpoint.entity()) else {
            warn!("Discarding write to missing tee endpoint {endpoint:?}");
            return;
        };
        routed.write(EndpointWriteMsg::new(
            process,
            fd,
            endpoint,
            Arc::clone(payload),
        ));
        for output in tee.outputs() {
            route_endpoint(
                process,
                fd,
                *output,
                payload,
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

    routed.write(EndpointWriteMsg::new(
        process,
        fd,
        endpoint,
        Arc::clone(payload),
    ));
    if component == TypeId::of::<PipeEndpoint<T>>() {
        let Ok(pipe) = pipes.get(endpoint.entity()) else {
            warn!("Discarding write to missing pipe endpoint {endpoint:?}");
            return;
        };
        inputs.write(ProcessInputMsg::from_shared(
            pipe.process(),
            pipe.fd(),
            endpoint,
            Arc::clone(payload),
        ));
    }
}

pub(crate) fn cleanup_process_io(mut closing: ResMut<ClosingProcessIo>) {
    closing.clear();
}
