use std::path::Path;

use serde::Serialize;

use crate::tools::{hide_console_window, resolve_program, strip_terminal_identity};

/// How badly Beacon needs something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Importance {
    /// Beacon's central feature does not work without it.
    Required,
    /// A panel does not work without it; the rest is fine.
    Recommended,
}

/// Where a requirement stands, in the terms somebody reading the screen cares
/// about.
///
/// Four states and not two, because "not working" has four different answers
/// and only one of them is "install it". Being told to install something that
/// is already installed is how a person decides the program is lying to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Found, and it answered when asked what it was.
    Ready,
    /// Not on this machine, as far as the login shell knows.
    Missing,
    /// There, and it would not say what version it is. Something is wrong with
    /// the installation rather than with its absence, and the command that
    /// installs it again is usually the fix — but saying "not installed" to
    /// somebody looking at the binary is not.
    Broken,
    /// There and working, and nobody has signed in. Beacon never touches the
    /// credential; it asked the program, which is the same question the user
    /// could type.
    NeedsAuth,
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
    pub state: State,
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

/// What a program's state is, given where it was found.
///
/// The version is already asked for and shown, so `Broken` costs nothing: a
/// program that resolves and will not say what it is has something wrong with
/// it, and that is worth telling apart from not being there.
///
/// `signed_in` is asked only of something that is otherwise ready, and only
/// where there is an official way to ask. `None` from it means the question
/// had no answer — a version too old to have the subcommand, or one that would
/// not finish — and then Beacon says nothing rather than guessing.
fn state_of(
    path: Option<&Path>,
    version: Option<&str>,
    signed_in: impl Fn(&Path) -> Option<bool>,
) -> State {
    let Some(path) = path else {
        return State::Missing;
    };
    if version.is_none() {
        return State::Broken;
    }
    match signed_in(path) {
        Some(false) => State::NeedsAuth,
        _ => State::Ready,
    }
}

/// For a program nobody signs in to.
fn no_sign_in(_: &Path) -> Option<bool> {
    None
}

/// Everything Beacon needs from the machine it is running on.
///
/// Resolved through the user's own login shell, exactly as a session would —
/// a check that looks somewhere else could pass while the thing it checked
/// still failed to start, which is worse than not checking.
pub fn check() -> Vec<Requirement> {
    vec![check_claude(), check_codex(), check_git()]
}

/// The same, after forgetting everything Beacon had worked out about this
/// machine.
///
/// For the one moment it matters, and it is the moment that decides whether
/// somebody stays: they read that Claude Code was missing, they installed it,
/// they came back. Everything Beacon knew — where each program was, what the
/// login shell's `PATH` is, what each agent can do — was worked out before
/// that happened, and without this they would have to restart Beacon to be
/// believed. Nothing tells them to.
///
/// It costs what a cold start costs, a shell and a few short processes, which
/// is why it is this and not what every check does.
pub fn recheck() -> Vec<Requirement> {
    crate::tools::forget_programs();
    crate::claude::forget_capabilities();
    crate::codex::forget_capabilities();
    check()
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
    let version = path
        .as_deref()
        .and_then(|path| version_of(path, "--version"));
    let state = state_of(
        path.as_deref(),
        version.as_deref(),
        crate::claude::signed_in,
    );
    Requirement {
        id: "claude",
        name: "Claude Code",
        importance: Importance::Required,
        state,
        version,
        path: path.map(|path| path.to_string_lossy().into_owned()),
        what_breaks: "Beacon runs the real claude command in each project. \
                      Without it, the Claude panel has nothing to run — everything else works.",
        install: if cfg!(windows) {
            vec![
                InstallOption {
                    label: "Official installer (PowerShell)",
                    command: "irm https://claude.ai/install.ps1 | iex",
                },
                InstallOption {
                    label: "WinGet",
                    command: "winget install Anthropic.ClaudeCode",
                },
            ]
        } else {
            vec![
                InstallOption {
                    label: "Official installer",
                    command: "curl -fsSL https://claude.ai/install.sh | bash",
                },
                InstallOption {
                    label: "Homebrew",
                    command: "brew install --cask claude-code",
                },
            ]
        },
        note: Some(
            "Claude Code needs a Pro, Max, Team or Enterprise account. \
             After installing, run `claude` once in a terminal to sign in — \
             Beacon does not handle signing in, it runs the CLI you already use.",
        ),
    }
}

/// Codex is recommended, not required.
///
/// The difference matters: without Claude Code, Beacon's central feature does
/// not work, and it says so in a way that blocks. Codex is a second agent
/// somebody may never want, so not having it costs one panel and must not
/// present itself as something wrong with the installation.
fn check_codex() -> Requirement {
    let path = resolve_program("codex");
    let version = path
        .as_deref()
        .and_then(|path| version_of(path, "--version"));
    let state = state_of(path.as_deref(), version.as_deref(), crate::codex::signed_in);
    Requirement {
        id: "codex",
        name: "Codex",
        importance: Importance::Recommended,
        state,
        version,
        path: path.map(|path| path.to_string_lossy().into_owned()),
        what_breaks: "Beacon can run Codex beside Claude Code, in its own \
                      conversation. Without it, the Codex panel has nothing to \
                      run — everything else works.",
        install: vec![
            InstallOption {
                label: "npm",
                command: "npm install -g @openai/codex",
            },
            InstallOption {
                label: "Homebrew",
                command: "brew install --cask codex",
            },
        ],
        note: Some(
            "Codex needs a ChatGPT plan or an API key. After installing, run \
             `codex` once in a terminal to sign in — Beacon does not handle \
             signing in, it runs the CLI you already use.",
        ),
    }
}

fn check_git() -> Requirement {
    let path = resolve_program("git");
    let version = path
        .as_deref()
        .and_then(|path| version_of(path, "--version"));
    let state = state_of(path.as_deref(), version.as_deref(), no_sign_in);
    Requirement {
        id: "git",
        name: "Git",
        importance: Importance::Recommended,
        state,
        version,
        path: path.map(|path| path.to_string_lossy().into_owned()),
        what_breaks: "The Git panel needs it, and Quick Open uses it to respect \
                      your ignore rules. Without it those fall back or go quiet; \
                      terminals and Claude are unaffected.",
        install: if cfg!(windows) {
            vec![InstallOption {
                label: "Git for Windows (WinGet)",
                command: "winget install --id Git.Git -e --source winget",
            }]
        } else {
            vec![
                InstallOption {
                    label: "Apple command line tools",
                    command: "xcode-select --install",
                },
                InstallOption {
                    label: "Homebrew",
                    command: "brew install git",
                },
            ]
        },
        note: Some(if cfg!(windows) {
            "Git for Windows also brings Git Bash, which Claude Code prefers \
             for running commands and hooks when it is there."
        } else {
            "The Apple tools are the smaller install and enough for everything \
             Beacon does with git."
        }),
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
    hide_console_window(&mut command);
    // An npm-installed agent is a Node script and needs its interpreter, which
    // lives beside it. Without this the program is found and then refuses to
    // say its version, which reads as a broken install rather than a missing
    // `node`.
    command.env("PATH", crate::tools::session_path(path));

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
    fn a_state_is_worked_out_from_what_was_already_asked() {
        let somewhere = Path::new("/usr/local/bin/claude");

        // Not there at all.
        assert_eq!(state_of(None, None, no_sign_in), State::Missing);

        // There, and it would not say what it is. "Not installed" would be a
        // lie to somebody looking straight at the binary.
        assert_eq!(state_of(Some(somewhere), None, no_sign_in), State::Broken);

        // There, working, and nobody signed in.
        assert_eq!(
            state_of(Some(somewhere), Some("1.0"), |_| Some(false)),
            State::NeedsAuth
        );

        // There, working, signed in.
        assert_eq!(
            state_of(Some(somewhere), Some("1.0"), |_| Some(true)),
            State::Ready
        );

        // And the one that matters most: the question had no answer, because
        // this version has no way to ask. Saying nothing is the only honest
        // move, so it reads as ready rather than as a warning nobody can act
        // on.
        assert_eq!(
            state_of(Some(somewhere), Some("1.0"), |_| None),
            State::Ready
        );
    }

    #[test]
    fn missing_is_the_only_state_that_asks_somebody_to_install_something() {
        // The point of having four: a broken install and an unsigned-in one
        // are not fixed by the install command, and showing it is how a
        // program convinces somebody it is not listening.
        for requirement in check() {
            if requirement.state == State::Missing {
                assert!(
                    !requirement.install.is_empty(),
                    "{} is missing and offers no way to get it",
                    requirement.name
                );
            }
            assert_eq!(
                requirement.state == State::Missing,
                !requirement.found(),
                "{} disagrees with itself about whether it is there",
                requirement.name
            );
        }
    }

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
