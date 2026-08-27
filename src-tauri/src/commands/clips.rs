//! The clip drawer: reading it, and forgetting things in it.
//!
//! There is no command to *add* a clip, and that is deliberate. Clips come from
//! Claude through the MCP server and the daemon; the window only ever displays
//! what it is told and asks for things to be removed. Giving the frontend a way
//! to write one would create a second writer for a file whose whole design
//! rests on there being one.

use beacon_core::clips::Clip;
use beacon_core::domain::ClipId;
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

/// Everything in the drawer, newest first.
///
/// Asked for when the window opens and again whenever it reattaches: the daemon
/// outlives the window, so there is usually something waiting.
#[tauri::command]
pub fn session_clips(state: State<'_, AppState>) -> CommandResult<Vec<Clip>> {
    Ok(state.daemon()?.clips()?)
}

/// Forgets one clip, or the whole drawer when `id` is absent.
///
/// Returns what is left. Every other window hears about it through
/// `clips:replaced`, so two open windows cannot disagree about what is there.
#[tauri::command]
pub fn forget_clips(state: State<'_, AppState>, id: Option<ClipId>) -> CommandResult<Vec<Clip>> {
    Ok(state.daemon()?.forget_clips(id)?)
}
