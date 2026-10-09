//! Basic data types required for process execution
use std::{any::TypeId, borrow::Borrow, marker::PhantomData};

use bevy::{
    ecs::{
        intern::{Interned, Interner},
        lifecycle::HookContext,
        schedule::ScheduleLabel,
        system::{ScheduleSystem, SystemId},
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
    marker: ProgramMarker,
}

/// Inserts and removes the marker component that selects a program's
/// invocations for its ordinary systems.
#[derive(Clone, Copy, Debug)]
struct ProgramMarker {
    insert: fn(&mut EntityWorldMut),
    remove: fn(&mut EntityWorldMut),
}

/// Which half of a [`ProgramMarker`] a [`Process`] hook applies.
#[derive(Clone, Copy, Debug)]
enum MarkerChange {
    Insert,
    Remove,
}

impl ProgramMarker {
    fn apply(self, change: MarkerChange, entity: &mut EntityWorldMut) {
        match change {
            MarkerChange::Insert => (self.insert)(entity),
            MarkerChange::Remove => (self.remove)(entity),
        }
    }

    fn of<T: Component + Default>() -> Self {
        Self {
            insert: |entity| {
                entity.insert(T::default());
            },
            // Program state is the marker's required components.
            remove: |entity| {
                entity.remove_with_requires::<T>();
            },
        }
    }
}

impl Programs {
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
    fn marker(&self, label: InternedProgramLabel) -> Option<ProgramMarker> {
        Some(self.0.get(&label.name())?.marker)
    }
    fn register<T: ProgramLabel + Component + Default>(&mut self, program: T) {
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
            }
            Entry::Vacant(entry) => {
                entry.insert(RegisteredProgram {
                    owner,
                    marker: ProgramMarker::of::<T>(),
                });
            }
        }
    }
}

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
///
/// Derive this trait with one explicit `#[program_label("name")]` attribute.
/// Names must be nonempty and contain no whitespace. The derive validates the
/// literal at compile time; runtime registration still enforces uniqueness.
/// A registered marker also needs `Component`, `Default`, and `Debug`.
///
/// ```
/// use q_proc::prelude::*;
///
/// #[derive(Component, Default, Debug, ProgramLabel)]
/// #[program_label("sleep")]
/// struct Sleep;
///
/// assert_eq!(Sleep.name().name(), "sleep");
/// ```
///
/// A missing name is a compile error:
///
/// ```compile_fail
/// use q_proc::prelude::*;
/// #[derive(Debug, ProgramLabel)]
/// struct MissingName;
/// ```
///
/// Empty names are also rejected:
///
/// ```compile_fail
/// use q_proc::prelude::*;
/// #[derive(Debug, ProgramLabel)]
/// #[program_label("")]
/// struct EmptyName;
/// ```
///
/// Whitespace is rejected before registration:
///
/// ```compile_fail
/// use q_proc::prelude::*;
/// #[derive(Debug, ProgramLabel)]
/// #[program_label("two words")]
/// struct InvalidName;
/// ```
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

/// A [`Process`] is one running instance of a registered [`ProgramLabel`].
///
/// The entity is the process. Use [`ProcessExitExt::exit`] to complete it with a
/// code, or despawn it to terminate without an explicit status. [`ProcessExited`]
/// is triggered during removal, and cached descriptors route pending writes
/// after despawn.
///
/// Removing this component also ends the invocation permanently: cleanup
/// despawns the entity after routing, even if `Process` is reinserted. Replace
/// the component directly to switch programs without ending the invocation.
///
/// A process carries its program's marker component for as long as it runs.
#[derive(Component, Clone, Debug)]
#[component(
    immutable,
    on_insert = Process::on_insert,
    on_discard = Process::on_discard,
    on_remove = Process::on_remove
)]
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
    fn on_insert(world: DeferredWorld, context: HookContext) {
        Self::change_marker(world, context.entity, MarkerChange::Insert);
    }

    /// Runs before every removal, replacement, and despawn, so a replaced
    /// process's old marker is removed before the new one is inserted.
    fn on_discard(world: DeferredWorld, context: HookContext) {
        Self::change_marker(world, context.entity, MarkerChange::Remove);
    }

    /// Queues `change` to `entity`'s program marker.
    fn change_marker(mut world: DeferredWorld, entity: Entity, change: MarkerChange) {
        let Some(prog) = world.get::<Process>(entity).map(|process| process.prog) else {
            return;
        };
        let Some(marker) = world
            .get_resource::<Programs>()
            .and_then(|programs| programs.marker(prog))
        else {
            if matches!(change, MarkerChange::Insert) {
                warn!("Process {entity} runs unregistered program {:?}", prog.name().name());
            }
            return;
        };
        world.commands().queue(move |world: &mut World| {
            if let Ok(mut entity) = world.get_entity_mut(entity) {
                marker.apply(change, &mut entity);
            }
        });
    }

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

        let status = world
            .get::<ExitStatus>(context.entity)
            .copied()
            .unwrap_or(ExitStatus::Terminated);
        world.trigger(ProcessExited {
            entity: context.entity,
            status,
        });
    }
}

/// How a [`Process`] ended.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitStatus {
    /// The program completed with this code through [`ProcessExitExt::exit`].
    Code(i32),
    /// The process was removed or despawned without an explicit status.
    Terminated,
}

/// Triggered on a process's entity when the process ends, before the entity is
/// despawned. Owners observe it to learn the [`ExitStatus`].
///
/// Observers run synchronously inside the removal hook and can read the entity's
/// components and relationships. Queued commands run after removal; on despawn
/// the entity is already gone. Store the status on the owner, and use `try_*`
/// commands when targeting the ended process. Its descriptor table is also
/// removed before queued observer commands run.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ProcessExited {
    /// The entity of the ended invocation.
    pub entity: Entity,
    /// The explicit status, or [`ExitStatus::Terminated`] if none was supplied.
    pub status: ExitStatus,
}

/// Completes a process through deferred or immediate entity access.
pub trait ProcessExitExt {
    /// Sets the temporary exit status and despawns the process in one operation.
    ///
    /// Does nothing if the entity has no [`Process`]. With [`EntityCommands`],
    /// the operation is deferred and also ignores an entity that disappeared
    /// before command application. With [`EntityWorldMut`], it runs immediately.
    /// The removal hook remains the sole source of [`ProcessExited`].
    fn exit(self, code: i32);
}

impl ProcessExitExt for EntityWorldMut<'_> {
    fn exit(mut self, code: i32) {
        if !self.contains::<Process>() {
            return;
        }
        self.insert(ExitStatus::Code(code));
        self.despawn();
    }
}

impl ProcessExitExt for EntityCommands<'_> {
    fn exit(mut self, code: i32) {
        self.queue_silenced(move |process: EntityWorldMut| process.exit(code));
    }
}

/// Registration options for one program type.
pub struct AppProgramOpts<'a, T: ProgramLabel + Component + Default> {
    app: &'a mut App,
    marker: PhantomData<T>,
}

impl<T: ProgramLabel + Component + Default> AppProgramOpts<'_, T> {
    /// Adds ordinary systems for this program to `schedule`'s
    /// [`ProcessSystems::RunPrograms`] set, installing process I/O routing in
    /// `schedule` if needed.
    ///
    /// Each running invocation carries a `T` marker, so systems select their
    /// invocations with `With<T>`. Per-invocation state belongs in components
    /// that `T` requires.
    ///
    /// Requirements:
    /// - Register the program before spawning its processes. The marker is
    ///   inserted when a [`Process`] is inserted, so earlier processes never
    ///   receive it.
    /// - When the process is removed, replaced, or despawned, `T` is removed
    ///   *with its required components*. Require only program-owned state:
    ///   requiring a shared component, such as `Name`, removes it from the
    ///   entity too.
    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel + Clone,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        crate::plugins::add_process_schedule(self.app, schedule.clone());
        self.app
            .add_systems(schedule, systems.in_set(ProcessSystems::RunPrograms));
        self
    }
}

/// Adds and configures program types on an [`App`].
pub trait ProgramAppExt {
    /// Registers `T` without changing systems already configured for it.
    fn register_program<T: ProgramLabel + Component + Default>(&mut self) -> &mut Self;

    /// Registers `T` and returns its system-registration options.
    fn program<T: ProgramLabel + Component + Default>(&mut self) -> AppProgramOpts<'_, T>;
}

impl ProgramAppExt for App {
    fn register_program<T: ProgramLabel + Component + Default>(&mut self) -> &mut Self {
        self.world_mut().init_resource::<Programs>();
        let program = T::default();
        trace!("Registered program {program:?}");
        let mut programs = self.world_mut().resource_mut::<Programs>();
        programs.register(program);
        trace!("Programs: {programs:#?}");
        self
    }

    fn program<T: ProgramLabel + Component + Default>(&mut self) -> AppProgramOpts<'_, T> {
        self.register_program::<T>();
        AppProgramOpts {
            app: self,
            marker: PhantomData,
        }
    }
}
