use beacon_core::claude_hooks::{self, HookStatus};
use beacon_core::client::daemon_binary_path;

use crate::error::CommandResult;

/// The command Claude Code would run for each hook.
///
/// The daemon binary in its `hook` mode: nothing extra to ship, and it is
/// already beside the application wherever the application is.
fn hook_command() -> String {
    format!("{} hook", daemon_binary_path().display())
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
