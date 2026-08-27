use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::error::{CoreError, Result};

/// Marks the hook entries as ours, so they can be found and removed again
/// without disturbing anything else in the file.
///
/// The daemon's own file name, not the product name: the application is called
/// "Beacon Split" and installs at `/Applications/Beacon Split.app`, so a path
/// to it contains no `beacon-split` anywhere. Marking by the product name found
/// our entries in a development checkout and never in an installed build, which
/// meant every install added another copy instead of replacing the last.
const MARKER: &str = "beacon-daemon";

/// Where the status line Beacon displaced is remembered.
///
/// Spelled out rather than built from `MARKER`, because it is already written
/// into settings files in the wild and renaming it would strand them.
const PREVIOUS_STATUS_LINE: &str = "beacon-splitPreviousStatusLine";

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
        let registered = hooks
            .get(*event)
            .and_then(Value::as_array)
            .map(|groups| ours(groups))
            .unwrap_or_default();

        match registered.as_slice() {
            // More than one means an earlier install left a copy behind, so
            // Claude would run the hook twice for every event. Stale rather
            // than installed: reinstalling is what clears it.
            [only] if *only == command.to_string_lossy() => found = true,
            [] => {
                return Ok(if found {
                    HookStatus::Stale
                } else {
                    HookStatus::NotInstalled
                });
            }
            _ => return Ok(HookStatus::Stale),
        }
    }

    Ok(HookStatus::Installed)
}

/// Every command registered by us for one event.
///
/// A list rather than the first match, so duplicates left by an install that
/// could not recognise its own entries are visible instead of silent.
fn ours(groups: &[Value]) -> Vec<String> {
    groups
        .iter()
        .filter_map(|group| group.get("hooks")?.as_array())
        .flatten()
        .filter_map(|hook| {
            let command = hook.get("command")?.as_str()?;
            command.contains(MARKER).then(|| command.to_string())
        })
        .collect()
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

/// Takes over Claude Code's status line, keeping whatever was there.
///
/// The status line is one slot, not a list: configuring one replaces what came
/// before. So Beacon's is given the previous command as an argument and runs
/// it, and what Claude Code shows is still the user's own line. Replacing it
/// outright would be taking something away in exchange for a feature they asked
/// to add.
pub fn install_status_line_at(path: &Path, command: &Path) -> Result<()> {
    let mut settings = read(path)?;
    let object = settings
        .as_object_mut()
        .ok_or_else(|| CoreError::invalid("~/.claude/settings.json is not an object"))?;

    let existing = object
        .get("statusLine")
        .and_then(|line| line.get("command"))
        .and_then(Value::as_str)
        .filter(|previous| !previous.contains(MARKER))
        // An empty command is nothing to preserve, and putting one back would
        // leave Claude Code running a status line that prints nothing.
        .filter(|previous| !previous.trim().is_empty())
        .map(str::to_string);

    let ours = match &existing {
        Some(previous) => format!("{} {}", command.display(), shell_quote(previous)),
        None => command.display().to_string(),
    };

    if let Some(previous) = &existing {
        // Recorded so uninstalling can put it back exactly.
        object.insert(PREVIOUS_STATUS_LINE.into(), Value::String(previous.clone()));
    }

    object.insert(
        "statusLine".into(),
        json!({ "type": "command", "command": ours }),
    );
    write(path, &settings)
}

/// Gives the status line back, restoring whatever Beacon displaced.
pub fn remove_status_line_at(path: &Path) -> Result<()> {
    let mut settings = read(path)?;
    let Some(object) = settings.as_object_mut() else {
        return Ok(());
    };

    let is_ours = object
        .get("statusLine")
        .and_then(|line| line.get("command"))
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(MARKER));

    if !is_ours {
        return Ok(());
    }

    match object
        .remove(PREVIOUS_STATUS_LINE)
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|previous| !previous.trim().is_empty())
    {
        Some(previous) => {
            object.insert(
                "statusLine".into(),
                json!({ "type": "command", "command": previous }),
            );
        }
        None => {
            object.remove("statusLine");
        }
    }

    write(path, &settings)
}

pub fn status_line_installed_at(path: &Path) -> Result<bool> {
    Ok(read(path)?
        .get("statusLine")
        .and_then(|line| line.get("command"))
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(MARKER)))
}

pub fn install_status_line(command: &Path) -> Result<()> {
    install_status_line_at(&settings_path(), command)
}

pub fn remove_status_line() -> Result<()> {
    remove_status_line_at(&settings_path())
}

pub fn status_line_installed() -> Result<bool> {
    status_line_installed_at(&settings_path())
}

/// Wraps a word so it survives the shell Claude Code runs hooks through.
///
/// Not decoration: the application installs at `/Applications/Beacon Split.app`,
/// and an unquoted path through it is two words to a shell, which then tries to
/// run `/Applications/Beacon` and fails on every hook Claude fires.
pub fn shell_quote(command: &str) -> String {
    format!("'{}'", command.replace('\'', r"'\''"))
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

    /// The command as an installed build actually registers it: a quoted path
    /// through `/Applications/Beacon Split.app`, which is where the packaged
    /// application lives.
    fn beacon() -> PathBuf {
        PathBuf::from(format!(
            "{} hook",
            shell_quote("/Applications/Beacon Split.app/Contents/MacOS/beacon-daemon")
        ))
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
    fn a_path_with_a_space_in_it_survives_the_shell() {
        // "/Applications/Beacon Split.app" is two words to a shell, and Claude
        // Code runs hook commands through one. Unquoted, every hook it fired
        // tried to run "/Applications/Beacon".
        let quoted = shell_quote("/Applications/Beacon Split.app/Contents/MacOS/beacon-daemon");
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("printf '%s' {quoted}"))
            .output()
            .unwrap();

        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "/Applications/Beacon Split.app/Contents/MacOS/beacon-daemon"
        );
    }

    #[test]
    fn hooks_installed_from_the_bundle_are_recognised_as_ours() {
        // The marker used to be the product name, which appears in the
        // development checkout's path and nowhere in an installed build's — so
        // an installed Beacon could never find, replace, or remove its own
        // entries.
        let (_guard, path) = scratch();
        install_at(&path, &beacon()).unwrap();

        assert_eq!(status_at(&path, &beacon()).unwrap(), HookStatus::Installed);
        uninstall_at(&path).unwrap();
        assert_eq!(
            status_at(&path, &beacon()).unwrap(),
            HookStatus::NotInstalled
        );
    }

    #[test]
    fn a_second_copy_left_by_an_older_install_reads_as_stale() {
        // What earlier builds left behind: one hook per install, all firing.
        // Reporting it as installed would hide it; stale asks for the reinstall
        // that clears it.
        let (_guard, path) = scratch();
        install_at(&path, &beacon()).unwrap();

        let mut settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let groups = settings["hooks"]["Stop"].as_array_mut().unwrap();
        let duplicate = groups[0].clone();
        groups.push(duplicate);
        std::fs::write(&path, serde_json::to_vec_pretty(&settings).unwrap()).unwrap();

        assert_eq!(status_at(&path, &beacon()).unwrap(), HookStatus::Stale);

        // And installing again clears it rather than adding a third.
        install_at(&path, &beacon()).unwrap();
        assert_eq!(status_at(&path, &beacon()).unwrap(), HookStatus::Installed);
    }

    #[test]
    fn a_status_line_that_was_empty_is_not_put_back() {
        // An empty command is nothing to preserve, and restoring one would
        // leave Claude Code running a status line that prints nothing.
        let (_guard, path) = scratch();
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({
                "statusLine": { "type": "command", "command": "" }
            }))
            .unwrap(),
        )
        .unwrap();

        install_status_line_at(&path, &beacon()).unwrap();
        remove_status_line_at(&path).unwrap();

        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(settings.get("statusLine").is_none(), "got {settings}");
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
    fn taking_the_status_line_keeps_the_one_that_was_there() {
        let (_guard, path) = scratch();
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({
                "statusLine": { "type": "command", "command": "~/.claude/my-statusline.sh" }
            }))
            .unwrap(),
        )
        .unwrap();

        install_status_line_at(&path, &beacon()).unwrap();

        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let command = settings["statusLine"]["command"].as_str().unwrap();
        assert!(command.contains("beacon-daemon"), "got {command}");
        assert!(
            command.contains("my-statusline.sh"),
            "the user's line should still run: {command}"
        );

        // And giving it back restores exactly what was there.
        remove_status_line_at(&path).unwrap();
        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            settings["statusLine"]["command"],
            "~/.claude/my-statusline.sh"
        );
        assert!(settings.get("beacon-splitPreviousStatusLine").is_none());
    }

    #[test]
    fn taking_a_status_line_nobody_had_leaves_none_behind() {
        let (_guard, path) = scratch();
        install_status_line_at(&path, &beacon()).unwrap();
        assert!(status_line_installed_at(&path).unwrap());

        remove_status_line_at(&path).unwrap();
        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(settings.get("statusLine").is_none(), "got {settings}");
    }

    #[test]
    fn installing_twice_does_not_wrap_ourselves() {
        let (_guard, path) = scratch();
        install_status_line_at(&path, &beacon()).unwrap();
        install_status_line_at(&path, &beacon()).unwrap();

        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let command = settings["statusLine"]["command"].as_str().unwrap();
        assert_eq!(command.matches("beacon-daemon").count(), 1, "got {command}");
    }

    #[test]
    fn a_status_line_belonging_to_someone_else_is_left_alone() {
        let (_guard, path) = scratch();
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&json!({
                "statusLine": { "type": "command", "command": "/usr/local/bin/theirs" }
            }))
            .unwrap(),
        )
        .unwrap();

        remove_status_line_at(&path).unwrap();
        let settings: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(settings["statusLine"]["command"], "/usr/local/bin/theirs");
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
