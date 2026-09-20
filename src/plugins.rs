//! The primary [`Plugin`] for q_proc.

use bevy::ecs::schedule::ApplyDeferred;

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

macro_rules! impl_program_schedules {
    ($app:ident, $($schedule:ident),+) => {
        $(
            $app.configure_sets(
                $schedule,
                (
                    ProcessSystems::RunPrograms,
                    ProcessSystems::RouteWrites,
                    ProcessSystems::Cleanup,
                )
                    .chain(),
            );
            $app.add_systems(
                $schedule,
                (
                    run_programs::<$schedule>.in_set(ProcessSystems::RunPrograms),
                    ApplyDeferred
                        .after(ProcessSystems::RunPrograms)
                        .before(ProcessSystems::RouteWrites),
                    route_writes.in_set(ProcessSystems::RouteWrites),
                    cleanup_process_io.in_set(ProcessSystems::Cleanup),
                ),
            );
        )+
    };
}

/// Registers process-management messages and runs programs across every standard schedule.
#[derive(Debug)]
pub struct ProcessPlugin;
impl Plugin for ProcessPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Programs>();
        app.init_resource::<IoComponentCache>();
        app.add_message::<SignalMsg>();
        app.add_message::<StdOut>();
        app.add_message::<StdErr>();
        app.add_message::<ProcessWriteMsg>();
        app.add_message::<EndpointWriteMsg>();
        app.add_message::<ProcessInputMsg>();
        app.add_systems(First, demux_input.in_set(ProcessSystems::QueueInput));

        impl_program_schedules!(
            app,
            PreUpdate,
            Update,
            PostUpdate,
            FixedPreUpdate,
            FixedUpdate,
            FixedPostUpdate
        );
    }
}
