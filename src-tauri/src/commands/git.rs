use beacon_core::domain::{ProjectId, WorkspaceId};
use beacon_core::git::{self, GitStatus};
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::state::AppState;

/// What the Git panel shows: branch, tracking position, and changed paths.
///
/// Returns `None` when the project is not a repository, which is a normal state
/// rather than an error to report.
///
/// Off-thread like the rest of this file. The panel asks for this every two
/// seconds without being told to, and every git command here is allowed to
/// take its timeout before giving up — an IPC worker held for that long is one
/// that is not answering keystrokes.
#[tauri::command]
pub async fn git_status(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<Option<GitStatus>> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    run_off_thread(move || {
        if !git::is_repository(&root) {
            return Ok(None);
        }
        git::status(&root).map(Some)
    })
    .await
}

#[tauri::command]
pub async fn git_diff(
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
    run_off_thread(move || git::diff(&root, &path, staged, untracked)).await
}

#[tauri::command]
pub async fn git_stage(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    run_off_thread(move || {
        git::stage(&root, &path)?;
        git::status(&root)
    })
    .await
}

#[tauri::command]
pub async fn git_unstage(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    path: String,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    run_off_thread(move || {
        git::unstage(&root, &path)?;
        git::status(&root)
    })
    .await
}

#[tauri::command]
pub async fn git_stage_all(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    run_off_thread(move || {
        git::stage_all(&root)?;
        git::status(&root)
    })
    .await
}

/// Commits, which is as slow as the repository's own hooks make it.
///
/// A pre-commit hook is somebody's whole test suite, so this belongs on the
/// blocking pool for the same reason push and pull do: an IPC worker held for
/// two minutes is one that is not answering anything else.
#[tauri::command]
pub async fn git_commit(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    message: String,
) -> CommandResult<GitStatus> {
    let root = state
        .beacon()
        .resolve_project_path(&workspace_id, &project_id)?;
    // The message is the user's; nothing here logs it.
    run_off_thread(move || {
        git::commit(&root, &message)?;
        git::status(&root)
    })
    .await
}

/// Push and pull talk to a network and can take as long as they like.
///
/// They run on the blocking pool rather than on an IPC worker, so a slow remote
/// cannot stall the commands that keep the window responsive. Git itself is
/// configured never to stop and ask for a password — there is no terminal here
/// for it to ask on, and it would simply hang — and stops waiting for anything
/// that will not finish, so a control the user cannot use never stays that way.
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

async fn run_off_thread<T, F>(work: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> beacon_core::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| CommandError::from(err.to_string()))?
        .map_err(CommandError::from)
}
