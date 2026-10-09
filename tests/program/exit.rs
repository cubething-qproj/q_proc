use super::*;

#[derive(Resource, Default)]
struct Exits(Vec<(Entity, ExitStatus)>);

struct Fixture {
    app: App,
    process: Entity,
}

impl Fixture {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins(ProcessPlugin);
        app.register_program::<TestProgram>();
        app.init_resource::<Exits>();
        app.add_observer(|exited: On<ProcessExited>, mut exits: ResMut<Exits>| {
            exits.0.push((exited.entity, exited.status));
        });
        let process = app
            .world_mut()
            .spawn(Process {
                prog: TestProgram.intern(),
                signal_overrides: HashMap::new(),
                argv: Vec::new(),
                environ: HashMap::new(),
            })
            .id();
        Self { app, process }
    }

    fn queue_exit(&mut self, code: i32) {
        self.app
            .world_mut()
            .commands()
            .entity(self.process)
            .exit(code);
    }

    fn flush(&mut self) {
        self.app.world_mut().flush();
    }

    fn assert_running(&self) {
        let process = self.app.world().entity(self.process);
        assert!(process.contains::<Process>());
        assert!(!process.contains::<ExitStatus>());
        assert!(self.app.world().resource::<Exits>().0.is_empty());
    }

    fn assert_ended(&self, status: ExitStatus) {
        assert!(self.app.world().get_entity(self.process).is_err());
        assert_eq!(
            self.app.world().resource::<Exits>().0,
            [(self.process, status)]
        );
    }
}

#[test]
fn immediate_exit_reports_status_and_despawns_before_returning() {
    let mut fixture = Fixture::new();

    fixture.app.world_mut().entity_mut(fixture.process).exit(3);

    fixture.assert_ended(ExitStatus::Code(3));
}

#[test]
fn deferred_exits_apply_once_with_the_first_code() {
    let mut fixture = Fixture::new();

    fixture.queue_exit(3);
    fixture.queue_exit(7);
    fixture.assert_running();
    fixture.flush();

    fixture.assert_ended(ExitStatus::Code(3));
}

#[test]
fn queued_exit_tolerates_an_earlier_despawn() {
    let mut fixture = Fixture::new();

    fixture
        .app
        .world_mut()
        .commands()
        .entity(fixture.process)
        .try_despawn();
    fixture.queue_exit(3);
    fixture.flush();

    fixture.assert_ended(ExitStatus::Terminated);
}

#[test]
fn exit_leaves_non_process_entities_untouched() {
    let mut fixture = Fixture::new();
    let unrelated = fixture.app.world_mut().spawn_empty().id();

    fixture.app.world_mut().entity_mut(unrelated).exit(3);
    fixture.app.world_mut().commands().entity(unrelated).exit(7);
    fixture.flush();

    let unrelated = fixture.app.world().entity(unrelated);
    assert!(!unrelated.contains::<ExitStatus>());
    fixture.assert_running();
}
