use beacon_core::domain::{ProjectId, WorkspaceId};
use beacon_core::session::{SessionId, SessionInfo, SessionKind};
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

/// Returns the project's session of this kind, starting one if needed.
///
/// Called every time a terminal view mounts. Because sessions live in the
/// daemon, this is also what reattaches to work that was running before the
/// window was last closed.
#[tauri::command]
pub fn open_session(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    kind: SessionKind,
    cols: u16,
    rows: u16,
) -> CommandResult<SessionInfo> {
    let cwd = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    Ok(state
        .daemon()?
        .ensure(&project_id, kind, &cwd, (cols.max(2), rows.max(2)))?)
}

/// Sends keystrokes to a session. Nothing about the payload is logged.
#[tauri::command]
pub fn write_session(state: State<'_, AppState>, id: SessionId, data: String) -> CommandResult<()> {
    Ok(state.daemon()?.write(&id, &data)?)
}

#[tauri::command]
pub fn resize_session(
    state: State<'_, AppState>,
    id: SessionId,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    Ok(state.daemon()?.resize(&id, cols.max(2), rows.max(2))?)
}

/// Everything the session has printed so far, base64-encoded, plus the stream
/// offset just past it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrollbackSnapshot {
    data: String,
    end_offset: u64,
}

#[tauri::command]
pub fn session_scrollback(
    state: State<'_, AppState>,
    id: SessionId,
) -> CommandResult<ScrollbackSnapshot> {
    let (data, end_offset) = state.daemon()?.scrollback(&id)?;
    Ok(ScrollbackSnapshot { data, end_offset })
}

#[tauri::command]
pub fn close_session(state: State<'_, AppState>, id: SessionId) -> CommandResult<()> {
    Ok(state.daemon()?.close(&id)?)
}

/// Gives a project a fresh session of this kind, replacing any it had.
#[tauri::command]
pub fn restart_session(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    kind: SessionKind,
    cols: u16,
    rows: u16,
) -> CommandResult<SessionInfo> {
    let cwd = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    Ok(state
        .daemon()?
        .restart(&project_id, kind, &cwd, (cols.max(2), rows.max(2)))?)
}

/// Stops every process belonging to a project, leaving the project itself in
/// the workspace.
#[tauri::command]
pub fn stop_project(state: State<'_, AppState>, project_id: ProjectId) -> CommandResult<()> {
    Ok(state.daemon()?.close_project(&project_id)?)
}

/// Which sessions are running, including any started before this window opened.
#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> CommandResult<Vec<SessionInfo>> {
    Ok(state.daemon()?.list()?)
}

/// Stops the daemon, and with it every session.
///
/// The one way to deliberately end work that is meant to outlive the window.
#[tauri::command]
pub fn stop_daemon(state: State<'_, AppState>) -> CommandResult<()> {
    Ok(state.daemon()?.shutdown()?)
}
