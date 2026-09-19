//! Data types for process management.

pub mod io;
pub mod process;
pub mod signals;
pub mod stdio;

pub mod prelude {
    pub use super::io::*;
    pub use super::process::*;
    pub use super::signals::*;
    pub use super::stdio::*;
}
