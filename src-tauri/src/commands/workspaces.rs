use beacon_core::Snapshot;
use beacon_core::domain::WorkspaceId;
use beacon_core::ui_state::PanelLayout;
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    state.beacon().snapshot()
}

#[tauri::command]
pub fn create_workspace(
    state: State<'_, AppState>,
    name: String,
    accent: String,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.create_workspace(&name, &accent)?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn update_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
    name: Option<String>,
    accent: Option<String>,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.update_workspace(&id, name.as_deref(), accent.as_deref(), None)?;
    Ok(beacon.snapshot())
}

/// Removes a workspace from Beacon. Project folders are never touched.
#[tauri::command]
pub fn delete_workspace(state: State<'_, AppState>, id: WorkspaceId) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.delete_workspace(&id)?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn set_active_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_active_workspace(&id)?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn set_panels(state: State<'_, AppState>, panels: PanelLayout) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_panels(panels)?;
    Ok(beacon.snapshot())
}
