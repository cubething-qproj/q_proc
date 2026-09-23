use super::*;
use bevy::ecs::intern::Interned;

/// A typed registration resolves to the validated name used as its runtime label.
#[test]
fn program_labels_are_names() {
    let name = ProgramName::new("test-program").unwrap();
    assert_eq!(TestProgram.intern(), name.intern());
    assert_eq!(name.name(), "test-program");
    assert!(ProgramName::new("").is_err());
    assert!(ProgramName::new("two words").is_err());
}

#[test]
#[should_panic(expected = "interned program label must have a valid name")]
fn invalid_interned_label_cannot_become_a_program_name() {
    let label = Interned("two words");
    let _ = ProgramLabel::name(&label);
}

#[test]
fn registered_programs_are_discoverable_by_name() {
    let mut app = get_test_app();
    app.register_program::<TestProgram>();
    app.register_program::<OtherProgram>();

    let programs = app.world().resource::<Programs>();
    assert_eq!(
        programs.get_by_name("test-program"),
        Some(TestProgram.intern())
    );
    assert_eq!(
        programs.get_by_name("other-program"),
        Some(OtherProgram.intern())
    );
    assert_eq!(programs.get_by_name("missing"), None);
    let mut names = programs.names().map(|name| name.name()).collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(names, ["other-program", "test-program"]);
}

#[test]
fn programs_registered_after_plugin_setup_are_discoverable() {
    let mut app = get_test_app();
    app.register_program::<TestProgram>();
    assert_eq!(
        app.world()
            .resource::<Programs>()
            .get_by_name("other-program"),
        None
    );
    app.register_program::<OtherProgram>();
    assert_eq!(
        app.world()
            .resource::<Programs>()
            .get_by_name("other-program"),
        Some(OtherProgram.intern())
    );
}

#[test]
#[should_panic(expected = "already registered to another program")]
fn duplicate_names_are_rejected() {
    #[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
    struct Duplicate;
    q_proc::impl_program_label!(Duplicate, "test-program");

    let mut app = get_test_app();
    app.register_program::<TestProgram>();
    app.register_program::<Duplicate>();
}

#[test]
#[should_panic(expected = "program name must be nonempty")]
fn empty_names_are_rejected() {
    #[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
    struct Empty;
    q_proc::impl_program_label!(Empty, "");

    get_test_app().register_program::<Empty>();
}

#[test]
#[should_panic(expected = "program name must be nonempty")]
fn whitespace_in_names_is_rejected() {
    #[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
    struct Whitespace;
    q_proc::impl_program_label!(Whitespace, "two words");

    get_test_app().register_program::<Whitespace>();
}
