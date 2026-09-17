use crate::prelude::*;

/// Program names preserve valid input and reject embedded whitespace.
#[test]
fn validates_basic_program_names() {
    let name = ProgramName::new("worker").expect("a single-word name should be valid");

    assert_eq!(name.name(), "worker");
    assert!(ProgramName::new("two words").is_err());
}
