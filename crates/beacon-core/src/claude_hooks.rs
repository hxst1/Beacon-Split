use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::error::{CoreError, Result};

/// Marks the hook entries as ours, so they can be found and removed again
/// without disturbing anything else in the file.
const MARKER: &str = "beacon-split";

/// The events worth knowing about.
///
/// Deliberately few. Every hook is a process Claude has to start, and only
/// events that change what someone would *do* about a project earn one.
const EVENTS: &[&str] = &[
    "PermissionRequest",
    "Notification",
    "PreToolUse",
    "UserPromptSubmit",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HookStatus {
    /// Registered and pointing at this build.
    Installed,
    /// Registered, but pointing somewhere else — an older Beacon, or one that
    /// has moved.
    Stale,
    NotInstalled,
}

/// Where Claude Code keeps user-level settings.
///
/// User level, not project level: the project file is committed, and Beacon has
/// no business putting a path from this machine into someone's repository.
pub fn settings_path() -> PathBuf {
    crate::paths::home_dir().join(".claude/settings.json")
}

/// Whether Beacon's hooks are registered, and whether they still point here.
pub fn status(command: &Path) -> Result<HookStatus> {
    status_at(&settings_path(), command)
}

pub fn install(command: &Path) -> Result<()> {
    install_at(&settings_path(), command)
}

pub fn uninstall() -> Result<()> {
    uninstall_at(&settings_path())
}

/// The same, against a settings file named explicitly.
///
/// Parameterised so tests never go near the real one: a test that can rewrite
/// `~/.claude/settings.json` can break Claude Code everywhere on the machine.
pub fn status_at(path: &Path, command: &Path) -> Result<HookStatus> {
    let settings = read(path)?;
    let hooks = settings.get("hooks").and_then(Value::as_object);
    let Some(hooks) = hooks else {
        return Ok(HookStatus::NotInstalled);
    };

    let mut found = false;
    for event in EVENTS {
        let Some(groups) = hooks.get(*event).and_then(Value::as_array) else {
            return Ok(if found {
                HookStatus::Stale
            } else {
                HookStatus::NotInstalled
            });
        };

        match ours(groups) {
            Some(registered) => {
                found = true;
                if registered != command.to_string_lossy() {
                    return Ok(HookStatus::Stale);
                }
            }
            None => {
                return Ok(if found {
                    HookStatus::Stale
                } else {
                    HookStatus::NotInstalled
                });
            }
        }
    }

    Ok(HookStatus::Installed)
}

/// The command registered by our group, if it is there.
fn ours(groups: &[Value]) -> Option<String> {
    groups
        .iter()
        .find(|group| group.get("matcher").and_then(Value::as_str) == Some("*"))
        .and_then(|group| group.get("hooks")?.as_array())
        .and_then(|hooks| {
            hooks.iter().find_map(|hook| {
                let command = hook.get("command")?.as_str()?;
                command.contains(MARKER).then(|| command.to_string())
            })
        })
}

/// Registers the hooks, replacing any Beacon left from a previous install.
///
/// Everything else in the file is preserved: this is the user's configuration,
/// and Beacon is a guest in it.
pub fn install_at(path: &Path, command: &Path) -> Result<()> {
    let mut settings = read(path)?;

    let object = settings
        .as_object_mut()
        .ok_or_else(|| CoreError::invalid("~/.claude/settings.json is not an object"))?;

    let hooks = object
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| CoreError::invalid("the hooks section is not an object"))?;

    for event in EVENTS {
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| CoreError::invalid(format!("the {event} hooks are not a list")))?;

        remove_ours(groups);
        groups.push(json!({
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": command.to_string_lossy(),
                // A hook that hangs would hang Claude. This one writes a line
                // to a socket and exits.
                "timeout": 5
            }]
        }));
    }

    write(path, &settings)
}

/// Takes the hooks out again, leaving the rest of the file alone.
pub fn uninstall_at(path: &Path) -> Result<()> {
    let mut settings = read(path)?;

    let Some(object) = settings.as_object_mut() else {
        return Ok(());
    };
    let Some(hooks) = object.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Ok(());
    };

    for event in EVENTS {
        if let Some(groups) = hooks.get_mut(*event).and_then(Value::as_array_mut) {
            remove_ours(groups);
        }
    }

    // Leave nothing behind: empty lists we created are not the user's.
    hooks.retain(|_, groups| !groups.as_array().is_some_and(|list| list.is_empty()));
    if hooks.is_empty() {
        object.remove("hooks");
    }

    write(path, &settings)
}

fn remove_ours(groups: &mut Vec<Value>) {
    groups.retain(|group| {
        !group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|command| command.contains(MARKER))
                })
            })
    });
}

fn read(path: &Path) -> Result<Value> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| CoreError::Parse {
            path: path.to_path_buf(),
            source,
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(err) => Err(CoreError::io(path, err)),
    }
}

/// Written through a temporary file, like Beacon's own settings.
///
/// A truncated `~/.claude/settings.json` would break Claude Code everywhere,
/// not just here.
fn write(path: &Path, settings: &Value) -> Result<()> {
    let json = serde_json::to_vec_pretty(settings).map_err(|source| CoreError::Serialize {
        path: path.to_path_buf(),
        source,
    })?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| CoreError::io(parent, err))?;
    }

    let tmp = path.with_extension("json.beacon-tmp");
    std::fs::write(&tmp, &json).map_err(|err| CoreError::io(&tmp, err))?;
    std::fs::rename(&tmp, path).map_err(|err| CoreError::io(path, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        (dir, path)
    }

    fn beacon() -> PathBuf {
        PathBuf::from("/Applications/beacon-split.app/Contents/MacOS/beacon-daemon")
    }

    #[test]
    fn nothing_is_installed_to_begin_with() {
        let (_guard, path) = scratch();
        assert_eq!(
            status_at(&path, &beacon()).unwrap(),
            HookStatus::NotInstalled
        );
    }

    #[test]
    fn installing_registers_every_event_and_reports_itself() {
        let (_guard, path) = scratch();
        install_at(&path, &beacon()).unwrap();

        assert_eq!(status_at(&path, &beacon()).unwrap(), HookStatus::Installed);

        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        for event in EVENTS {
            assert!(hooks.contains_key(*event), "{event} was not registered");
        }
    }

    #[test]
    fn a_hook_pointing_at_another_beacon_is_stale_rather_than_missing() {
        // An app that moved, or an older build. The difference matters: one
        // needs installing, the other needs replacing.
        let (_guard, path) = scratch();
        install_at(
            &path,
            Path::new("/somewhere/else/beacon-split/beacon-daemon"),
        )
        .unwrap();

        assert_eq!(status_at(&path, &beacon()).unwrap(), HookStatus::Stale);
    }

    #[test]
    fn installing_twice_does_not_register_twice() {
        let (_guard, path) = scratch();
        install_at(&path, &beacon()).unwrap();
        install_at(&path, &beacon()).unwrap();

        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let groups = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(groups.len(), 1, "got {groups:?}");
    }

    #[test]
    fn the_users_own_settings_and_hooks_are_left_alone() {
        let (_guard, path) = scratch();
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({
                "model": "opus",
                "permissions": { "allow": ["Bash(ls:*)"] },
                "hooks": {
                    "Stop": [{
                        "matcher": "*",
                        "hooks": [{ "type": "command", "command": "/usr/local/bin/my-own-thing" }]
                    }]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        install_at(&path, &beacon()).unwrap();
        uninstall_at(&path).unwrap();

        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(settings["model"], "opus");
        assert_eq!(settings["permissions"]["allow"][0], "Bash(ls:*)");

        let groups = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(groups.len(), 1, "the user's own hook should survive");
        assert_eq!(
            groups[0]["hooks"][0]["command"],
            "/usr/local/bin/my-own-thing"
        );
    }

    #[test]
    fn uninstalling_leaves_no_empty_scaffolding_behind() {
        let (_guard, path) = scratch();
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({ "model": "opus" })).unwrap(),
        )
        .unwrap();

        install_at(&path, &beacon()).unwrap();
        uninstall_at(&path).unwrap();

        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(settings.get("hooks").is_none(), "got {settings}");
        assert_eq!(settings["model"], "opus");
    }

    #[test]
    fn uninstalling_when_nothing_is_installed_is_not_an_error() {
        let (_guard, path) = scratch();
        assert!(uninstall_at(&path).is_ok());
    }

    #[test]
    fn a_settings_file_that_is_not_json_is_refused_rather_than_overwritten() {
        let (_guard, path) = scratch();
        std::fs::write(&path, "{ this is not json").unwrap();

        assert!(install_at(&path, &beacon()).is_err());
        // And it is still there, untouched.
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ this is not json"
        );
    }
}
