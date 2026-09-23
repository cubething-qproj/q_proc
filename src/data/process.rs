//! Basic data types required for process execution
use std::{any::TypeId, borrow::Borrow, marker::PhantomData};

use bevy::{
    ecs::{
        intern::{Interned, Interner},
        lifecycle::HookContext,
        schedule::{InternedScheduleLabel, ScheduleLabel},
        system::SystemId,
        world::DeferredWorld,
    },
    platform::collections::{HashMap, hash_map::Entry},
};

use crate::prelude::*;

/// Registered programs indexed by their validated command name.
#[derive(Resource, Default, Debug)]
pub struct Programs(HashMap<ProgramName, RegisteredProgram>);

#[derive(Debug)]
struct RegisteredProgram {
    owner: TypeId,
    systems: ProgramData,
}

impl Programs {
    pub fn get(&self, label: impl ProgramLabel) -> Option<&ProgramData> {
        self.0.get(&label.name()).map(|program| &program.systems)
    }
    pub fn get_mut(&mut self, label: impl ProgramLabel) -> Option<&mut ProgramData> {
        self.0
            .get_mut(&label.name())
            .map(|program| &mut program.systems)
    }
    pub fn contains(&self, label: impl ProgramLabel) -> bool {
        self.0.contains_key(&label.name())
    }
    /// Returns a registered program's interned name.
    pub fn get_by_name(&self, name: &str) -> Option<InternedProgramLabel> {
        self.0.get_key_value(name).map(|(label, _)| label.intern())
    }
    /// Returns the registered program names in unspecified order.
    pub fn names(&self) -> impl Iterator<Item = ProgramName> + '_ {
        self.0.keys().copied()
    }
    fn register<T: ProgramLabel + 'static>(&mut self, program: T) -> &mut ProgramData {
        let name = program.name();
        let owner = TypeId::of::<T>();
        match self.0.entry(name) {
            Entry::Occupied(entry) => {
                assert_eq!(
                    entry.get().owner,
                    owner,
                    "program name {:?} is already registered to another program",
                    name.name()
                );
                &mut entry.into_mut().systems
            }
            Entry::Vacant(entry) => {
                &mut entry
                    .insert(RegisteredProgram {
                        owner,
                        systems: ProgramData::default(),
                    })
                    .systems
            }
        }
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

/// A program's runtime identity and command name.
#[derive(Clone, Copy, Debug, Deref, Eq, Hash, PartialEq)]
pub struct ProgramName(&'static str);
impl ProgramName {
    /// Constructs a name suitable for a single command token.
    pub fn new(name: &'static str) -> Result<Self, &'static str> {
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            Err("Program name must be nonempty and contain no whitespace.")
        } else {
            Ok(Self(name))
        }
    }
    pub fn name(&self) -> &'static str {
        self.0
    }
}
impl Borrow<str> for ProgramName {
    fn borrow(&self) -> &str {
        self.0
    }
}

/// The runtime label is the interned program name, not a separate type identity.
pub type InternedProgramLabel = Interned<str>;

static PROGRAM_NAME_INTERNER: Interner<str> = Interner::new();

/// Supplies a program name for typed application registration.
pub trait ProgramLabel: Send + Sync + std::fmt::Debug + 'static {
    fn name(&self) -> ProgramName;
    fn intern(&self) -> InternedProgramLabel {
        PROGRAM_NAME_INTERNER.intern(self.name().name())
    }
}
impl ProgramLabel for ProgramName {
    fn name(&self) -> ProgramName {
        *self
    }
}
impl ProgramLabel for InternedProgramLabel {
    fn name(&self) -> ProgramName {
        ProgramName::new(self.0).expect("interned program label must have a valid name")
    }
    fn intern(&self) -> InternedProgramLabel {
        *self
    }
}

#[macro_export]
macro_rules! impl_program_label {
    ($t:ty, $name:literal) => {
        impl $crate::prelude::ProgramLabel for $t {
            fn name(&self) -> $crate::prelude::ProgramName {
                $crate::prelude::ProgramName::new($name)
                    .expect("program name must be nonempty and contain no whitespace")
            }
        }
    };
}

/// A [`Process`] is one running instance of a registered [`ProgramLabel`].
/// The process dies when this component is removed.
#[derive(Component, Clone, Debug)]
#[component(immutable, on_remove = Process::on_remove)]
#[require(ProcessFdTable)]
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
                process.remove::<ProcessFdTable>();
            }
        });
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
        programs.register(program).insert(schedule.intern(), id);
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
        programs.register(program);
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
