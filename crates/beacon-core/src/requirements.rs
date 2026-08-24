use std::path::Path;

use serde::Serialize;

use crate::tools::{resolve_program, strip_terminal_identity};

/// How badly Beacon needs something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Importance {
    /// Beacon's central feature does not work without it.
    Required,
    /// A panel does not work without it; the rest is fine.
    Recommended,
}

/// One way to get a missing program.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOption {
    pub label: &'static str,
    pub command: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub id: &'static str,
    pub name: &'static str,
    pub importance: Importance,
    /// Where it was found, resolved the same way a session would resolve it.
    pub path: Option<String>,
    pub version: Option<String>,
    /// What stops working without it, in plain terms.
    pub what_breaks: &'static str,
    pub install: Vec<InstallOption>,
    /// Anything worth knowing beyond running the command.
    pub note: Option<&'static str>,
}

impl Requirement {
    pub fn found(&self) -> bool {
        self.path.is_some()
    }
}

/// Everything Beacon needs from the machine it is running on.
///
/// Resolved through the user's own login shell, exactly as a session would —
/// a check that looks somewhere else could pass while the thing it checked
/// still failed to start, which is worse than not checking.
pub fn check() -> Vec<Requirement> {
    vec![check_claude(), check_git()]
}

/// Whether anything Beacon considers essential is missing.
pub fn missing_essentials(requirements: &[Requirement]) -> Vec<&Requirement> {
    requirements
        .iter()
        .filter(|requirement| {
            !requirement.found() && requirement.importance == Importance::Required
        })
        .collect()
}

fn check_claude() -> Requirement {
    let path = resolve_program("claude");
    Requirement {
        id: "claude",
        name: "Claude Code",
        importance: Importance::Required,
        version: path
            .as_deref()
            .and_then(|path| version_of(path, "--version")),
        path: path.map(|path| path.to_string_lossy().into_owned()),
        what_breaks: "Beacon runs the real claude command in each project. \
                      Without it, the Claude panel has nothing to run — everything else works.",
        install: vec![
            InstallOption {
                label: "Official installer",
                command: "curl -fsSL https://claude.ai/install.sh | bash",
            },
            InstallOption {
                label: "Homebrew",
                command: "brew install --cask claude-code",
            },
        ],
        note: Some(
            "Claude Code needs a Pro, Max, Team or Enterprise account. \
             After installing, run `claude` once in a terminal to sign in — \
             Beacon does not handle signing in, it runs the CLI you already use.",
        ),
    }
}

fn check_git() -> Requirement {
    let path = resolve_program("git");
    Requirement {
        id: "git",
        name: "Git",
        importance: Importance::Recommended,
        version: path
            .as_deref()
            .and_then(|path| version_of(path, "--version")),
        path: path.map(|path| path.to_string_lossy().into_owned()),
        what_breaks: "The Git panel needs it, and Quick Open uses it to respect \
                      your ignore rules. Without it those fall back or go quiet; \
                      terminals and Claude are unaffected.",
        install: vec![
            InstallOption {
                label: "Apple command line tools",
                command: "xcode-select --install",
            },
            InstallOption {
                label: "Homebrew",
                command: "brew install git",
            },
        ],
        note: Some(
            "The Apple tools are the smaller install and enough for everything \
             Beacon does with git.",
        ),
    }
}

/// Asks a program its version, briefly.
///
/// Best-effort: a program that is present but will not say what it is still
/// counts as present, since that is what determines whether Beacon can run it.
fn version_of(path: &std::path::Path, flag: &str) -> Option<String> {
    let mut command = std::process::Command::new(path);
    command.arg(flag);
    strip_terminal_identity(&mut command);

    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?.trim();
    (!first.is_empty()).then(|| first.to_string())
}

/// Where the session daemon should be, for reporting it as missing sensibly.
pub fn daemon_present(binary: &Path) -> bool {
    binary.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_requirement_says_what_breaks_and_how_to_fix_it() {
        // A check that only says "missing" leaves the reader exactly as stuck.
        for requirement in check() {
            assert!(
                !requirement.what_breaks.is_empty(),
                "{} does not say what it costs",
                requirement.id
            );
            assert!(
                !requirement.install.is_empty(),
                "{} does not say how to get it",
                requirement.id
            );
            for option in &requirement.install {
                assert!(!option.command.is_empty());
            }
        }
    }

    #[test]
    fn claude_is_required_and_git_is_not() {
        // Losing git costs a panel. Losing claude costs the point of the app.
        let requirements = check();
        let claude = requirements.iter().find(|r| r.id == "claude").unwrap();
        let git = requirements.iter().find(|r| r.id == "git").unwrap();

        assert_eq!(claude.importance, Importance::Required);
        assert_eq!(git.importance, Importance::Recommended);
    }

    #[test]
    fn only_missing_essentials_are_reported_as_blocking() {
        let requirements = check();
        for blocking in missing_essentials(&requirements) {
            assert!(!blocking.found());
            assert_eq!(blocking.importance, Importance::Required);
        }
    }

    #[test]
    fn what_is_installed_here_is_found_with_its_version() {
        // This machine has both; the point is that resolution and the version
        // probe agree with each other.
        for requirement in check() {
            if requirement.found() {
                assert!(
                    requirement.version.is_some(),
                    "{} was found at {:?} but would not say its version",
                    requirement.id,
                    requirement.path
                );
            }
        }
    }
}
