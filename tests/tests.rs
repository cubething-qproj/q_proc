mod plugin;
mod program;
mod stdio;

pub mod prelude {
    pub use super::get_test_app;
    pub use bevy::prelude::*;
    pub use q_proc::prelude::*;
    pub use q_test_harness::prelude::*;
}

use prelude::*;

/// Creates a minimal headless Bevy app for q_proc integration tests.
pub fn get_test_app() -> App {
    let mut app = App::new();
    app.add_plugins((TestRunnerPlugin::default(), ProcessPlugin));
    app.insert_resource(TestRunnerTimeout(2.));
    app
}
