use super::*;

#[derive(Resource, Default)]
struct ScheduleInvocations(Vec<&'static str>);

fn record_pre_update(In(_process): In<Entity>, mut invocations: ResMut<ScheduleInvocations>) {
    invocations.0.push("pre-update");
}

fn record_update(In(_process): In<Entity>, mut invocations: ResMut<ScheduleInvocations>) {
    invocations.0.push("update");
}

/// Systems registered to different schedules are retained and dispatched in
/// schedule order.
#[test]
fn program_runs_each_registered_schedule() {
    let mut app = get_test_app();
    app.init_resource::<ScheduleInvocations>();
    app.add_program_system(TestProgram, PreUpdate, record_pre_update);
    app.add_program_system(TestProgram, Update, record_update);

    app.add_systems(Startup, |mut commands: Commands| {
        let stdio = commands.spawn_empty().id();
        spawn_process(&mut commands, stdio, TestProgram);
    });

    app.add_step(
        0,
        |invocations: Res<ScheduleInvocations>, mut commands: Commands| {
            if invocations.0.len() < 2 {
                return;
            }

            if commands.assert(
                invocations.0.as_slice() == ["pre-update", "update"],
                format!("expected pre-update then update, got {:?}", invocations.0),
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}
