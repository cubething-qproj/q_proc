use super::*;
use std::{fmt::Debug, marker::PhantomData};

#[derive(Component, Default, Debug, ProgramLabel)]
#[program_label("derived")]
#[require(DerivedState)]
struct DerivedProgram;

#[derive(Component, Default)]
struct DerivedState;

#[derive(Debug, q_proc::ProgramLabel)]
#[program_label("generic")]
struct GenericProgram<T>(PhantomData<T>)
where
    T: Debug + Send + Sync + 'static;

#[test]
fn derived_program_registers_and_initializes_required_state() {
    let mut app = App::new();
    app.add_plugins(ProcessPlugin);
    app.register_program::<DerivedProgram>();
    let label = app
        .world()
        .resource::<Programs>()
        .get_by_name("derived")
        .unwrap();

    let process = app.world_mut().spawn(Process {
        prog: label,
        signal_overrides: HashMap::new(),
        argv: Vec::new(),
        environ: HashMap::new(),
    });
    assert!(process.contains::<DerivedProgram>());
    assert!(process.contains::<DerivedState>());
}

#[test]
fn derive_preserves_generic_parameters_and_where_bounds() {
    let program = GenericProgram::<u32>(PhantomData);
    assert_eq!(program.name().name(), "generic");
    assert_eq!(
        program.intern(),
        ProgramName::new("generic").unwrap().intern()
    );
}
