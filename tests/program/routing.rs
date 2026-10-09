use super::*;

#[derive(Resource, Default)]
struct RoutedInvocations(Vec<(&'static str, Entity)>);

#[derive(Resource)]
struct ExpectedRoutes {
    test: Entity,
    other: Entity,
}

fn record_test(
    processes: Query<Entity, With<TestProgram>>,
    mut invocations: ResMut<RoutedInvocations>,
) {
    invocations.0.extend(processes.iter().map(|process| ("test", process)));
}

fn record_other(
    processes: Query<Entity, With<OtherProgram>>,
    mut invocations: ResMut<RoutedInvocations>,
) {
    invocations.0.extend(processes.iter().map(|process| ("other", process)));
}

#[derive(Resource)]
struct Victim(Entity);

fn remove_victim(victim: Res<Victim>, mut commands: Commands) {
    commands.entity(victim.0).remove::<Process>();
}

/// Each program's systems run only for that program's processes.
#[test]
fn program_labels_route_to_their_own_systems() {
    let mut app = get_test_app();
    app.init_resource::<RoutedInvocations>();
    app.program::<TestProgram>().add_systems(Update, record_test);
    app.program::<OtherProgram>()
        .add_systems(Update, record_other);

    app.add_systems(Startup, |mut commands: Commands| {
        let test = spawn_process(&mut commands, TestProgram);
        let other = spawn_process(&mut commands, OtherProgram);
        commands.insert_resource(ExpectedRoutes { test, other });
    });

    app.add_step(
        0,
        |expected: Res<ExpectedRoutes>,
         invocations: Res<RoutedInvocations>,
         mut commands: Commands| {
            if invocations.0.len() < 2 {
                return;
            }

            let routed_correctly = invocations.0.len() == 2
                && invocations.0.contains(&("test", expected.test))
                && invocations.0.contains(&("other", expected.other));
            if commands.assert(
                routed_correctly,
                format!(
                    "expected test->{:?} and other->{:?}, got {:?}",
                    expected.test, expected.other, invocations.0
                ),
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}

/// A process removed by another program stops running once the removal is
/// applied, at the end of `RunPrograms`.
#[test]
fn removed_process_stops_running() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.init_resource::<RoutedInvocations>();
    app.program::<TestProgram>()
        .add_systems(Update, remove_victim);
    app.program::<OtherProgram>()
        .add_systems(Update, record_other);

    app.world_mut().spawn(Process {
        prog: TestProgram.intern(),
        signal_overrides: HashMap::new(),
        argv: Vec::new(),
        environ: HashMap::new(),
    });
    let victim = app
        .world_mut()
        .spawn(Process {
            prog: OtherProgram.intern(),
            signal_overrides: HashMap::new(),
            argv: Vec::new(),
            environ: HashMap::new(),
        })
        .id();
    app.insert_resource(Victim(victim));

    app.world_mut().run_schedule(Update);
    app.world_mut().run_schedule(Update);

    assert_eq!(
        app.world().resource::<RoutedInvocations>().0,
        [("other", victim)]
    );
    let victim = app.world().entity(victim);
    assert!(!victim.contains::<Process>());
    assert!(!victim.contains::<OtherProgram>());
}
