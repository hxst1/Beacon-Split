use beacon_core::domain::{ProjectId, WorkspaceId};
use beacon_core::session::{SessionId, SessionInfo, SessionKind, SessionPrefs};
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
    slot: u32,
    cols: u16,
    rows: u16,
) -> CommandResult<SessionInfo> {
    let (cwd, prefs) = {
        let beacon = state.beacon();
        (
            beacon.resolve_project_path(&workspace_id, &project_id)?,
            SessionPrefs {
                shell: beacon.shell(),
                agents: beacon.claude_agents(),
            },
        )
    };
    Ok(state.daemon()?.ensure(
        &project_id,
        kind,
        slot,
        &cwd,
        (cols.max(2), rows.max(2)),
        prefs,
    )?)
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
    slot: u32,
    cols: u16,
    rows: u16,
) -> CommandResult<SessionInfo> {
    let (cwd, prefs) = {
        let beacon = state.beacon();
        (
            beacon.resolve_project_path(&workspace_id, &project_id)?,
            SessionPrefs {
                shell: beacon.shell(),
                agents: beacon.claude_agents(),
            },
        )
    };
    Ok(state.daemon()?.restart(
        &project_id,
        kind,
        slot,
        &cwd,
        (cols.max(2), rows.max(2)),
        prefs,
    )?)
}

/// Stops every process belonging to a project, leaving the project itself in
/// the workspace.
#[tauri::command]
pub fn stop_project(state: State<'_, AppState>, project_id: ProjectId) -> CommandResult<()> {
    Ok(state.daemon()?.close_project(&project_id)?)
}

/// Ends one of a project's sessions.
///
/// Addressed by project and slot rather than by session id: closing a terminal
/// tab is something the window knows about, and the id belongs to the daemon.
/// Found through the session list rather than a new message — the daemon
/// already knows how to answer that question.
#[tauri::command]
pub fn stop_session_slot(
    state: State<'_, AppState>,
    project_id: ProjectId,
    slot: u32,
) -> CommandResult<()> {
    let daemon = state.daemon()?;
    let target = daemon
        .list()?
        .into_iter()
        .find(|info| info.project == project_id && info.slot == slot);

    // Already gone is the outcome asked for, not a failure.
    if let Some(info) = target {
        daemon.close(&info.id)?;
    }
    Ok(())
}

/// Which sessions are running, including any started before this window opened.
#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> CommandResult<Vec<SessionInfo>> {
    Ok(state.daemon()?.list()?)
}

/// What each project's Claude session last reported it was costing.
#[tauri::command]
pub fn session_usage(
    state: State<'_, AppState>,
) -> CommandResult<Vec<beacon_core::protocol::UsageReport>> {
    Ok(state.daemon()?.usage()?)
}

/// Stops the daemon, and with it every session.
///
/// The one way to deliberately end work that is meant to outlive the window.
#[tauri::command]
pub fn stop_daemon(state: State<'_, AppState>) -> CommandResult<()> {
    Ok(state.daemon()?.shutdown()?)
}
