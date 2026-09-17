//! Basic data types required for process execution
use bevy::{
    ecs::{
        define_label,
        intern::Interned,
        schedule::{InternedScheduleLabel, ScheduleLabel},
        system::SystemId,
    },
    platform::collections::{HashMap, hash_map::Entry},
};

use crate::prelude::*;

/// A [`Resource`] which tracks a registered [`Program`] through its
/// [`ProgramLabel`].
#[derive(Resource, Default, Deref, DerefMut, Debug)]
pub struct Programs(pub(crate) HashMap<InternedProgramLabel, ProgramData>);
impl Programs {
    pub fn get(&self, label: impl ProgramLabel) -> Option<&ProgramData> {
        self.0.get(&label.intern())
    }
    pub fn get_mut(&mut self, label: impl ProgramLabel) -> Option<&mut ProgramData> {
        self.0.get_mut(&label.intern())
    }
    pub fn contains(&self, label: impl ProgramLabel) -> bool {
        self.0.contains_key(&label.intern())
    }
    pub fn entry(
        &mut self,
        label: impl ProgramLabel,
    ) -> Entry<'_, InternedProgramLabel, ProgramData> {
        self.0.entry(label.intern())
    }
}

/// Data associated with a [`Program`]. Specifically, [`SystemId`]s mapped to [`ScheduleLabel`]s.
/// Note that this currently only accepts **one** system per label.
// TODO: Store schedules rather than individual systems so a program phase can
// contain multiple ordered systems.
pub type ProgramData = HashMap<InternedScheduleLabel, SystemId<In<Entity>, ()>>;

/// Type alias for a [`System`] associated with a [`Program`].
/// The input is an [`Entity`] pointer to the live [`Process`].
pub type ProgramSystem = SystemId<In<Entity>, ()>;

pub trait IntoProgramSystem<M>: IntoSystem<In<Entity>, (), M> + 'static {}
impl<T, M> IntoProgramSystem<M> for T where T: IntoSystem<In<Entity>, (), M> + 'static {}

define_label!(
    /// Label for a [`Program`] analagous to [`ScheduleLabel`]
    ProgramLabel,
    PROGRAM_LABEL_INTERNER,
    extra_methods: {
        /// Name of the program, used to run it on the command line.
        fn name(&self) -> ProgramName;
    },
    extra_methods_impl: {
        /// Name of the program, used to run it on the command line.
        fn name(&self) -> ProgramName {
            ProgramName::new("PLACEHOLDER").unwrap()
        }
    }
);

/// Shorthand for [`Interned<dyn ProgramLabel>`]
pub type InternedProgramLabel = Interned<dyn ProgramLabel>;

/// A [`Program`] is a set of instructions which is instantiated by spawning a
/// [`Process`]. Where the [`Process`] is the [`Component`], this is the [`System`]
/// manager.
pub trait Program {
    /// Signal overriding behavior.
    /// Returns a HashMap from the signal to its override command.
    /// By default, SIGINT, SIGQUIT, SIGTERM, and SIGHUP all despawn the entity.
    /// Use [`ProcessSignalOverride`] to define the schedule.
    fn trap(&self, _kind: Sig) -> Option<SystemId> {
        None
    }
}

// TODO: Derive macro for ProgramLabel
#[macro_export]
macro_rules! impl_program_label {
    ($t:ty, $name:literal) => {
        impl ProgramLabel for $t {
            fn name(&self) -> ProgramName {
                ProgramName::new($name).unwrap()
            }
            fn dyn_clone(&self) -> Box<dyn ProgramLabel> {
                Box::new(self.clone())
            }
        }
    };
}

/// A [`Process`] is a set of [`System`]s which is managed by a [`Shell`].
/// Processes should be run by running [`Shell::spawn_process`]. The process
/// dies when this component is removed. Lifecycle hooks are a good way to
/// implement de/initialization behaviors.
// TODO: Piping? Need file descriptors if so. Probably a relationship (ProcessFd<const CHANNEL: u8)
#[derive(Component, Clone, Debug)]
#[component(immutable)]
pub struct Process {
    /// The [`ProgramLabel`] associated with this [`Process`].
    /// Determines what this process _does_.
    pub prog: InternedProgramLabel,
    /// Signal catching behavior
    pub signal_overrides: HashMap<Sig, SystemId>,
    /// Argument values.
    pub argv: Vec<String>,
    /// Environment variables.
    pub environ: HashMap<String, String>,
    /// stdin
    pub fd0: Entity,
    /// stdout
    pub fd1: Entity,
    /// stderr
    pub fd2: Entity,
}
/// The name of a [`Program`]. This type exists to ensure validity on construction.
/// In particular, program names must not contain whitespace.
#[derive(Debug, Deref)]
pub struct ProgramName(&'static str);
impl ProgramName {
    pub fn new(name: &'static str) -> Result<Self, &'static str> {
        if name.split_whitespace().count() > 1 {
            Err("Program name must not contain whitespace.")
        } else {
            Ok(Self(name))
        }
    }
    pub fn name(&self) -> &'static str {
        self.0
    }
}

pub trait ProgramAppExt {
    fn register_program(&mut self, prog: impl ProgramLabel);
    // NOTE: This is similar to a ScreenScope
    fn add_program_system<M>(
        &mut self,
        prog: impl ProgramLabel + Clone,
        schedule: impl ScheduleLabel,
        system: impl IntoProgramSystem<M>,
    );
}
impl ProgramAppExt for App {
    fn register_program(&mut self, prog: impl ProgramLabel) {
        self.world_mut().init_resource::<Programs>();
        let mut progs = self.world_mut().resource_mut::<Programs>();
        progs.0.entry(prog.intern()).or_default();
        trace!("Registered program {:?}", prog,);
        trace!("Programs: {:#?}", progs)
    }
    fn add_program_system<M>(
        &mut self,
        prog: impl ProgramLabel + Clone,
        schedule: impl ScheduleLabel,
        system: impl IntoProgramSystem<M>,
    ) {
        self.init_resource::<Programs>();
        let id = self.register_system(system);
        let mut progs = self.world_mut().resource_mut::<Programs>();
        let data = progs.entry(prog.clone()).or_default();
        data.insert(schedule.intern(), id);
        trace!("Registered program system for {:?}", prog,);
        trace!("Programs: {:#?}", progs)
    }
}
