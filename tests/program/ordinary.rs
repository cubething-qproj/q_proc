use super::*;

/// Counts frames, then exits once it has counted to `argv[0]`.
#[derive(Component, Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
#[require(Ticks)]
struct Countdown;

q_proc::impl_program_label!(Countdown, "countdown");

/// Per-invocation program state.
#[derive(Component, Default)]
struct Ticks(u32);

/// `(process, ticks)` recorded as each invocation exits.
#[derive(Resource, Default)]
struct Exits(Vec<(Entity, u32)>);

#[derive(Resource)]
struct Expected([(Entity, u32); 2]);

fn run_countdown(
    mut invocations: Query<(Entity, &Process, &mut Ticks), With<Countdown>>,
    mut exits: ResMut<Exits>,
    mut commands: Commands,
) {
    for (process, info, mut ticks) in &mut invocations {
        ticks.0 += 1;
        if info.argv[0].parse() == Ok(ticks.0) {
            exits.0.push((process, ticks.0));
            commands.entity(process).remove::<Process>();
        }
    }
}

/// One ordinary system advances every invocation, each with its own state,
/// and an exited invocation leaves the program with its state.
#[test]
fn invocations_keep_independent_state() {
    let mut app = get_test_app();
    app.init_resource::<Exits>();
    app.program::<Countdown>().add_systems(Update, run_countdown);

    app.add_systems(Startup, |mut commands: Commands| {
        let mut spawn = |count: u32| {
            let process = commands
                .spawn(Process {
                    prog: Countdown.intern(),
                    signal_overrides: HashMap::new(),
                    argv: vec![count.to_string()],
                    environ: HashMap::new(),
                })
                .id();
            (process, count)
        };
        let expected = Expected([spawn(2), spawn(4)]);
        commands.insert_resource(expected);
    });

    app.add_step(
        0,
        |expected: Res<Expected>,
         exits: Res<Exits>,
         leftovers: Query<(), Or<(With<Countdown>, With<Ticks>)>>,
         mut commands: Commands| {
            if exits.0.len() < 2 {
                return;
            }
            if commands.assert(
                exits.0 == expected.0,
                format!("expected exits {:?}, got {:?}", expected.0, exits.0),
            ) && commands.assert(
                leftovers.is_empty(),
                "exited invocations kept their marker or state",
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}

/// A program with state that does nothing.
#[derive(Component, Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
#[require(IdleState)]
struct Idle;

q_proc::impl_program_label!(Idle, "idle");

#[derive(Component, Default)]
struct IdleState;

fn process(program: impl ProgramLabel, argv: &[&str]) -> Process {
    Process {
        prog: program.intern(),
        signal_overrides: HashMap::new(),
        argv: argv.iter().map(|arg| arg.to_string()).collect(),
        environ: HashMap::new(),
    }
}

/// Replacing a process with another program swaps the marker and state.
#[test]
fn replacing_a_process_swaps_its_program() {
    #[derive(Resource)]
    struct Replaced(Entity);

    let mut app = get_test_app();
    app.init_resource::<Exits>();
    app.program::<Countdown>().add_systems(Update, run_countdown);
    app.program::<Idle>().add_systems(Update, || {});

    app.add_systems(Startup, |mut commands: Commands| {
        let entity = commands.spawn(process(Countdown, &["1000"])).id();
        commands.insert_resource(Replaced(entity));
    });

    app.add_step(
        0,
        |replaced: Res<Replaced>,
         counting: Query<(), With<Ticks>>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            if counting.contains(replaced.0) {
                commands.entity(replaced.0).insert(process(Idle, &[]));
                next.set(Step(1));
            }
        },
    );
    app.add_step(
        1,
        |replaced: Res<Replaced>,
         entities: Query<(Has<Countdown>, Has<Ticks>, Has<Idle>, Has<IdleState>)>,
         mut commands: Commands| {
            let found = r!(entities.get(replaced.0));
            if commands.assert(
                found == (false, false, true, true),
                format!("expected only Idle and its state, got {found:?}"),
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}
