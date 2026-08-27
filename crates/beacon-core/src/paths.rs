use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where Beacon keeps its own configuration.
///
/// Resolved here rather than through Tauri so that a future headless daemon
/// reads exactly the same files as the UI.
///
/// `BEACON_CONFIG_DIR` overrides it, which is what makes a second, isolated
/// Beacon possible — and what the tests use, so that running them cannot
/// rewrite the workspaces or the clip book somebody is actually working in.
/// The socket has had an equivalent escape hatch since the daemon existed; the
/// configuration needed one the moment the daemon started writing to it.
pub fn default_config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("BEACON_CONFIG_DIR") {
        return PathBuf::from(dir);
    }

    let home = home_dir();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/beacon-split")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("beacon-split")
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The default root Beacon assumes projects live under.
pub fn default_projects_home() -> PathBuf {
    home_dir().join("projects")
}

/// A project location that survives moving between machines.
///
/// Anything under `projects_home` is stored relative to it, so the same
/// `workspaces.json` works on macOS (`/Users/x/projects`) and Linux
/// (`/home/x/projects`). Projects outside that root fall back to an absolute
/// path — correctness first, portability where we can get it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "base", rename_all = "camelCase")]
pub enum ProjectPath {
    #[serde(rename_all = "camelCase")]
    ProjectsHome { relative: String },
    #[serde(rename_all = "camelCase")]
    Absolute { path: String },
}

impl ProjectPath {
    /// Classifies an absolute path against the current projects home.
    pub fn from_absolute(path: impl AsRef<Path>, projects_home: &Path) -> Self {
        let path = path.as_ref();
        match path.strip_prefix(projects_home) {
            Ok(relative) if !relative.as_os_str().is_empty() => Self::ProjectsHome {
                relative: to_portable_string(relative),
            },
            _ => Self::Absolute {
                path: path.to_string_lossy().into_owned(),
            },
        }
    }

    pub fn resolve(&self, projects_home: &Path) -> PathBuf {
        match self {
            Self::ProjectsHome { relative } => relative
                .split('/')
                .fold(projects_home.to_path_buf(), |acc, segment| {
                    acc.join(segment)
                }),
            Self::Absolute { path } => PathBuf::from(path),
        }
    }

    /// Short form for the UI: `Personal/beacon-split` or `~/work/thing`.
    pub fn display(&self, projects_home: &Path) -> String {
        match self {
            Self::ProjectsHome { relative } => relative.clone(),
            Self::Absolute { path } => {
                let home = home_dir();
                match Path::new(path).strip_prefix(&home) {
                    Ok(rest) => format!("~/{}", to_portable_string(rest)),
                    Err(_) => {
                        let _ = projects_home;
                        path.clone()
                    }
                }
            }
        }
    }
}

/// Always emit `/` separators in stored paths, whatever the host uses.
fn to_portable_string(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_directory_can_be_moved_out_of_the_way() {
        // Not a convenience: without it a test that starts a daemon writes to
        // the real clip book, and one that clears a drawer clears the user's.
        // SAFETY: single-threaded, and put back before anything else reads it.
        unsafe { std::env::set_var("BEACON_CONFIG_DIR", "/tmp/beacon-elsewhere") };
        assert_eq!(default_config_dir(), PathBuf::from("/tmp/beacon-elsewhere"));
        unsafe { std::env::remove_var("BEACON_CONFIG_DIR") };
        assert_ne!(default_config_dir(), PathBuf::from("/tmp/beacon-elsewhere"));
    }

    #[test]
    fn paths_inside_projects_home_are_stored_relatively() {
        let home = Path::new("/Users/eya/projects");
        let stored = ProjectPath::from_absolute("/Users/eya/projects/Personal/beacon", home);
        assert_eq!(
            stored,
            ProjectPath::ProjectsHome {
                relative: "Personal/beacon".into()
            }
        );
    }

    #[test]
    fn a_relative_project_resolves_under_a_different_projects_home() {
        let stored = ProjectPath::ProjectsHome {
            relative: "Personal/beacon".into(),
        };
        assert_eq!(
            stored.resolve(Path::new("/home/eya/projects")),
            PathBuf::from("/home/eya/projects/Personal/beacon")
        );
    }

    #[test]
    fn paths_outside_projects_home_stay_absolute() {
        let home = Path::new("/Users/eya/projects");
        let stored = ProjectPath::from_absolute("/opt/src/thing", home);
        assert_eq!(
            stored,
            ProjectPath::Absolute {
                path: "/opt/src/thing".into()
            }
        );
    }

    #[test]
    fn projects_home_itself_is_not_a_project() {
        let home = Path::new("/Users/eya/projects");
        let stored = ProjectPath::from_absolute("/Users/eya/projects", home);
        assert!(matches!(stored, ProjectPath::Absolute { .. }));
    }
}
