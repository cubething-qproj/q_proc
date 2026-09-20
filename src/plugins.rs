//! The primary [`Plugin`] for q_proc.

use std::any::TypeId;

use bevy::{
    ecs::schedule::{ApplyDeferred, InternedScheduleLabel, ScheduleLabel},
    platform::collections::{HashMap, HashSet},
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

type IoScheduleInstaller = fn(&mut App, InternedScheduleLabel);

#[derive(Resource, Default, Deref, DerefMut)]
struct IoMessageRegistry(HashMap<TypeId, IoScheduleInstaller>);

fn install_io_schedule<T: IoMessage>(app: &mut App, schedule: InternedScheduleLabel) {
    app.add_systems(
        schedule,
        route_writes::<T>.in_set(ProcessSystems::RouteWrites),
    );
}

fn add_process_input_buffer<T: IoMessage>(added: On<Add, Process>, mut commands: Commands) {
    commands
        .entity(added.entity)
        .entry::<ProcessInputBuffer<T>>()
        .or_default();
}

fn remove_process_input_buffer<T: IoMessage>(removed: On<Remove, Process>, mut commands: Commands) {
    let process = removed.entity;
    commands.queue(move |world: &mut World| {
        if let Ok(mut process) = world.get_entity_mut(process) {
            process.remove::<ProcessInputBuffer<T>>();
        }
    });
}

/// Adds typed process I/O message lanes to an [`App`].
pub trait RegisterIoMessageAppExt {
    /// Idempotently registers the process I/O lane for `T`.
    fn register_io_msg<T: IoMessage>(&mut self) -> &mut Self;
}

impl RegisterIoMessageAppExt for App {
    fn register_io_msg<T: IoMessage>(&mut self) -> &mut Self {
        self.init_resource::<IoMessageRegistry>();
        self.init_resource::<ProcessSchedules>();
        let inserted = self
            .world_mut()
            .resource_mut::<IoMessageRegistry>()
            .insert(TypeId::of::<T>(), install_io_schedule::<T>)
            .is_none();
        if !inserted {
            return self;
        }

        self.add_message::<ProcessWriteMsg<T>>();
        self.add_message::<EndpointWriteMsg<T>>();
        self.add_message::<ProcessInputMsg<T>>();
        crate::data::io::register_typed_io_component::<PipeEndpoint<T>, T>(self);
        crate::data::io::register_typed_io_component::<TeeEndpoint<T>, T>(self);
        self.add_observer(add_process_input_buffer::<T>);
        self.add_observer(remove_process_input_buffer::<T>);
        self.add_observer(close_tee_outputs::<T>);
        self.add_systems(First, demux_input::<T>.in_set(ProcessSystems::QueueInput));

        let processes = {
            let world = self.world_mut();
            let mut query =
                world.query_filtered::<Entity, (With<Process>, Without<ProcessInputBuffer<T>>)>();
            query.iter(world).collect::<Vec<_>>()
        };
        for process in processes {
            self.world_mut()
                .entity_mut(process)
                .insert(ProcessInputBuffer::<T>::default());
        }

        let schedules = self
            .world()
            .resource::<ProcessSchedules>()
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for schedule in schedules {
            install_io_schedule::<T>(self, schedule);
        }
        self
    }
}

pub(crate) fn add_process_schedule<S: ScheduleLabel + Clone>(app: &mut App, schedule: S) {
    app.init_resource::<ProcessSchedules>();
    app.init_resource::<IoMessageRegistry>();
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
            cleanup_process_io.in_set(ProcessSystems::Cleanup),
        ),
    );

    let installers = app
        .world()
        .resource::<IoMessageRegistry>()
        .values()
        .copied()
        .collect::<Vec<_>>();
    for install in installers {
        install(app, schedule_id);
    }
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
        app.configure_sets(
            First,
            (ProcessSystems::QueueInput, ProcessSystems::Cleanup).chain(),
        );
        app.add_systems(First, cleanup_process_io.in_set(ProcessSystems::Cleanup));
        app.register_io_msg::<Vec<u8>>();

        add_process_schedule(app, PreUpdate);
        add_process_schedule(app, Update);
        add_process_schedule(app, PostUpdate);
        add_process_schedule(app, FixedPreUpdate);
        add_process_schedule(app, FixedUpdate);
        add_process_schedule(app, FixedPostUpdate);
    }
}
