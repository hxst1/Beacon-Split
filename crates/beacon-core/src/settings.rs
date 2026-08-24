use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::default_projects_home;

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
    fn an_explicit_projects_home_wins() {
        let settings = Settings {
            projects_home: Some("/srv/code".into()),
            ..Settings::default()
        };
        assert_eq!(settings.projects_home_path(), PathBuf::from("/srv/code"));
    }
}
