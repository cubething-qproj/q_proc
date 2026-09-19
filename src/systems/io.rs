//! Descriptor routing, process input, and process-local I/O teardown.

use crate::prelude::*;

pub(crate) fn queue_input(world: &mut World) {
    let messages = world
        .resource_mut::<Messages<ProcessInputMsg>>()
        .drain()
        .collect::<Vec<_>>();

    for message in messages {
        let descriptor_matches = world
            .get_entity(message.process)
            .ok()
            .filter(|process| process.contains::<Process>())
            .and_then(|process| process.get::<ProcessFdTable>())
            .and_then(|descriptors| descriptors.get(message.fd))
            == Some(message.endpoint);

        if !descriptor_matches {
            warn!(
                "Discarding input for missing process or mismatched descriptor {:?}:{:?}",
                message.process, message.fd
            );
            continue;
        }
        if !endpoint_is_open(world, message.endpoint) {
            warn!(
                "Discarding input from closed endpoint {:?}",
                message.endpoint
            );
            continue;
        }

        let Ok(mut process) = world.get_entity_mut(message.process) else {
            continue;
        };
        let Some(mut input) = process.get_mut::<ProcessInputBuffer>() else {
            warn!(
                "Discarding input for process {:?} without an input buffer",
                message.process
            );
            continue;
        };
        input.append(message.fd, message.bytes);
    }
}

pub(crate) fn route_writes(world: &mut World) {
    let messages = world
        .resource_mut::<Messages<ProcessWriteMsg>>()
        .drain()
        .collect::<Vec<_>>();

    for message in messages {
        let endpoint = world
            .get_entity(message.process)
            .ok()
            .and_then(|process| process.get::<ProcessFdTable>())
            .and_then(|descriptors| descriptors.get(message.fd));

        let Some(endpoint) = endpoint else {
            warn!(
                "Discarding write for missing process or descriptor {:?}:{:?}",
                message.process, message.fd
            );
            continue;
        };
        if !endpoint_is_open(world, endpoint) {
            warn!("Discarding write to closed endpoint {endpoint:?}");
            continue;
        }

        world.write_message(EndpointWriteMsg::new(
            message.process,
            message.fd,
            endpoint,
            message.bytes,
        ));
    }
}

pub(crate) fn cleanup_process_io(
    mut commands: Commands,
    mut removed_processes: RemovedComponents<Process>,
) {
    for process in removed_processes.read() {
        let Ok(mut process) = commands.get_entity(process) else {
            continue;
        };
        process.remove::<(ProcessFdTable, ProcessInputBuffer)>();
    }
}

fn endpoint_is_open(world: &World, endpoint: IoHandle) -> bool {
    world
        .get_resource::<IoComponentCache>()
        .is_some_and(|components| components.contains(endpoint.component_type_id()))
        && world
            .get_entity(endpoint.entity())
            .is_ok_and(|entity| entity.contains_type_id(endpoint.component_type_id()))
}
