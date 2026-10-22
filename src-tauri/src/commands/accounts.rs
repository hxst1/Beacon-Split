//! Which account each agent signs in with.
//!
//! An account is a configuration home and nothing else (ADR-099). Beacon
//! decides which directory a session is started with; the agent's own `login`
//! command does everything that involves a credential, and Beacon never reads,
//! writes, copies or moves one.
//!
//! Switching takes effect the next time the agent starts, because the home is
//! an environment variable and a running process cannot be handed another one.
//! The window says so rather than letting it be discovered.

use tauri::State;

use beacon_core::account::Profile;
use beacon_core::agent::AgentKind;
use beacon_core::claude_hooks;
use beacon_core::client::daemon_binary_path;
use beacon_core::codex_plugin;
use beacon_core::domain::AccountId;
use beacon_core::state::Snapshot;

use crate::error::CommandResult;
use crate::state::AppState;

/// Adds an account: a new, empty configuration home with a name on it.
///
/// Nobody is signed in to it yet, and Beacon will not do that part — the
/// account reports itself as needing a sign-in until somebody runs the agent's
/// own login command with this home, which the terminal can type for them.
#[tauri::command]
pub fn add_account(
    state: State<'_, AppState>,
    agent: AgentKind,
    label: String,
) -> CommandResult<Snapshot> {
    let profile = {
        let mut beacon = state.beacon();
        beacon.add_account(agent, &label)?
    };
    tracing::info!(agent = agent.as_str(), home = ?profile.home, "added an account");

    // Whatever Beacon has already been allowed to install goes into the new
    // home too. Not a new decision: the user agreed to Beacon registering its
    // hooks, and an account without them is one where the amber tab, "waiting
    // for you" and resuming a conversation all silently stop working. Where
    // they were never installed, nothing is installed here either.
    if let Err(error) = carry_over(agent, &profile) {
        // Not fatal, and not silent. The account exists and can be signed in
        // to; what is missing is reporting, which the integration screen
        // already shows and can install.
        tracing::warn!(%error, "could not carry Beacon's reporting into the new account");
    }

    Ok(state.beacon().snapshot())
}

/// Installs into a new home whatever is installed in the machine's own.
fn carry_over(agent: AgentKind, profile: &Profile) -> Result<(), beacon_core::error::CoreError> {
    let own = beacon_core::account::home_or_default(agent, None);

    match agent {
        AgentKind::Claude => {
            let command = super::hook_command();
            let command = std::path::Path::new(&command);
            let theirs = profile.home.join("settings.json");

            if claude_hooks::status_at(&own.join("settings.json"), command)?
                == claude_hooks::HookStatus::Installed
            {
                claude_hooks::install_at(&theirs, command)?;
            }
            if claude_hooks::status_line_installed_at(&own.join("settings.json"))? {
                let line = super::status_line_command();
                claude_hooks::install_status_line_at(&theirs, std::path::Path::new(&line))?;
            }
        }
        AgentKind::Codex => {
            // Codex keeps a hash of every hook it has been shown and runs none
            // it has not, so the last step is the user's in Codex's own
            // `/hooks` — here as it was for the first account.
            if codex_plugin::status(&daemon_binary_path())? == codex_plugin::PluginStatus::Installed
            {
                codex_plugin::install(&daemon_binary_path(), Some(&profile.home))?;
            }
        }
    }
    Ok(())
}

/// Switches which account the agent's next session uses.
///
/// `None` is the machine's own — `~/.claude`, `~/.codex`, with no variable set
/// at all, which is where everybody starts and what Beacon leaves alone.
#[tauri::command]
pub fn set_account(
    state: State<'_, AppState>,
    agent: AgentKind,
    id: Option<AccountId>,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_active_account(agent, id.clone())?;
    tracing::info!(agent = agent.as_str(), account = ?id, "switched account");
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn rename_account(
    state: State<'_, AppState>,
    agent: AgentKind,
    id: AccountId,
    label: String,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.rename_account(agent, &id, &label)?;
    Ok(beacon.snapshot())
}

/// Takes an account off the list and leaves its files where they are.
///
/// Deliberately not a delete. That directory holds somebody's settings, their
/// history and a live credential, and none of it is Beacon's to destroy from a
/// menu. Forgetting is reversible; deleting is not.
#[tauri::command]
pub fn forget_account(
    state: State<'_, AppState>,
    agent: AgentKind,
    id: AccountId,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.forget_account(agent, &id)?;
    tracing::info!(agent = agent.as_str(), account = %id, "forgot an account");
    Ok(beacon.snapshot())
}
