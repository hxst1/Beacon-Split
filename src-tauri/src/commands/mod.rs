//! The IPC surface.
//!
//! Commands are intentionally thin: they translate arguments, call into
//! `beacon-core`, and return a fresh [`Snapshot`]. Every mutation returning the
//! whole state keeps the frontend from having to reconcile partial updates.

mod clips;
mod files;
mod git;
mod integration;
mod notifications;
mod projects;
mod sessions;
mod system;
mod workspaces;
mod workstreams;

pub use clips::*;
pub use files::*;
pub use git::*;
pub use integration::*;
pub use notifications::*;
pub use projects::*;
pub use sessions::*;
pub use system::*;
pub use workspaces::*;
pub use workstreams::*;
