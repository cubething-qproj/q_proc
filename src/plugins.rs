//! The primary [`Plugin`] for q_proc.

use bevy::{
    ecs::schedule::{ApplyDeferred, InternedScheduleLabel, ScheduleLabel},
    platform::collections::HashSet,
};

use crate::prelude::*;
use crate::systems::{io::*, prog::*};

/// Ordered slots for process execution and I/O systems.
#[derive(SystemSet, Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProcessSystems {
    /// Demux endpoint input into process-local buffers.
    QueueInput,
    /// Dispatch registered program systems.
    RunPrograms,
    /// Resolve process writes through descriptor tables.
    RouteWrites,
    /// Remove I/O state belonging to dead processes.
    Cleanup,
}

#[derive(Resource, Default, Deref, DerefMut)]
struct ProcessSchedules(HashSet<InternedScheduleLabel>);

pub(crate) fn add_process_schedule<S: ScheduleLabel + Clone>(app: &mut App, schedule: S) {
    app.init_resource::<ProcessSchedules>();
    let schedule_id = schedule.intern();
    if !app
        .world_mut()
        .resource_mut::<ProcessSchedules>()
        .insert(schedule_id)
    {
        return;
    }

    app.configure_sets(
        schedule.clone(),
        (
            ProcessSystems::RunPrograms,
            ProcessSystems::RouteWrites,
            ProcessSystems::Cleanup,
        )
            .chain(),
    );
    app.add_systems(
        schedule,
        (
            (move |commands: Commands,
                   processes: Query<(Entity, &Process)>,
                   programs: Res<Programs>| {
                run_programs(schedule_id, commands, processes, programs);
            })
            .in_set(ProcessSystems::RunPrograms),
            ApplyDeferred
                .after(ProcessSystems::RunPrograms)
                .before(ProcessSystems::RouteWrites),
            route_writes.in_set(ProcessSystems::RouteWrites),
            cleanup_process_io.in_set(ProcessSystems::Cleanup),
        ),
    );
}

/// Registers process-management messages and runs programs across every standard schedule.
#[derive(Debug)]
pub struct ProcessPlugin;
impl Plugin for ProcessPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Programs>();
        app.init_resource::<IoComponentCache>();
        app.init_resource::<ClosingProcessIo>();
        app.add_message::<SignalMsg>();
        app.add_message::<ProcessWriteMsg>();
        app.add_message::<EndpointWriteMsg>();
        app.add_message::<ProcessInputMsg>();
        app.configure_sets(
            First,
            (ProcessSystems::QueueInput, ProcessSystems::Cleanup).chain(),
        );
        app.add_systems(
            First,
            (
                demux_input.in_set(ProcessSystems::QueueInput),
                cleanup_process_io.in_set(ProcessSystems::Cleanup),
            ),
        );

        add_process_schedule(app, PreUpdate);
        add_process_schedule(app, Update);
        add_process_schedule(app, PostUpdate);
        add_process_schedule(app, FixedPreUpdate);
        add_process_schedule(app, FixedUpdate);
        add_process_schedule(app, FixedPostUpdate);
    }
}
