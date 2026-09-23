use super::*;

#[test]
fn adding_a_system_registers_the_program_name() {
    let mut app = get_test_app();
    app.program::<TestProgram>()
        .add_system(Update, record_invocation);

    let programs = app.world().resource::<Programs>();
    assert_eq!(
        programs.get_by_name("test-program"),
        Some(TestProgram.intern())
    );
}

/// Explicit registration is safe after systems have already caused the program
/// entry to be created.
#[test]
fn registering_a_program_preserves_existing_systems() {
    let mut app = get_test_app();
    app.program::<TestProgram>()
        .add_system(Update, record_invocation);
    app.register_program::<TestProgram>();

    let programs = app.world().resource::<Programs>();
    let data = programs
        .get(TestProgram)
        .expect("the program should remain registered");
    assert_eq!(data.len(), 1, "registration erased the Update system");
    assert_eq!(
        programs.get_by_name("test-program"),
        Some(TestProgram.intern())
    );
    assert_eq!(
        programs.names().map(|name| name.name()).collect::<Vec<_>>(),
        ["test-program"]
    );
}
