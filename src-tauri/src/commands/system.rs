use beacon_core::domain::{ProjectId, WorkspaceId};
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// Reveals a project in Finder / the desktop file manager.
#[tauri::command]
pub fn reveal_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<()> {
    let path = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|err| CommandError::from(err.to_string()))
}

/// Which platform we are on, so the frontend can label shortcuts `⌘` or `Ctrl`
/// without sniffing the user agent.
#[tauri::command]
pub fn host_platform() -> &'static str {
    std::env::consts::OS
}
