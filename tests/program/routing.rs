use super::*;

#[derive(Resource, Default)]
struct RoutedInvocations(Vec<(&'static str, Entity)>);

#[derive(Resource)]
struct ExpectedRoutes {
    test: Entity,
    other: Entity,
}

fn record_test(In(process): In<Entity>, mut invocations: ResMut<RoutedInvocations>) {
    invocations.0.push(("test", process));
}

fn record_other(In(process): In<Entity>, mut invocations: ResMut<RoutedInvocations>) {
    invocations.0.push(("other", process));
}

#[derive(Resource)]
struct Victim(Entity);

fn remove_victim(
    In(process): In<Entity>,
    victim: Res<Victim>,
    mut commands: Commands,
    mut invocations: ResMut<RoutedInvocations>,
) {
    invocations.0.push(("test", process));
    commands.entity(victim.0).remove::<Process>();
}

/// Processes dispatch only through the system registered for their own label.
#[test]
fn program_labels_route_to_their_own_systems() {
    let mut app = get_test_app();
    app.init_resource::<RoutedInvocations>();
    app.program::<TestProgram>().add_system(Update, record_test);
    app.program::<OtherProgram>()
        .add_system(Update, record_other);

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

#[test]
fn queued_invocation_skips_a_process_removed_by_an_earlier_invocation() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.init_resource::<RoutedInvocations>();
    app.program::<TestProgram>()
        .add_system(Update, remove_victim);
    app.program::<OtherProgram>()
        .add_system(Update, record_other);

    let killer = app
        .world_mut()
        .spawn(Process {
            prog: TestProgram.intern(),
            signal_overrides: HashMap::new(),
            argv: Vec::new(),
            environ: HashMap::new(),
        })
        .id();
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

    assert_eq!(
        app.world().resource::<RoutedInvocations>().0,
        [("test", killer)]
    );
    assert!(!app.world().entity(victim).contains::<Process>());
}
