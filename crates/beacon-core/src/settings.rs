use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::appearance::Appearance;
use crate::paths::default_projects_home;

/// A shell and how to start it.
///
/// Arguments are configurable because "login shell" is spelled differently
/// enough to matter: `-l` for zsh, bash and fish, nothing at all for nushell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Application-wide preferences. Deliberately small: anything that belongs to a
/// workspace lives in `workspaces.json`, anything ephemeral lives in
/// `ui-state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    /// Root that portable project paths are stored relative to.
    /// `None` means "use this machine's default", which is what keeps the file
    /// itself portable between macOS and Linux.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects_home: Option<String>,
    /// Keyboard bindings, keyed by action id.
    ///
    /// Only the ones that differ from the default are stored, so changing a
    /// default in a later version reaches everyone who never overrode it, and
    /// no stale entry is left behind pointing at an action that moved on.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, String>,
    /// What a terminal runs, when the user wants something other than the
    /// shell their account is set to.
    ///
    /// Beacon is the terminal emulator, so this is a shell — zsh, fish, nu —
    /// and not another emulator. `None` means `$SHELL`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellSpec>,
    /// How the window looks. Stored whole rather than only when changed: unlike
    /// a shortcut, every field here is always in effect.
    #[serde(default)]
    pub appearance: Appearance,
}

impl Settings {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn projects_home_path(&self) -> PathBuf {
        self.projects_home
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(default_projects_home)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            projects_home: None,
            bindings: BTreeMap::new(),
            shell: None,
            appearance: Appearance::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_projects_home_falls_back_to_the_machine_default() {
        assert_eq!(
            Settings::default().projects_home_path(),
            default_projects_home()
        );
    }

    #[test]
    fn a_fresh_configuration_stores_no_bindings() {
        // Defaults are not written out: they are not the user's choices.
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!json.contains("bindings"), "got {json}");
    }

    #[test]
    fn an_explicit_projects_home_wins() {
        let settings = Settings {
            projects_home: Some("/srv/code".into()),
            ..Settings::default()
        };
        assert_eq!(settings.projects_home_path(), PathBuf::from("/srv/code"));
    }
}
