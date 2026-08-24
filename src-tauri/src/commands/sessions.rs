use beacon_core::domain::{ProjectId, WorkspaceId};
use beacon_core::session::{SessionId, SessionInfo, SessionKind};
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

/// Returns the project's session of this kind, starting one if needed.
///
/// Called every time a terminal view mounts, which is what makes switching
/// projects cheap: the process is already there.
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
    let id = state
        .sessions
        .ensure(&project_id, kind, &cwd, (cols.max(2), rows.max(2)))?;
    Ok(state.sessions.info(&id)?)
}

/// Sends keystrokes to a session.
///
/// Nothing about the payload is logged — it is whatever the user typed.
#[tauri::command]
pub fn write_session(state: State<'_, AppState>, id: SessionId, data: String) -> CommandResult<()> {
    state.sessions.write(&id, data.as_bytes())?;
    Ok(())
}

#[tauri::command]
pub fn resize_session(
    state: State<'_, AppState>,
    id: SessionId,
    cols: u16,
    rows: u16,
) -> CommandResult<()> {
    state.sessions.resize(&id, cols.max(2), rows.max(2))?;
    Ok(())
}

/// Everything the session has printed so far, base64-encoded, plus the stream
/// offset just past it.
///
/// A terminal view replays this on mount and then ignores any live chunk that
/// ends at or before `end_offset`, so a rebuilt panel looks exactly like the one
/// you left — no gap, no repeated block.
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
    use base64::Engine as _;
    let (bytes, end_offset) = state.sessions.scrollback(&id)?;
    Ok(ScrollbackSnapshot {
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
        end_offset,
    })
}

#[tauri::command]
pub fn close_session(state: State<'_, AppState>, id: SessionId) -> CommandResult<()> {
    state.sessions.close(&id)?;
    Ok(())
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
    let id = state
        .sessions
        .restart_for(&project_id, kind, &cwd, (cols.max(2), rows.max(2)))?;
    Ok(state.sessions.info(&id)?)
}

/// Stops every process belonging to a project, leaving the project itself in
/// the workspace.
#[tauri::command]
pub fn stop_project(state: State<'_, AppState>, project_id: ProjectId) -> CommandResult<()> {
    state.sessions.close_project(&project_id)?;
    Ok(())
}
