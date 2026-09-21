//! Minimal endpoint-neutral program example.
//!
//! Spawns a process with stdout connected to an example-local endpoint and
//! writes bytes once per second through [`ProcessWriteMsg`].

use std::{any::TypeId, time::Duration};

use bevy::{app::ScheduleRunnerPlugin, log::LogPlugin, platform::collections::HashMap, prelude::*};
use q_proc::{impl_program_label, prelude::*};

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
struct MyProgram;

impl_program_label!(MyProgram, "my-program");

#[derive(Component)]
struct LogEndpoint;

impl IoComponent for LogEndpoint {
    type Stdin = ();
    type Stdout = String;
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(100))),
        LogPlugin::default(),
        ProcessPlugin,
    ));
    app.register_io_component::<LogEndpoint>();
    app.program::<MyProgram>().add_system(Update, write_hello);
    app.add_systems(
        Update,
        log_endpoint_writes.after(ProcessSystems::RouteWrites),
    );

    let endpoint = app.world_mut().spawn(LogEndpoint).id();
    let handle = {
        let world = app.world_mut();
        let mut endpoints = world.query_filtered::<(), With<LogEndpoint>>();
        let endpoints = endpoints.query(world);
        world
            .resource::<IoComponentCache>()
            .handle::<LogEndpoint>(endpoint, &endpoints)
            .expect("the registered endpoint component should produce a handle")
    };
    let mut descriptors = ProcessFdTable::default();
    descriptors.set(FileDescriptor::STDOUT, handle);
    app.world_mut().spawn((
        Process {
            prog: MyProgram.intern(),
            signal_overrides: HashMap::new(),
            argv: Vec::new(),
            environ: HashMap::new(),
        },
        descriptors,
    ));

    app.run();
}

fn write_hello(
    In(process): In<Entity>,
    mut writes: MessageWriter<ProcessWriteMsg<String>>,
    mut timer: Local<Option<Timer>>,
    time: Res<Time>,
) {
    let timer = timer.get_or_insert_with(|| Timer::from_seconds(1.0, TimerMode::Repeating));
    timer.tick(time.delta());
    if timer.just_finished() {
        writes.write(ProcessWriteMsg::<String>::stdout(
            process,
            format!("Hello from process {process}!"),
        ));
    }
}

fn log_endpoint_writes(mut writes: MessageReader<EndpointWriteMsg<String>>) {
    for write in writes.read() {
        if write.endpoint().component_type_id() == TypeId::of::<LogEndpoint>() {
            info!("{}", write.payload());
        }
    }
}
