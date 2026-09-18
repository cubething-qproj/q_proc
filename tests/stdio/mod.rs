use crate::prelude::*;

mod terminal;

#[derive(Resource)]
struct ExpectedTarget(Entity);

/// Stdout and stderr retain their targets and payloads on distinct message
/// channels.
#[test]
fn output_channels_round_trip_independently() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn_empty().id();
        commands.insert_resource(ExpectedTarget(target));
        commands.write_message(StdOut::new(
            target,
            vec!["first".to_string(), "second".to_string()],
        ));
        commands.write_message(StdErr::write(target, "error"));
    });
    app.add_step(
        0,
        |expected: Res<ExpectedTarget>,
         mut stdout: MessageReader<StdOut>,
         mut stderr: MessageReader<StdErr>,
         mut commands: Commands| {
            let stdout = stdout.read().collect::<Vec<_>>();
            let stderr = stderr.read().collect::<Vec<_>>();
            if stdout.is_empty() || stderr.is_empty() {
                return;
            }

            let correct = stdout.len() == 1
                && stdout[0].term == expected.0
                && stdout[0].writes == ["first", "second"]
                && stderr.len() == 1
                && stderr[0].term == expected.0
                && stderr[0].writes == ["error"];
            if commands.assert(
                correct,
                format!("unexpected stdout {stdout:?} or stderr {stderr:?}"),
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}
