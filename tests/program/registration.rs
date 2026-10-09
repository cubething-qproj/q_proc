use super::*;

#[test]
fn adding_systems_registers_the_program_name() {
    let mut app = get_test_app();
    app.program::<TestProgram>()
        .add_systems(Update, record_invocation);

    let programs = app.world().resource::<Programs>();
    assert_eq!(
        programs.get_by_name("test-program"),
        Some(TestProgram.intern())
    );
}
