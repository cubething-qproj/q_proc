//! Data types for process management.

pub mod process;
pub mod signals;
pub mod stdio;

pub mod prelude {
    pub use super::process::*;
    pub use super::signals::*;
    pub use super::stdio::*;
}
