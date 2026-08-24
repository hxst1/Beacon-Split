//! The IPC surface.
//!
//! Commands are intentionally thin: they translate arguments, call into
//! `beacon-core`, and return a fresh [`Snapshot`]. Every mutation returning the
//! whole state keeps the frontend from having to reconcile partial updates.

mod files;
mod projects;
mod sessions;
mod system;
mod workspaces;

pub use files::*;
pub use projects::*;
pub use sessions::*;
pub use system::*;
pub use workspaces::*;
