use beacon_core::domain::{ProjectId, WorkspaceId};
use beacon_core::dotenv::{self, EnvEntry};
use beacon_core::files::{self, DirEntry, FileContents};
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

/// Every file command takes the project it belongs to and a path relative to
/// it. Nothing accepts an absolute path, so a path can only ever name something
/// inside a project the user added.
macro_rules! project_root {
    ($state:expr, $workspace:expr, $project:expr) => {
        $state
            .beacon()
            .resolve_project_path(&$workspace, &$project)?
    };
}

#[tauri::command]
pub fn list_dir(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<Vec<DirEntry>> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::list_dir(&root, &path)?)
}

#[tauri::command]
pub fn read_file(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<FileContents> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::read_file(&root, &path)?)
}

#[tauri::command]
pub fn write_file(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
    text: String,
) -> CommandResult<()> {
    let root = project_root!(state, workspace_id, project_id);
    // Never log `text`: this is the user's file, and one of them is `.env`.
    Ok(files::write_file(&root, &path, &text)?)
}

#[tauri::command]
pub fn create_file(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<()> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::create_file(&root, &path)?)
}

#[tauri::command]
pub fn create_dir(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<()> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::create_dir(&root, &path)?)
}

#[tauri::command]
pub fn rename_path(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    from: String,
    to: String,
) -> CommandResult<()> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::rename(&root, &from, &to)?)
}

#[tauri::command]
pub fn duplicate_path(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<String> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::duplicate(&root, &path)?)
}

#[tauri::command]
pub fn copy_into(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    source: String,
    target_dir: String,
) -> CommandResult<String> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::copy_into(&root, &source, &target_dir)?)
}

/// Moves an entry to the system trash — recoverable from the user's own file
/// manager. Beacon has no operation that deletes outright.
#[tauri::command]
pub fn trash_path(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<()> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::move_to_trash(&root, &path)?)
}

/// Reveals a file or folder in Finder / the desktop file manager.
#[tauri::command]
pub fn reveal_path(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<()> {
    use tauri_plugin_opener::OpenerExt;

    let root = project_root!(state, workspace_id, project_id);
    let target = beacon_core::files::resolve_within(&root, &path)?;
    app.opener()
        .reveal_item_in_dir(&target)
        .map_err(|err| crate::error::CommandError::from(err.to_string()))
}

/// Every file in a project, for quick open.
///
/// Listed on demand rather than kept in memory: a project's file list changes
/// under us constantly, and a stale one is worse than a fresh read.
#[tauri::command]
pub fn list_project_files(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<Vec<String>> {
    let root = project_root!(state, workspace_id, project_id);
    Ok(files::list_project_files(&root)?)
}

/// Reads a `.env` file into its assignments.
///
/// Read fresh on every call and never cached: the file is the only place these
/// values live, and nothing on this path logs them.
#[tauri::command]
pub fn read_env_file(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<Vec<EnvEntry>> {
    let root = project_root!(state, workspace_id, project_id);
    match files::read_file(&root, &path)? {
        FileContents::Text { text } => Ok(dotenv::parse(&text)),
        _ => Ok(Vec::new()),
    }
}
