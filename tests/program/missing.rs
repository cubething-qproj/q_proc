use super::*;

/// Processes with an unknown program or no system in the current schedule are
/// ignored rather than preventing the app from running.
#[test]
fn skips_processes_without_registered_systems() {
    let mut app = get_test_app();
    app.register_program(TestProgram);

    app.add_systems(Startup, |mut commands: Commands| {
        let stdio = commands.spawn_empty().id();
        spawn_process(&mut commands, stdio, TestProgram);
        spawn_process(&mut commands, stdio, OtherProgram);
    });
    app.add_step(0, |mut commands: Commands| {
        commands.write_message(AppExit::Success);
    });

    assert!(app.run().is_success());
}
