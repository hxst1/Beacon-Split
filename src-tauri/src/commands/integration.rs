use std::path::PathBuf;

use tauri::State;

use beacon_core::agent::AgentKind;
use beacon_core::claude::{self, Capabilities};
use beacon_core::claude_hooks::{self, HookStatus};
use beacon_core::client::daemon_binary_path;
use beacon_core::codex;
use beacon_core::codex_plugin::{self, PluginStatus};
use beacon_core::requirements::{self, Requirement};

use super::run_off_thread;
use crate::error::CommandResult;
use crate::state::AppState;

/// Every place an agent's configuration lives on this machine.
///
/// The machine's own account and every account that was added. What Beacon
/// installs into another application's configuration goes into all of them:
/// hooks only report where they are registered, so an account without them is
/// an account where the amber tab, "waiting for you" and resuming a
/// conversation all quietly stop working (ADR-099).
fn homes(state: &State<'_, AppState>, agent: AgentKind) -> Vec<PathBuf> {
    state.beacon().accounts().every_home(agent)
}

/// Claude Code's user settings file in each of them.
fn claude_settings(state: &State<'_, AppState>) -> Vec<PathBuf> {
    homes(state, AgentKind::Claude)
        .into_iter()
        .map(|home| home.join("settings.json"))
        .collect()
}

/// The worst of what the accounts say, because the reader is deciding whether
/// to press one button.
///
/// An installation that is only partly there has to read as not there: the fix
/// is the same — install, which goes everywhere — and "installed" beside an
/// account that reports nothing is the kind of half-truth somebody debugs for
/// an hour.
fn worst(found: impl IntoIterator<Item = HookStatus>) -> HookStatus {
    let mut worst = HookStatus::Installed;
    for status in found {
        worst = match (worst, status) {
            (HookStatus::NotInstalled, _) | (_, HookStatus::NotInstalled) => {
                HookStatus::NotInstalled
            }
            (HookStatus::Stale, _) | (_, HookStatus::Stale) => HookStatus::Stale,
            _ => HookStatus::Installed,
        };
    }
    worst
}

/// What the machine has, and what is missing.
///
/// Checked on demand rather than cached: someone told that git is missing will
/// go and install it, and the next thing they do is look again.
#[tauri::command]
pub fn check_requirements(state: State<'_, AppState>) -> Vec<Requirement> {
    requirements::check_in(&state.beacon().account_homes())
}

/// The same, after forgetting what Beacon had worked out about this machine.
///
/// Asked for by name, because it is slow — a shell and a few short processes —
/// and because the ordinary check has to stay fast enough to open a screen
/// with. This is the button somebody presses after installing what was
/// missing, and the whole point of it is that the answer is not the one Beacon
/// already had.
#[tauri::command]
pub async fn recheck_requirements(state: State<'_, AppState>) -> CommandResult<Vec<Requirement>> {
    // Read before leaving the thread: the state is not `Send`, and this is a
    // snapshot of a preference rather than something to watch.
    let homes = state.beacon().account_homes();
    run_off_thread(move || Ok(requirements::recheck(&homes))).await
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
///
/// On Windows it is the path alone: the hook is registered with `hook` in its
/// `args`, which Claude Code runs without a shell. See ADR-073.
pub(super) fn hook_command() -> String {
    let daemon = daemon_binary_path().to_string_lossy().into_owned();
    if cfg!(windows) {
        return daemon;
    }
    format!("{} hook", claude_hooks::shell_quote(&daemon))
}

pub(super) fn status_line_command() -> String {
    format!(
        "{} statusline",
        claude_hooks::command_word(&daemon_binary_path().to_string_lossy())
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
pub fn claude_integration(state: State<'_, AppState>) -> CommandResult<Integration> {
    let command = hook_command();
    let command = std::path::Path::new(&command);
    // A home that cannot be read counts as not installed rather than failing
    // the whole screen: one unparseable `settings.json` in one account would
    // otherwise leave the reader unable to see or fix any of the others, and
    // "not installed" is both the safe reading and the one whose button does
    // the right thing.
    let mut hooks = Vec::new();
    let mut lines = true;
    for settings in claude_settings(&state) {
        hooks.push(claude_hooks::status_at(&settings, command).unwrap_or(HookStatus::NotInstalled));
        lines = lines && claude_hooks::status_line_installed_at(&settings).unwrap_or(false);
    }

    Ok(Integration {
        hooks: worst(hooks),
        hook_command: claude_hook_command(),
        status_line: lines,
        status_line_command: status_line_command(),
    })
}

/// Takes over Claude Code's status line, which is the only place it reports the
/// five-hour allowance and how full the context is. Whatever was there still
/// runs and is still what Claude Code shows.
#[tauri::command]
pub fn install_claude_status_line(state: State<'_, AppState>) -> CommandResult<Integration> {
    let command = status_line_command();
    for settings in claude_settings(&state) {
        claude_hooks::install_status_line_at(&settings, std::path::Path::new(&command))?;
    }
    tracing::info!("took over Claude Code's status line");
    claude_integration(state)
}

#[tauri::command]
pub fn remove_claude_status_line(state: State<'_, AppState>) -> CommandResult<Integration> {
    for settings in claude_settings(&state) {
        claude_hooks::remove_status_line_at(&settings)?;
    }
    tracing::info!("gave Claude Code's status line back");
    claude_integration(state)
}

/// Whether Beacon's hooks are registered with Claude Code, and whether they
/// still point at this build.
#[tauri::command]
pub fn claude_hook_status(state: State<'_, AppState>) -> CommandResult<HookStatus> {
    let command = hook_command();
    let command = std::path::Path::new(&command);
    let mut found = Vec::new();
    for settings in claude_settings(&state) {
        found.push(claude_hooks::status_at(&settings, command).unwrap_or(HookStatus::NotInstalled));
    }
    Ok(worst(found))
}

/// Registers the hooks in the user's Claude Code settings.
///
/// Explicitly, never on startup: this writes to a file that belongs to another
/// application, and doing that unasked is not Beacon's to decide.
#[tauri::command]
pub fn install_claude_hooks(state: State<'_, AppState>) -> CommandResult<HookStatus> {
    let command = hook_command();
    // Every account is attempted before anything is reported. Stopping at the
    // first failure would leave the accounts after it untouched, and the
    // status that comes back says which — so a half-done install reads as not
    // done, and pressing the button again finishes the job.
    let mut failed = None;
    for settings in claude_settings(&state) {
        if let Err(error) = claude_hooks::install_at(&settings, std::path::Path::new(&command)) {
            tracing::warn!(%error, settings = ?settings, "could not register the hooks here");
            failed = Some(error);
        }
    }
    if let Some(error) = failed {
        return Err(error.into());
    }
    tracing::info!("registered Beacon's hooks with Claude Code");
    claude_hook_status(state)
}

#[tauri::command]
pub fn remove_claude_hooks(state: State<'_, AppState>) -> CommandResult<HookStatus> {
    // Out of every account, whatever one of them says: a removal that stops
    // half way cannot be finished by pressing the button again.
    let mut failed = None;
    for settings in claude_settings(&state) {
        if let Err(error) = claude_hooks::uninstall_at(&settings) {
            tracing::warn!(%error, settings = ?settings, "could not remove the hooks here");
            failed = Some(error);
        }
    }
    if let Some(error) = failed {
        return Err(error.into());
    }
    tracing::info!("removed Beacon's hooks from Claude Code");
    claude_hook_status(state)
}

/// The exact command that would be registered, so the user can see what Beacon
/// is asking to add before agreeing to it.
#[tauri::command]
pub fn claude_hook_command() -> String {
    if cfg!(windows) {
        // What runs, written the way someone would type it: the entry itself
        // holds the path and its argument separately.
        return format!("\"{}\" hook", hook_command());
    }
    hook_command()
}

/// Everything the Codex section needs to describe itself.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexIntegration {
    plugin: PluginStatus,
    /// Where Beacon writes the marketplace, shown so the user can see what is
    /// being added to their Codex before agreeing to it.
    marketplace: String,
    /// What the installed Codex can do, or nothing if there is no Codex.
    capabilities: codex::Capabilities,
}

fn codex_integration_now() -> CommandResult<CodexIntegration> {
    Ok(CodexIntegration {
        plugin: codex_plugin::status(&daemon_binary_path())?,
        marketplace: codex_plugin::marketplace_dir()
            .to_string_lossy()
            .into_owned(),
        capabilities: codex::capabilities().clone(),
    })
}

#[tauri::command]
pub fn codex_integration() -> CommandResult<CodexIntegration> {
    codex_integration_now()
}

/// Generates Beacon's Codex marketplace and asks Codex to install the plugin
/// in it.
///
/// Explicitly, never on startup, for the same reason the Claude hooks are: this
/// writes into another application's configuration, and doing that unasked is
/// not Beacon's to decide.
///
/// It does not — and cannot — trust the hooks. Codex keeps a hash of every hook
/// it has been shown and runs none it has not, because a hook is arbitrary
/// code. The last step is the user's, in Codex's own `/hooks`.
#[tauri::command]
pub fn install_codex_plugin(state: State<'_, AppState>) -> CommandResult<CodexIntegration> {
    let mut failed = None;
    for home in homes(&state, AgentKind::Codex) {
        if let Err(error) = codex_plugin::install(&daemon_binary_path(), Some(&home)) {
            tracing::warn!(%error, home = ?home, "could not install the plugin here");
            failed = Some(error);
        }
    }
    if let Some(error) = failed {
        return Err(error.into());
    }
    tracing::info!("installed Beacon's plugin into Codex");
    codex_integration_now()
}

#[tauri::command]
pub fn remove_codex_plugin(state: State<'_, AppState>) -> CommandResult<CodexIntegration> {
    codex_plugin::uninstall(&homes(&state, AgentKind::Codex))?;
    tracing::info!("removed Beacon's plugin from Codex");
    codex_integration_now()
}

/// What the installed Codex can do. Read the same way Claude Code's is: by
/// asking the program, not a table of versions.
#[tauri::command]
pub fn codex_capabilities() -> codex::Capabilities {
    codex::capabilities().clone()
}

/// What the installed Claude Code can do, so a feature built on a flag it does
/// not have can hide rather than fail.
///
/// Worked out once per process and cached in `beacon-core`: it costs three
/// short processes and cannot change under a running binary.
#[tauri::command]
pub fn claude_capabilities() -> Capabilities {
    claude::capabilities().clone()
}
