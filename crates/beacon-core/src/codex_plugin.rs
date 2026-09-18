//! Letting Codex tell Beacon what a session is doing.
//!
//! The counterpart of [`crate::claude_hooks`], and a different shape for a
//! good reason. Claude Code's hooks are entries Beacon writes into the user's
//! settings file. Codex loads hooks from plugins, and installs plugins from
//! marketplaces — so Beacon ships one of each: a marketplace of its own with a
//! single plugin in it, generated into Beacon's own configuration directory and
//! registered with `codex plugin`.
//!
//! Generated rather than shipped, because the hook has to name the daemon on
//! *this* machine, which moves when the application does. And written under
//! the configuration directory rather than the temporary one, which is where
//! the MCP configuration used to live until macOS swept it and Claude Code
//! refused to start.
//!
//! What Beacon cannot do is trust it. Codex records a hash of every hook it has
//! been shown and runs none it has not — deliberately, because a hook is
//! arbitrary code, and forging that record would be defeating a safeguard on
//! the user's behalf. So installing is all this does, and the last step belongs
//! to the person: `/hooks` inside Codex, once. Whether it worked is not read
//! out of their configuration either; it is answered by whether a report ever
//! arrives, which is the only answer that cannot be stale.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::{CoreError, Result};

/// The events worth a hook.
///
/// Deliberately few, and every one of them is read: each hook is a process
/// Codex has to start, and an event registered here that nothing interprets is
/// a process started for nothing, on every turn.
///
/// `PostToolUse`, `PreCompact` and `PostCompact` are Codex's and are left out
/// for exactly that reason — nothing Beacon shows would change.
pub const EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "Stop",
    // Codex's own: a turn cut short. It stopped either way, which is what a tab
    // has to stop saying.
    "Interrupt",
    "SubagentStart",
    "SubagentStop",
];

/// The name Beacon's marketplace and plugin both answer to, so the selector is
/// `beacon@beacon`.
pub const NAME: &str = "beacon";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginStatus {
    /// Generated, and pointing at this build.
    Installed,
    /// Generated, but not as this build would generate it — pointing at an
    /// older Beacon or one that has moved, or missing an event added since.
    Stale,
    NotInstalled,
}

/// Where Beacon keeps the marketplace it offers Codex.
pub fn marketplace_dir() -> PathBuf {
    crate::paths::default_config_dir().join("codex-marketplace")
}

/// Whether the marketplace is there and still describes this build.
pub fn status(command: &Path) -> Result<PluginStatus> {
    status_at(&marketplace_dir(), command)
}

/// The same, against a directory named explicitly, so tests never go near the
/// real one.
pub fn status_at(root: &Path, command: &Path) -> Result<PluginStatus> {
    let path = hooks_path(root);
    let found = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PluginStatus::NotInstalled);
        }
        Err(err) => return Err(CoreError::io(&path, err)),
    };

    let found: Value = serde_json::from_slice(&found).map_err(|source| CoreError::Parse {
        path: path.clone(),
        source,
    })?;

    // Compared against what this build would write, whole. Anything that
    // differs — a moved daemon, an event added since, a file somebody edited —
    // is stale, and reinstalling is what clears it.
    Ok(if found == hooks_document(command) {
        PluginStatus::Installed
    } else {
        PluginStatus::Stale
    })
}

/// Writes the marketplace and the plugin in it.
///
/// Only the files. Registering them with Codex is a separate step because it
/// means running Codex, which can fail for reasons that have nothing to do with
/// what is on disk.
pub fn write_at(root: &Path, command: &Path) -> Result<()> {
    let plugin = plugin_dir(root);
    create_dir(&plugin.join(".claude-plugin"))?;
    create_dir(&plugin.join("hooks"))?;
    create_dir(&root.join(".agents/plugins"))?;

    write_json(&manifest_path(root), &marketplace_document())?;
    write_json(
        &plugin.join(".claude-plugin/plugin.json"),
        &plugin_document(),
    )?;
    write_json(&hooks_path(root), &hooks_document(command))
}

/// Writes the marketplace and asks Codex to install the plugin in it.
///
/// Two steps that fail for unrelated reasons, so they are reported apart: the
/// files can be written and Codex can still refuse, or be missing entirely.
pub fn install(command: &Path) -> Result<()> {
    let root = marketplace_dir();
    write_at(&root, command)?;
    register(&root)
}

/// Registers the marketplace and installs the plugin from it.
///
/// Idempotent as far as Codex is concerned — adding a marketplace it already
/// has, or a plugin it already installed, replaces what was there. Which is
/// what makes reinstalling the answer to a stale one.
pub fn register(root: &Path) -> Result<()> {
    let codex = crate::tools::resolve_program("codex").ok_or_else(|| {
        CoreError::invalid(
            "could not find the codex command. Install Codex, or make sure it is on the PATH your \
             login shell sets.",
        )
    })?;

    run(
        &codex,
        &["plugin", "marketplace", "add", &root.to_string_lossy()],
    )?;
    run(&codex, &["plugin", "add", &format!("{NAME}@{NAME}")])
}

/// Removes the plugin and the marketplace, leaving nothing of Beacon behind.
pub fn uninstall() -> Result<()> {
    let codex = crate::tools::resolve_program("codex")
        .ok_or_else(|| CoreError::invalid("could not find the codex command"))?;

    // The plugin first: a marketplace removed from under an installed plugin
    // leaves Codex describing something it can no longer find.
    run(&codex, &["plugin", "remove", &format!("{NAME}@{NAME}")])?;
    run(&codex, &["plugin", "marketplace", "remove", NAME])?;

    let root = marketplace_dir();
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|err| CoreError::io(&root, err))?;
    }
    Ok(())
}

/// Runs Codex and turns a refusal into something a person can act on.
///
/// Codex says why it refused on stderr, and that sentence is worth far more
/// than "it did not work" — so it is carried out rather than logged and
/// dropped.
fn run(codex: &Path, args: &[&str]) -> Result<()> {
    let mut command = std::process::Command::new(codex);
    command.args(args);
    crate::tools::strip_terminal_identity(&mut command);
    // An npm-installed Codex is a Node script and needs its interpreter, which
    // lives beside it.
    if let Some(path) = crate::tools::path_with_program_dir(codex) {
        command.env("PATH", path);
    }

    let output = command
        .output()
        .map_err(|err| CoreError::session("could not run codex", err))?;

    if output.status.success() {
        return Ok(());
    }

    let said = String::from_utf8_lossy(&output.stderr);
    let said = said.trim();
    Err(CoreError::invalid(if said.is_empty() {
        format!("codex {} failed", args.join(" "))
    } else {
        format!("codex {}: {said}", args.join(" "))
    }))
}

/// What Codex is asked to install: a marketplace with one plugin in it.
fn marketplace_document() -> Value {
    json!({
        "name": NAME,
        "interface": { "displayName": "Beacon Split" },
        "plugins": [{
            "name": NAME,
            "source": { "source": "local", "path": "./plugins/beacon" },
            "policy": { "installation": "AVAILABLE" },
            "category": "Developer Tools",
        }],
    })
}

fn plugin_document() -> Value {
    json!({
        "name": NAME,
        "description": "Lets Beacon Split see what a Codex session is doing.",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

/// Every event, pointing at this machine's daemon.
///
/// No environment is declared. The hook needs to know which socket, which
/// project and which agent, and it gets all three by being a child of a session
/// Beacon started with them set — which was checked against the real Codex
/// rather than assumed. It is also what makes this safe to leave installed: a
/// Codex the user starts themselves has none of it and reports nothing.
fn hooks_document(command: &Path) -> Value {
    let command = format!("{} hook", shell_quote(command));
    let hooks: serde_json::Map<String, Value> = EVENTS
        .iter()
        .map(|event| {
            (
                (*event).to_string(),
                json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 10 }] }]),
            )
        })
        .collect();

    json!({ "hooks": hooks })
}

/// A path as a shell word.
///
/// The daemon lives at `/Applications/Beacon Split.app/…`, and the space in
/// that name is not a detail: the command is a shell string, so an unquoted
/// path would run `/Applications/Beacon` with `Split.app/…` as an argument.
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

fn plugin_dir(root: &Path) -> PathBuf {
    root.join("plugins").join(NAME)
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join(".agents/plugins/marketplace.json")
}

fn hooks_path(root: &Path) -> PathBuf {
    plugin_dir(root).join("hooks/hooks.json")
}

fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|err| CoreError::io(path, err))
}

fn write_json(path: &Path, document: &Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(document).map_err(|source| CoreError::Serialize {
        path: path.to_path_buf(),
        source,
    })?;
    std::fs::write(path, bytes).map_err(|err| CoreError::io(path, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAEMON: &str = "/Applications/Beacon Split.app/Contents/MacOS/beacon-daemon";

    #[test]
    fn a_marketplace_that_was_never_written_is_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            status_at(dir.path(), Path::new(DAEMON)).unwrap(),
            PluginStatus::NotInstalled
        );
    }

    #[test]
    fn what_is_written_reads_back_as_installed() {
        let dir = tempfile::tempdir().unwrap();
        write_at(dir.path(), Path::new(DAEMON)).unwrap();

        assert_eq!(
            status_at(dir.path(), Path::new(DAEMON)).unwrap(),
            PluginStatus::Installed
        );
        // The three files Codex needs to find, in the places it looks.
        assert!(manifest_path(dir.path()).is_file());
        assert!(
            plugin_dir(dir.path())
                .join(".claude-plugin/plugin.json")
                .is_file()
        );
        assert!(hooks_path(dir.path()).is_file());
    }

    #[test]
    fn a_daemon_that_moved_makes_it_stale() {
        // What happens on every update: the application is replaced, and the
        // hooks left behind name a binary at the old path. Stale rather than
        // installed, because reinstalling is what fixes it.
        let dir = tempfile::tempdir().unwrap();
        write_at(dir.path(), Path::new("/Users/eya/old/beacon-daemon")).unwrap();

        assert_eq!(
            status_at(dir.path(), Path::new(DAEMON)).unwrap(),
            PluginStatus::Stale
        );
    }

    #[test]
    fn a_path_with_a_space_in_it_stays_one_word() {
        // `/Applications/Beacon Split.app` — unquoted, the shell would run
        // `/Applications/Beacon` and pass the rest as an argument.
        let document = hooks_document(Path::new(DAEMON));
        let command = document["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();

        assert_eq!(
            command,
            "'/Applications/Beacon Split.app/Contents/MacOS/beacon-daemon' hook"
        );
    }

    #[test]
    fn every_event_beacon_reads_is_registered_and_no_others() {
        let document = hooks_document(Path::new(DAEMON));
        let registered = document["hooks"].as_object().unwrap();

        assert_eq!(registered.len(), EVENTS.len());
        for event in EVENTS {
            assert!(registered.contains_key(*event), "{event} is missing");
        }
        // A hook is a process started on every turn, so an event nothing
        // interprets must not be here.
        for unread in ["PostToolUse", "PreCompact", "PostCompact"] {
            assert!(
                !registered.contains_key(unread),
                "{unread} costs a process and changes nothing Beacon shows"
            );
        }
    }

    #[test]
    fn the_marketplace_offers_exactly_one_plugin_at_the_path_it_is_written_to() {
        let document = marketplace_document();
        let plugins = document["plugins"].as_array().unwrap();

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0]["name"], NAME);
        // The selector `beacon@beacon` depends on both names matching, and the
        // path on where `write_at` puts the plugin.
        assert_eq!(document["name"], NAME);
        assert_eq!(plugins[0]["source"]["path"], "./plugins/beacon");
    }
}
