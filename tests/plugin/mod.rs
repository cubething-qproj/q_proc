use crate::prelude::*;

/// `ProcessPlugin` owns the resources required by its scheduled systems, so an
/// app with no registered programs can still run normally.
#[test]
fn runs_without_registered_programs() {
    let mut app = get_test_app();
    app.add_step(0, |mut commands: Commands| {
        commands.write_message(AppExit::Success);
    });

    assert!(app.run().is_success());
}
