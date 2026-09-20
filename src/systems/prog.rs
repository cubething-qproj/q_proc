//! Systems for programs.

use crate::prelude::*;
use bevy::ecs::schedule::InternedScheduleLabel;

pub(crate) fn run_programs(
    schedule: InternedScheduleLabel,
    mut commands: Commands,
    processes: Query<(Entity, &Process)>,
    programs: Res<Programs>,
) {
    for (entity, process) in &processes {
        trace!("{schedule:?}: Running {:?}", process.prog.name());
        let program = c!(programs.0.get(&process.prog));
        let system = *cq!(program.get(&schedule));
        let expected_program = process.prog;
        commands.queue(move |world: &mut World| {
            let still_running = world
                .get::<Process>(entity)
                .is_some_and(|process| process.prog == expected_program);
            if still_running && let Err(error) = world.run_system_with(system, entity) {
                warn!("Could not run program for process {entity:?}: {error}");
            }
        });
    }
}
