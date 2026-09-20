//! Descriptor routing, process input, and process-local I/O teardown.

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

pub(crate) fn cleanup_process_io(mut closing: ResMut<ClosingProcessIo>) {
    closing.clear();
}
