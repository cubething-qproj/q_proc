//! Systems for programs.

use crate::prelude::*;
use bevy::ecs::schedule::ScheduleLabel;

pub fn run_programs<S: ScheduleLabel + Default>(
    mut commands: Commands,
    q_procs: Query<(Entity, &Process)>,
    progs: Res<Programs>,
) {
    for (entity, proc) in q_procs.iter() {
        trace!("{:?}: Running {:?}", S::default(), proc.prog.name());
        let pdata = c!(progs.0.get(&proc.prog));
        let sysid = cq!(pdata.get(&S::default().intern()));
        commands.run_system_with(*sysid, entity);
    }
}
