//! The primary [`Plugin`] for q_proc.

use crate::prelude::*;

macro_rules! impl_run_progs {
    ($app:ident, $($sched:ident),+) => {
        $(
            $app.add_systems($sched, run_programs::<$sched>);
        )+
    };
}

/// Registers process-management messages and schedules `run_programs`
/// across every standard schedule.
#[derive(Debug)]
pub struct ProcessPlugin;
impl Plugin for ProcessPlugin {
    fn build(&self, app: &mut App) {
        use crate::systems::prog::*;
        app.init_resource::<Programs>();
        app.add_message::<SignalMsg>();
        app.add_message::<StdOut>();
        app.add_message::<StdErr>();

        impl_run_progs!(
            app,
            PreUpdate,
            Update,
            PostUpdate,
            FixedPreUpdate,
            FixedUpdate,
            FixedPostUpdate
        );
    }
}
