use beacon_core::claude_hooks::{self, HookStatus};
use beacon_core::client::daemon_binary_path;
use beacon_core::requirements::{self, Requirement};

use crate::error::CommandResult;

/// What the machine has, and what is missing.
///
/// Checked on demand rather than cached: someone told that git is missing will
/// go and install it, and the next thing they do is look again.
#[tauri::command]
pub fn check_requirements() -> Vec<Requirement> {
    requirements::check()
}

/// Whether the session daemon was found where it should be.
///
/// Its absence is a packaging fault rather than something the user can install,
/// so it is reported separately and says so.
#[tauri::command]
pub fn daemon_available() -> bool {
    requirements::daemon_present(&daemon_binary_path())
}

/// The command Claude Code would run for each hook.
///
/// The daemon binary in its `hook` mode: nothing extra to ship, and it is
/// already beside the application wherever the application is.
///
/// Quoted, because Claude Code hands hook commands to a shell and the packaged
/// application lives at `/Applications/Beacon Split.app` — a space the shell
/// would otherwise read as the end of the command.
fn hook_command() -> String {
    format!(
        "{} hook",
        claude_hooks::shell_quote(&daemon_binary_path().to_string_lossy())
    )
}

fn status_line_command() -> String {
    format!(
        "{} statusline",
        claude_hooks::shell_quote(&daemon_binary_path().to_string_lossy())
    )
}

/// Everything the Claude Code section needs to describe itself.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    hooks: HookStatus,
    hook_command: String,
    status_line: bool,
    status_line_command: String,
}

#[tauri::command]
pub fn claude_integration() -> CommandResult<Integration> {
    Ok(Integration {
        hooks: claude_hooks::status(std::path::Path::new(&hook_command()))?,
        hook_command: hook_command(),
        status_line: claude_hooks::status_line_installed()?,
        status_line_command: status_line_command(),
    })
}

/// Takes over Claude Code's status line, which is the only place it reports the
/// five-hour allowance and how full the context is. Whatever was there still
/// runs and is still what Claude Code shows.
#[tauri::command]
pub fn install_claude_status_line() -> CommandResult<Integration> {
    claude_hooks::install_status_line(std::path::Path::new(&status_line_command()))?;
    tracing::info!("took over Claude Code's status line");
    claude_integration()
}

#[tauri::command]
pub fn remove_claude_status_line() -> CommandResult<Integration> {
    claude_hooks::remove_status_line()?;
    tracing::info!("gave Claude Code's status line back");
    claude_integration()
}

/// Whether Beacon's hooks are registered with Claude Code, and whether they
/// still point at this build.
#[tauri::command]
pub fn claude_hook_status() -> CommandResult<HookStatus> {
    Ok(claude_hooks::status(std::path::Path::new(&hook_command()))?)
}

/// Registers the hooks in the user's Claude Code settings.
///
/// Explicitly, never on startup: this writes to a file that belongs to another
/// application, and doing that unasked is not Beacon's to decide.
#[tauri::command]
pub fn install_claude_hooks() -> CommandResult<HookStatus> {
    let command = hook_command();
    claude_hooks::install(std::path::Path::new(&command))?;
    tracing::info!("registered Beacon's hooks with Claude Code");
    Ok(claude_hooks::status(std::path::Path::new(&command))?)
}

#[tauri::command]
pub fn remove_claude_hooks() -> CommandResult<HookStatus> {
    claude_hooks::uninstall()?;
    tracing::info!("removed Beacon's hooks from Claude Code");
    Ok(claude_hooks::status(std::path::Path::new(&hook_command()))?)
}

/// The exact command that would be registered, so the user can see what Beacon
/// is asking to add before agreeing to it.
#[tauri::command]
pub fn claude_hook_command() -> String {
    hook_command()
}
