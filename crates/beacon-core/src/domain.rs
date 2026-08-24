use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::detect::ProjectKind;
use crate::error::{CoreError, Result};
use crate::paths::ProjectPath;

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn generate() -> Self {
                Self(format!("{}_{}", $prefix, Uuid::new_v4().simple()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(WorkspaceId, "ws");
id_type!(ProjectId, "pj");

/// A project as Beacon knows it: a folder, a display name, and what we sniffed
/// out of it. Nothing here is authoritative about the folder's contents — it is
/// a cache for the UI, refreshed on demand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub path: ProjectPath,
    #[serde(default)]
    pub kinds: Vec<ProjectKind>,
}

impl Project {
    pub fn new(name: impl Into<String>, path: ProjectPath, kinds: Vec<ProjectKind>) -> Self {
        Self {
            id: ProjectId::generate(),
            name: name.into(),
            path,
            kinds,
        }
    }
}

/// A group of projects that share a visual identity.
///
/// The accent is not decoration: it is how you recognise which workspace you
/// are in before reading anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    /// Accent colour as `#rrggbb`.
    pub accent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub projects: Vec<Project>,
}

impl Workspace {
    pub fn new(name: impl Into<String>, accent: impl Into<String>) -> Result<Self> {
        let accent = normalize_accent(accent.into())?;
        Ok(Self {
            id: WorkspaceId::generate(),
            name: name.into(),
            accent,
            icon: None,
            projects: Vec::new(),
        })
    }

    pub fn project(&self, id: &ProjectId) -> Option<&Project> {
        self.projects.iter().find(|p| &p.id == id)
    }

    pub fn project_mut(&mut self, id: &ProjectId) -> Option<&mut Project> {
        self.projects.iter_mut().find(|p| &p.id == id)
    }
}

/// Accepts `#rrggbb` / `rrggbb`, rejects anything else.
///
/// The UI derives translucent variants from this value with `color-mix`, so a
/// malformed accent would silently break every surface that uses it.
pub fn normalize_accent(value: String) -> Result<String> {
    let hex = value.trim().trim_start_matches('#');
    let valid = hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit());
    if !valid {
        return Err(CoreError::invalid(format!(
            "accent must be a 6-digit hex colour, got {value:?}"
        )));
    }
    Ok(format!("#{}", hex.to_ascii_lowercase()))
}

/// The persisted shape of `workspaces.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesFile {
    pub schema_version: u32,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
}

impl WorkspacesFile {
    pub const SCHEMA_VERSION: u32 = 1;
}

impl Default for WorkspacesFile {
    fn default() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            workspaces: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_are_normalized_to_lowercase_hash_form() {
        assert_eq!(normalize_accent("4F8DF7".into()).unwrap(), "#4f8df7");
        assert_eq!(normalize_accent("#4F8DF7".into()).unwrap(), "#4f8df7");
    }

    #[test]
    fn malformed_accents_are_rejected() {
        for bad in ["", "#fff", "blue", "#12345g"] {
            assert!(normalize_accent(bad.into()).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn generated_ids_are_prefixed_and_unique() {
        let a = WorkspaceId::generate();
        let b = WorkspaceId::generate();
        assert!(a.as_str().starts_with("ws_"));
        assert_ne!(a, b);
    }
}
