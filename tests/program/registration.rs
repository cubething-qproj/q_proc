use super::*;

/// Explicit registration is safe after systems have already caused the program
/// entry to be created.
#[test]
fn registering_a_program_preserves_existing_systems() {
    let mut app = get_test_app();
    app.add_program_system(TestProgram, Update, record_invocation);
    app.register_program(TestProgram);

    let programs = app.world().resource::<Programs>();
    let data = programs
        .get(TestProgram)
        .expect("the program should remain registered");
    assert_eq!(data.len(), 1, "registration erased the Update system");
}
