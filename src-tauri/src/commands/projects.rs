use std::path::PathBuf;

use beacon_core::Snapshot;
use beacon_core::domain::{ProjectId, WorkspaceId};
use tauri::State;

use crate::error::CommandResult;
use crate::state::{AppState, lock};

/// Adds an already-chosen folder. The folder picker itself runs in the
/// frontend via the dialog plugin, so this command stays testable and has no
/// window dependency.
#[tauri::command]
pub fn add_project(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    path: String,
) -> CommandResult<Snapshot> {
    let mut beacon = lock(&state);
    beacon.add_project(&workspace_id, &PathBuf::from(path))?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn rename_project(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    name: String,
) -> CommandResult<Snapshot> {
    let mut beacon = lock(&state);
    beacon.rename_project(&workspace_id, &project_id, &name)?;
    Ok(beacon.snapshot())
}

/// Forgets a project. This does not delete anything from disk — see
/// `docs/DECISIONS.md`, ADR-006.
#[tauri::command]
pub fn remove_project(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<Snapshot> {
    let mut beacon = lock(&state);
    beacon.remove_project(&workspace_id, &project_id)?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn move_project(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    target_workspace_id: WorkspaceId,
) -> CommandResult<Snapshot> {
    let mut beacon = lock(&state);
    beacon.move_project(&workspace_id, &project_id, &target_workspace_id)?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn set_active_project(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<Snapshot> {
    let mut beacon = lock(&state);
    beacon.set_active_project(&workspace_id, &project_id)?;
    Ok(beacon.snapshot())
}
