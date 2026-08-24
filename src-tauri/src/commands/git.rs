use beacon_core::domain::{ProjectId, WorkspaceId};
use beacon_core::git::{self, GitStatus};
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// What the Git panel shows: branch, tracking position, and changed paths.
///
/// Returns `None` when the project is not a repository, which is a normal state
/// rather than an error to report.
#[tauri::command]
pub fn git_status(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<Option<GitStatus>> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    if !git::is_repository(&root) {
        return Ok(None);
    }
    Ok(Some(git::status(&root)?))
}

#[tauri::command]
pub fn git_diff(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
    staged: bool,
    untracked: bool,
) -> CommandResult<String> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    Ok(git::diff(&root, &path, staged, untracked)?)
}

#[tauri::command]
pub fn git_stage(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    git::stage(&root, &path)?;
    Ok(git::status(&root)?)
}

#[tauri::command]
pub fn git_unstage(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    git::unstage(&root, &path)?;
    Ok(git::status(&root)?)
}

#[tauri::command]
pub fn git_stage_all(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    git::stage_all(&root)?;
    Ok(git::status(&root)?)
}

#[tauri::command]
pub fn git_commit(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    message: String,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    // The message is the user's; nothing here logs it.
    git::commit(&root, &message)?;
    Ok(git::status(&root)?)
}

/// Push and pull talk to a network and can take as long as they like.
///
/// They run on the blocking pool rather than on an IPC worker, so a slow remote
/// cannot stall the commands that keep the window responsive. Git itself is
/// configured never to stop and ask for a password — there is no terminal here
/// for it to ask on, and it would simply hang.
#[tauri::command]
pub async fn git_push(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<String> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    run_off_thread(move || git::push(&root)).await
}

#[tauri::command]
pub async fn git_pull(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<String> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    run_off_thread(move || git::pull(&root)).await
}

async fn run_off_thread<F>(work: F) -> CommandResult<String>
where
    F: FnOnce() -> beacon_core::Result<String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| CommandError::from(err.to_string()))?
        .map_err(CommandError::from)
}
