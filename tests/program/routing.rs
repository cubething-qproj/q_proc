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

/// Processes dispatch only through the system registered for their own label.
#[test]
fn program_labels_route_to_their_own_systems() {
    let mut app = get_test_app();
    app.init_resource::<RoutedInvocations>();
    app.program::<TestProgram>().add_system(Update, record_test);
    app.program::<OtherProgram>()
        .add_system(Update, record_other);

    app.add_systems(Startup, |mut commands: Commands| {
        let stdio = commands.spawn_empty().id();
        let test = spawn_process(&mut commands, stdio, TestProgram);
        let other = spawn_process(&mut commands, stdio, OtherProgram);
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
