//! Finding the programs Beacon runs.
//!
//! Shared between spawning sessions and checking whether the machine has what
//! Beacon needs: both have to look in the same places, or a preflight check
//! would pass while the thing it checked still failed to start.

use std::path::PathBuf;

pub fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    })
}

/// Marks the answer inside whatever else a shell prints on the way.
const PROBE_MARKER: &str = "BEACON_RESOLVED=";

/// Finds a program the way the user's own shell would.
///
/// A GUI application starts with a minimal PATH, so Beacon must not be pickier
/// about where `claude` lives than the terminal the user installed it from.
///
/// The interactive login shell is asked first, because that is the only one
/// that reads `.zshrc` — where a great many people, including anyone using a
/// framework or a version manager, set their PATH. A non-interactive login
/// shell is the fallback, and our own PATH the last resort.
///
/// Runs once per program and is cached; it costs one short subprocess.
pub fn resolve_program(name: &str) -> Option<PathBuf> {
    let script = format!("{PROBE_MARKER}$(command -v {name} 2>/dev/null)");

    for args in [&["-l", "-i", "-c"][..], &["-l", "-c"][..]] {
        let mut probe = std::process::Command::new(user_shell());
        probe.args(args).arg(&script);
        strip_terminal_identity(&mut probe);

        if let Ok(output) = probe.output()
            && let Some(path) = extract_resolved_path(&String::from_utf8_lossy(&output.stdout))
        {
            tracing::debug!(program = name, path = %path.display(), "resolved via login shell");
            return Some(path);
        }
    }

    // Beacon's own environment, which is enough when it was launched from a
    // shell that already had the program on its PATH.
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .collect::<Vec<_>>()
        })
        .and_then(|candidates| candidates.into_iter().find(|path| path.is_file()))
}

/// Pulls the answer out of a shell's stdout.
///
/// An interactive shell prints more than the answer: a themed prompt writes a
/// terminal title escape, and it lands on the same line. The marker makes the
/// answer findable regardless of what surrounds it.
pub(crate) fn extract_resolved_path(stdout: &str) -> Option<PathBuf> {
    let answer = stdout
        .rmatch_indices(PROBE_MARKER)
        .map(|(index, _)| &stdout[index + PROBE_MARKER.len()..])
        .next()?;

    let value = answer
        .lines()
        .next()?
        .trim_matches(|c: char| c.is_whitespace() || c.is_control());

    if value.is_empty() {
        return None;
    }

    let path = PathBuf::from(value);
    path.is_file().then_some(path)
}

/// Strips the launcher's identity from a probe subprocess.
///
/// The same reasoning as [`prepare_environment`]: asking the shell a question
/// while pretending to be Terminal.app runs that terminal's session machinery,
/// which prints into the answer.
pub fn strip_terminal_identity(command: &mut std::process::Command) {
    for key in STRIPPED_ENV {
        command.env_remove(key);
    }
}

/// Environment variables a probe or a session must not inherit from whatever
/// launched Beacon. Defined in [`crate::session`], which is where the reasoning
/// for each one lives.
pub(crate) use crate::session::STRIPPED_ENV;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_found_despite_a_prompt_writing_a_terminal_title() {
        // A themed prompt writes an OSC title sequence that lands on the same
        // line as the answer. This is what an interactive zsh actually emits.
        let noisy = "\u{1b}]0;uwu\u{7}BEACON_RESOLVED=/bin/sh\n";
        assert_eq!(extract_resolved_path(noisy), Some(PathBuf::from("/bin/sh")));
    }

    #[test]
    fn a_program_the_shell_could_not_find_resolves_to_nothing() {
        assert_eq!(extract_resolved_path("BEACON_RESOLVED=\n"), None);
    }

    #[test]
    fn output_without_the_marker_resolves_to_nothing() {
        assert_eq!(
            extract_resolved_path("gitstatus failed to initialize\n"),
            None
        );
    }

    #[test]
    fn a_path_that_is_not_a_file_is_refused() {
        assert_eq!(
            extract_resolved_path("BEACON_RESOLVED=/definitely/not/here\n"),
            None
        );
    }

    #[test]
    fn the_last_marker_wins_when_a_shell_echoes_the_script() {
        let echoed = "BEACON_RESOLVED=$(command -v sh)\nBEACON_RESOLVED=/bin/sh\n";
        assert_eq!(
            extract_resolved_path(echoed),
            Some(PathBuf::from("/bin/sh"))
        );
    }
}
