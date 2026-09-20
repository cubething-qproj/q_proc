//! Basic data types required for process execution
use std::marker::PhantomData;

use bevy::{
    ecs::{
        define_label,
        intern::Interned,
        lifecycle::HookContext,
        schedule::{InternedScheduleLabel, ScheduleLabel},
        system::SystemId,
        world::DeferredWorld,
    },
    platform::collections::{HashMap, hash_map::Entry},
};

use crate::prelude::*;

/// A [`Resource`] which tracks a registered program through its
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

/// Data associated with a [`ProgramLabel`]. Specifically, [`SystemId`]s mapped to [`ScheduleLabel`]s.
/// Note that this currently only accepts **one** system per label.
// TODO: Store schedules rather than individual systems so a program phase can
// contain multiple ordered systems.
pub type ProgramData = HashMap<InternedScheduleLabel, SystemId<In<Entity>, ()>>;

/// Type alias for a [`System`] associated with a [`ProgramLabel`].
/// The input is an [`Entity`] pointer to the live [`Process`].
pub type ProgramSystem = SystemId<In<Entity>, ()>;

pub trait IntoProgramSystem<M>: IntoSystem<In<Entity>, (), M> + 'static {}
impl<T, M> IntoProgramSystem<M> for T where T: IntoSystem<In<Entity>, (), M> + 'static {}

define_label!(
    /// Label for a program, analogous to [`ScheduleLabel`].
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
#[component(immutable, on_remove = Process::on_remove)]
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

impl Process {
    fn on_remove(mut world: DeferredWorld, context: HookContext) {
        if let Some(descriptors) = world.get::<ProcessFdTable>(context.entity).cloned()
            && let Some(mut closing) = world.get_resource_mut::<ClosingProcessIo>()
        {
            closing.insert(context.entity, descriptors);
        }

        world.commands().queue(move |world: &mut World| {
            if let Ok(mut process) = world.get_entity_mut(context.entity) {
                process.remove::<(ProcessFdTable, ProcessInputBuffer)>();
            }
        });
    }
}

/// The name of a program. This type exists to ensure validity on construction.
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

/// Registration options for one program type.
pub struct AppProgramOpts<'a, T: ProgramLabel + Default> {
    app: &'a mut App,
    marker: PhantomData<T>,
}

impl<T: ProgramLabel + Default> AppProgramOpts<'_, T> {
    /// Registers one system in a host schedule for this program.
    pub fn add_system<M, S: ScheduleLabel + Clone>(
        &mut self,
        schedule: S,
        system: impl IntoProgramSystem<M>,
    ) -> &mut Self {
        crate::plugins::add_process_schedule(self.app, schedule.clone());
        let id = self.app.register_system(system);
        let program = T::default();
        trace!("Registered program system for {program:?}");
        let mut programs = self.app.world_mut().resource_mut::<Programs>();
        programs
            .entry(program)
            .or_default()
            .insert(schedule.intern(), id);
        trace!("Programs: {programs:#?}");
        self
    }
}

/// Adds and configures program types on an [`App`].
pub trait ProgramAppExt {
    /// Registers `T` without changing systems already configured for it.
    fn register_program<T: ProgramLabel + Default>(&mut self) -> &mut Self;

    /// Returns the system-registration options for `T`.
    fn program<T: ProgramLabel + Default>(&mut self) -> AppProgramOpts<'_, T>;
}

impl ProgramAppExt for App {
    fn register_program<T: ProgramLabel + Default>(&mut self) -> &mut Self {
        self.world_mut().init_resource::<Programs>();
        let program = T::default();
        trace!("Registered program {program:?}");
        let mut programs = self.world_mut().resource_mut::<Programs>();
        programs.entry(program).or_default();
        trace!("Programs: {programs:#?}");
        self
    }

    fn program<T: ProgramLabel + Default>(&mut self) -> AppProgramOpts<'_, T> {
        self.register_program::<T>();
        AppProgramOpts {
            app: self,
            marker: PhantomData,
        }
    }
}
