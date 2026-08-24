use std::path::Path;

use serde::{Deserialize, Serialize};

/// What a folder appears to be. Purely advisory — used for tab badges and,
/// later, for guessing dev-server commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectKind {
    Git,
    Node,
    Pnpm,
    Yarn,
    Bun,
    Deno,
    Rust,
    Python,
    Go,
    Tauri,
    Docker,
}

/// Marker files, in the order we want them reported.
const MARKERS: &[(&str, ProjectKind)] = &[
    (".git", ProjectKind::Git),
    ("package.json", ProjectKind::Node),
    ("pnpm-lock.yaml", ProjectKind::Pnpm),
    ("yarn.lock", ProjectKind::Yarn),
    ("bun.lockb", ProjectKind::Bun),
    ("deno.json", ProjectKind::Deno),
    ("Cargo.toml", ProjectKind::Rust),
    ("pyproject.toml", ProjectKind::Python),
    ("requirements.txt", ProjectKind::Python),
    ("go.mod", ProjectKind::Go),
    ("src-tauri", ProjectKind::Tauri),
    ("docker-compose.yml", ProjectKind::Docker),
    ("docker-compose.yaml", ProjectKind::Docker),
];

/// Inspects a folder with a handful of `exists` checks.
///
/// Adding a project must feel instant, so this stays at the top level and never
/// walks the tree.
pub fn detect_kinds(root: &Path) -> Vec<ProjectKind> {
    let mut kinds = Vec::new();
    for (marker, kind) in MARKERS {
        if root.join(marker).exists() && !kinds.contains(kind) {
            kinds.push(*kind);
        }
    }
    kinds
}

/// The name we suggest when a folder is added — its directory name.
pub fn suggest_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markers_it_finds_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let kinds = detect_kinds(dir.path());
        assert_eq!(
            kinds,
            vec![ProjectKind::Git, ProjectKind::Node, ProjectKind::Pnpm]
        );
    }

    #[test]
    fn an_empty_folder_detects_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect_kinds(dir.path()).is_empty());
    }

    #[test]
    fn python_is_reported_once_even_with_two_markers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "").unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "").unwrap();
        assert_eq!(detect_kinds(dir.path()), vec![ProjectKind::Python]);
    }

    #[test]
    fn suggested_name_is_the_folder_name() {
        assert_eq!(suggest_name(Path::new("/a/b/my-app")), "my-app");
    }
}
