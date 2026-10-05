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

use crate::error::{CommandError, CommandResult};

/// Runs work that can take a while on the blocking pool, so the IPC worker
/// that asked is free to answer keystrokes meanwhile.
pub(crate) async fn run_off_thread<T, F>(work: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> beacon_core::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| CommandError::from(err.to_string()))?
        .map_err(CommandError::from)
}
