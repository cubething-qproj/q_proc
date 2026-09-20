use bevy::platform::collections::HashMap;

use crate::prelude::*;

mod missing;
mod name;
mod registration;
mod routing;
mod schedules;

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
struct TestProgram;

q_proc::impl_program_label!(TestProgram, "test-program");

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
struct OtherProgram;

q_proc::impl_program_label!(OtherProgram, "other-program");

#[derive(Resource, Default)]
struct Invocations(Vec<Entity>);

#[derive(Resource)]
struct ExpectedProcess(Entity);

fn record_invocation(In(process): In<Entity>, mut invocations: ResMut<Invocations>) {
    invocations.0.push(process);
}

fn spawn_process(commands: &mut Commands, stdio: Entity, program: impl ProgramLabel) -> Entity {
    commands
        .spawn(Process {
            prog: program.intern(),
            signal_overrides: HashMap::new(),
            argv: Vec::new(),
            environ: HashMap::new(),
            fd0: stdio,
            fd1: stdio,
            fd2: stdio,
        })
        .id()
}

/// A program system registered for `Update` runs with the matching process
/// entity as its input.
#[test]
fn update_system_runs_for_matching_process() {
    let mut app = get_test_app();
    app.init_resource::<Invocations>();
    app.program::<TestProgram>()
        .add_system(Update, record_invocation);

    app.add_systems(Startup, |mut commands: Commands| {
        let stdio = commands.spawn_empty().id();
        let process = spawn_process(&mut commands, stdio, TestProgram);
        commands.insert_resource(ExpectedProcess(process));
    });

    app.add_step(
        0,
        |expected: Res<ExpectedProcess>, invocations: Res<Invocations>, mut commands: Commands| {
            if invocations.0.is_empty() {
                return;
            }

            if commands.assert(
                invocations.0.as_slice() == [expected.0],
                format!(
                    "expected one invocation for {:?}, got {:?}",
                    expected.0, invocations.0
                ),
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}

/// Every process carrying a registered program label is dispatched separately.
#[test]
fn update_system_runs_for_each_matching_process() {
    #[derive(Resource)]
    struct ExpectedProcesses([Entity; 2]);

    let mut app = get_test_app();
    app.init_resource::<Invocations>();
    app.program::<TestProgram>()
        .add_system(Update, record_invocation);

    app.add_systems(Startup, |mut commands: Commands| {
        let stdio = commands.spawn_empty().id();
        let processes = [
            spawn_process(&mut commands, stdio, TestProgram),
            spawn_process(&mut commands, stdio, TestProgram),
        ];
        commands.insert_resource(ExpectedProcesses(processes));
    });

    app.add_step(
        0,
        |expected: Res<ExpectedProcesses>,
         invocations: Res<Invocations>,
         mut commands: Commands| {
            if invocations.0.len() < expected.0.len() {
                return;
            }

            let each_process_ran_once = invocations.0.len() == expected.0.len()
                && expected.0.iter().all(|entity| {
                    invocations
                        .0
                        .iter()
                        .filter(|actual| *actual == entity)
                        .count()
                        == 1
                });
            if commands.assert(
                each_process_ran_once,
                format!(
                    "expected one invocation for each of {:?}, got {:?}",
                    expected.0, invocations.0
                ),
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}
