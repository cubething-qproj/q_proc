use bevy::platform::collections::HashMap;
use q_term::prelude::{
    TermInfo, TermStdOut, TermWrite, Terminal, TerminalPlugin, VtForegroundProcess, VtLine, VtSize,
};

use super::*;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct TerminalProgram;

q_proc::impl_program_label!(TerminalProgram, "terminal-program");

fn write_once(
    In(process_id): In<Entity>,
    processes: Query<&Process>,
    mut stdout: MessageWriter<TermStdOut>,
    mut wrote: Local<bool>,
) {
    if *wrote {
        return;
    }

    let process = processes
        .get(process_id)
        .expect("the dispatched process should still exist");
    stdout.write(TermStdOut {
        term: process.fd1,
        from: process_id,
        message: vec![TermWrite::new("Hello from q_proc!")],
    });
    *wrote = true;
}

/// A q_proc program can write through q_term into the terminal buffer.
#[test]
fn program_output_reaches_the_terminal_buffer() {
    let mut app = get_test_app();
    app.add_plugins(TerminalPlugin);
    app.add_program_system(TerminalProgram, Update, write_once);

    app.add_systems(Startup, |mut commands: Commands| {
        let terminal = commands
            .spawn((Terminal, VtSize { cols: 80, rows: 24 }))
            .id();
        let process = commands
            .spawn((
                Process {
                    prog: TerminalProgram.intern(),
                    signal_overrides: HashMap::new(),
                    argv: Vec::new(),
                    environ: HashMap::new(),
                    fd0: terminal,
                    fd1: terminal,
                    fd2: terminal,
                },
                VtForegroundProcess::new(terminal),
            ))
            .id();
        commands.entity(process).insert(Name::new("test process"));
    });

    app.add_step(
        0,
        |terminals: Query<TermInfo>, lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            let Ok(terminal) = terminals.single() else {
                return;
            };
            let text = terminal
                .lines(&lines)
                .map(|(_, line)| line.as_string())
                .collect::<String>();
            if text.is_empty() {
                return;
            }

            if commands.assert(
                text == "Hello from q_proc!",
                format!("expected terminal output, got {text:?}"),
            ) {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}
