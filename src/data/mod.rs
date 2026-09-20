//! Data types for process management.

pub mod endpoints;
pub mod io;
pub mod process;
pub mod signals;

pub mod prelude {
    pub use super::endpoints::*;
    pub use super::io::*;
    pub use super::process::*;
    pub use super::signals::*;
}
