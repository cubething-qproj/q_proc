---
name: q-proc
description: Deep dive on q_proc (lib/q_proc), the Bevy process model — Process entities, ProgramLabel derive and program registration, per-invocation state via required components, exit/ProcessExited lifecycle, file descriptor tables, IoComponent endpoints, typed I/O lanes (ProcessWriteMsg/EndpointWriteMsg/ProcessInputMsg), pipes and tees, and ProcessSystems ordering. Use when writing a program, an I/O endpoint, or debugging process lifecycle/output routing.
---

# q_proc

Source: `lib/q_proc/src` (~1.2k lines; read it). Normative lifecycle doc:
`lib/q_proc/docs/process-lifecycle.md`. Example: `examples/program.rs`.
Import everything via `use q_proc::prelude::*;`.

## Mental model

- **Program** = a marker component type `T: ProgramLabel + Component +
  Default` plus ordinary Bevy systems that query `With<T>`.
- **Process** = one entity with the immutable `Process` component (one
  invocation). Inserting `Process` inserts the program's marker `T`
  (and therefore its `#[require]`d state). The entity *is* the process.
- One system advances **all** invocations of its program. Put per-invocation
  state in components required by the marker; `Local<T>` is shared across
  invocations.

## Writing a program

```rust
#[derive(Component, Default, Debug, ProgramLabel)]
#[program_label("countdown")]      // nonempty, no whitespace; checked at compile time
#[require(Ticks)]                  // per-invocation state
struct Countdown;

#[derive(Component, Default)]
struct Ticks(u32);

fn run_countdown(mut q: Query<(Entity, &mut Ticks, &Process), With<Countdown>>,
                 mut out: MessageWriter<ProcessWriteMsg<Vec<u8>>>,
                 mut commands: Commands) {
    for (e, mut t, proc) in &mut q {        // proc.argv excludes the program name
        t.0 += 1;
        out.write(ProcessWriteMsg::stdout(e, format!("{}\r\n", t.0).into_bytes()));
        if t.0 >= 3 { commands.entity(e).exit(0); }
    }
}

app.add_plugins(ProcessPlugin);
app.program::<Countdown>().add_systems(Update, run_countdown);
// app.register_program::<T>() registers without adding systems.
```

Spawning directly (q_shell does this for you):

```rust
let mut fds = ProcessFdTable::default();
fds.set(FileDescriptor::STDOUT, handle);   // IoHandle, see Endpoints
world.spawn((Process { prog: Countdown.intern(), argv, environ, signal_overrides: default() }, fds));
```

Reading input: `ProcessInputBuffer<T>` component, a
`HashMap<FileDescriptor, VecDeque<Arc<T>>>`; `pop_front()` on
`FileDescriptor::STDIN`.

## Lifecycle rules

- `commands.entity(e).exit(code)` / `world.entity_mut(e).exit(code)`: sets a
  temporary `ExitStatus::Code`, despawns. No-op if no `Process` or already
  gone, so repeated exits are safe.
- Despawning or removing `Process` without `exit` → `ExitStatus::Terminated`.
- `ProcessExited { entity, status }` (an `EntityEvent`) is triggered **once**,
  from `Process`'s `on_remove` hook. Observers run synchronously and can read
  the dying entity's components/relationships; their queued commands run after
  despawn, so use `try_*` commands against the child. Store results on the
  owner — there are no zombies.
- Removing `Process` ends the invocation permanently (Cleanup despawns it even
  if re-inserted). **Replacing** `Process` (insert over it) switches program
  and resets marker/state with no exit event.
- Marker removal uses `remove_with_requires::<T>()`: **do not `#[require]`
  shared components** (e.g. `Name`) — they get stripped too.
- Registering after spawning: earlier processes never get the marker
  (a warning `runs unregistered program` is logged if unregistered at insert).
- Duplicate program names across different types panic at registration.

## Scheduling

`ProcessSystems`: `QueueInput` (in `First`), `RunPrograms`, `RouteWrites`,
`Cleanup`. `ProcessPlugin` installs `RunPrograms → ApplyDeferred →
RouteWrites → Cleanup` in PreUpdate, Update, PostUpdate and the three Fixed
update schedules. `program::<T>().add_systems(CustomSchedule, …)` installs
the chain in that schedule too. To observe routed output, order your system
`.after(ProcessSystems::RouteWrites)`.

Input written as `ProcessInputMsg` in frame N is buffered in frame N+1's
`First`.

## Endpoints & I/O lanes

- `IoMessage`: payload marker trait (`()`, `String`, `Vec<u8>` built in;
  `impl IoMessage for MyPayload {}`). Each `T` is an independent, ordered lane;
  no ordering across lanes.
- `IoComponent`: a component that makes an entity an endpoint, with
  `type Stdin` (endpoint → process) and `type Stdout` (process → endpoint).
  Register with `app.register_io_component::<C>()` (also registers both lanes;
  `app.register_io_msg::<T>()` for a lane alone). Re-registering with
  different lanes panics.
- `IoHandle` = (entity, component type). Get one with
  `IoComponentCache::handle::<C>(entity, &Query<(), With<C>>)`; it's `None`
  unless `C` is registered and present.
- `ProcessFdTable`: `get/set/close/duplicate(from, to)`; `FileDescriptor::
  {STDIN, STDOUT, STDERR}` or `FileDescriptor::new(n)`.
- Flow: program writes `ProcessWriteMsg<T>{process, fd}` → `RouteWrites`
  resolves fd → `EndpointWriteMsg<T>{process, fd, endpoint, payload}`; your
  endpoint system reads those filtered by
  `endpoint().component_type_id() == TypeId::of::<C>()`. Endpoints feed
  processes by writing `ProcessInputMsg<T>{process, fd, endpoint}`; it's
  dropped unless that fd currently maps to that endpoint.
- Writes are dropped (with `warn!`) for: missing process/fd, closed endpoint
  (component removed), lane mismatch (`C::Stdout != T`).
- Removing an endpoint component closes every fd pointing at it (live and
  cached tables) and removes it from tees.
- `PipeEndpoint<T>::new(process, fd)`: forwards writes to another process's
  input. No capacity/backpressure/EOF yet.
- `TeeEndpoint<T>::new([handles])`: fan-out in order; nested tees unsupported
  (dropped with a warning). Payloads are `Arc`-shared, not cloned.

## Not implemented yet

`Sig`/`SignalMsg` are declared and the message is registered, but nothing
consumes signals and `Process::signal_overrides` is unused.

## Tests

`lib/q_proc/tests/{io,program}/` cover routing, endpoints, lifecycle, exit,
naming, schedules — read the closest one before adding a test. Run
`just test -p q_proc` from the org root.
