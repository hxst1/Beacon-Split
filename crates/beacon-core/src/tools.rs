//! Finding the programs Beacon runs.
//!
//! Shared between spawning sessions and checking whether the machine has what
//! Beacon needs: both have to look in the same places, or a preflight check
//! would pass while the thing it checked still failed to start.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

pub fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    })
}

/// Marks the answer inside whatever else a shell writes on the way.
const PROBE_MARKER: &str = "BEACON_RESOLVED=";

/// How long a shell gets to answer before Beacon stops waiting for it.
///
/// An interactive shell is somebody's whole setup — prompt themes, version
/// managers, a git status daemon. Any of those can wedge, and a probe that
/// waits forever takes the window's first paint down with it. Four seconds is
/// far more than a healthy shell needs and short enough to not read as a hang.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);

/// Finds a program the way the user's own shell would.
///
/// A GUI application starts with a minimal PATH, so Beacon must not be pickier
/// about where `claude` lives than the terminal the user installed it from.
///
/// The interactive login shell is asked first, because that is the only one
/// that reads `.zshrc` — where a great many people, including anyone using a
/// framework or a version manager, set their PATH. A non-interactive login
/// shell is next, our own PATH after that, and the places installers actually
/// write to last, so a broken shell setup does not turn an installed program
/// into a missing one.
///
/// Runs once per program and is cached; it costs one short subprocess.
pub fn resolve_program(name: &str) -> Option<PathBuf> {
    for args in [&["-l", "-i", "-c"][..], &["-l", "-c"][..]] {
        if let Some(path) = ask_shell(name, args) {
            tracing::debug!(program = name, path = %path.display(), "resolved via login shell");
            return Some(path);
        }
    }

    // Beacon's own environment, which is enough when it was launched from a
    // shell that already had the program on its PATH.
    let on_our_path =
        std::env::var_os("PATH").and_then(|paths| find_in(std::env::split_paths(&paths), name));
    if let Some(path) = on_our_path {
        tracing::debug!(program = name, path = %path.display(), "resolved via our own PATH");
        return Some(path);
    }

    // Last resort: where installers put things. A prompt theme that wedges, or
    // a PATH set only in `.zshrc` while the shell we could ask is not
    // interactive, is not a reason to tell somebody their program is missing.
    let known = find_in(install_locations(), name);
    if let Some(path) = &known {
        tracing::debug!(program = name, path = %path.display(), "resolved via a known install location");
    } else {
        tracing::debug!(program = name, "not found anywhere Beacon looks");
    }
    known
}

/// Asks one shell where a program lives, and gives up if it will not say.
///
/// The answer goes to a file rather than to stdout, because stdout is shared
/// with everything the shell's startup prints — and worse, with any daemon it
/// leaves running, which holds the pipe open long after the shell is gone.
/// A file is still readable after the shell has been killed for taking too
/// long, so a setup that hangs *after* answering still counts as an answer.
fn ask_shell(name: &str, args: &[&str]) -> Option<PathBuf> {
    let answer = ProbeFile::new(name)?;
    let script = format!(
        "printf '{PROBE_MARKER}%s\\n' \"$(command -v {name} 2>/dev/null)\" > '{}' 2>/dev/null",
        answer.path.display()
    );

    let mut probe = std::process::Command::new(user_shell());
    probe
        .args(args)
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    strip_terminal_identity(&mut probe);

    let mut child = probe.spawn().ok()?;
    wait_briefly(&mut child, PROBE_TIMEOUT);

    extract_resolved_path(&std::fs::read_to_string(&answer.path).ok()?)
}

/// Waits for a probe, then stops waiting.
fn wait_briefly(child: &mut std::process::Child, limit: Duration) {
    let deadline = Instant::now() + limit;

    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => {}
        }

        if Instant::now() >= deadline {
            tracing::debug!("a probe shell would not finish; asking somewhere else instead");
            let _ = child.kill();
            let _ = child.wait();
            return;
        }

        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A scratch file for one probe's answer, removed when the probe is done.
struct ProbeFile {
    path: PathBuf,
}

impl ProbeFile {
    fn new(name: &str) -> Option<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let path = std::env::temp_dir().join(format!(
            "beacon-resolve-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));

        // A path we cannot quote plainly would be a shell injection waiting to
        // happen; there is no such temporary directory in practice, and
        // refusing one costs only this fallback.
        (!path.to_string_lossy().contains('\'')).then_some(Self { path })
    }
}

impl Drop for ProbeFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The first of these directories that holds the program.
fn find_in(dirs: impl IntoIterator<Item = PathBuf>, name: &str) -> Option<PathBuf> {
    dirs.into_iter()
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// Where the installers people actually use put their binaries.
///
/// Claude Code's own installer writes to `~/.local/bin`, which is on the PATH
/// of an interactive shell and nothing else — the exact gap this closes.
fn install_locations() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.extend([
            home.join(".local/bin"),
            home.join(".claude/local"),
            home.join("bin"),
            home.join(".bun/bin"),
            home.join(".npm-global/bin"),
            home.join(".volta/bin"),
            home.join("Library/pnpm"),
            home.join(".cargo/bin"),
        ]);

        // Anything installed globally under a node that nvm manages, which is
        // a different directory for every version of node they have.
        if let Ok(versions) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            dirs.extend(versions.flatten().map(|entry| entry.path().join("bin")));
        }
    }

    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]);

    dirs
}

/// Pulls the answer out of what a probe wrote.
///
/// The marker makes the answer findable regardless of what surrounds it: a
/// startup script that writes into the same place, or a shell that echoes the
/// script back, both leave the real answer last.
pub(crate) fn extract_resolved_path(written: &str) -> Option<PathBuf> {
    let answer = written
        .rmatch_indices(PROBE_MARKER)
        .map(|(index, _)| &written[index + PROBE_MARKER.len()..])
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

    #[test]
    fn a_shell_that_will_not_finish_is_given_up_on() {
        // The real case: a prompt theme whose git daemon wedges, so the shell
        // never reaches the question. Beacon must come back, not wait.
        let mut probe = std::process::Command::new("/bin/sh");
        probe
            .args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let mut child = probe.spawn().expect("sh should be runnable");
        let started = Instant::now();
        wait_briefly(&mut child, Duration::from_millis(200));

        assert!(
            started.elapsed() < Duration::from_secs(5),
            "waited too long"
        );
        assert!(
            child.try_wait().unwrap().is_some(),
            "child was left running"
        );
    }

    #[test]
    fn a_shell_that_answers_and_then_hangs_still_counts_as_an_answer() {
        // Writing to a file rather than a pipe is what makes this work: the
        // answer survives the shell being killed for taking too long.
        assert_eq!(
            resolve_via_hanging_shell("sh"),
            Some(PathBuf::from("/bin/sh"))
        );
    }

    fn resolve_via_hanging_shell(name: &str) -> Option<PathBuf> {
        let answer = ProbeFile::new(name)?;
        let script = format!(
            "printf 'BEACON_RESOLVED=/bin/{name}\\n' > '{}'; sleep 30",
            answer.path.display()
        );

        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", &script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        wait_briefly(&mut child, Duration::from_millis(400));

        extract_resolved_path(&std::fs::read_to_string(&answer.path).ok()?)
    }

    #[test]
    fn a_program_is_found_where_installers_put_it() {
        // /bin is one of the places we look, and every machine that runs this
        // has sh in it.
        assert_eq!(
            find_in(install_locations(), "sh"),
            Some(PathBuf::from("/bin/sh"))
        );
    }

    #[test]
    fn nothing_is_found_for_a_program_nobody_installed() {
        assert_eq!(find_in(install_locations(), "beacon-no-such-program"), None);
    }

    #[test]
    fn a_probe_file_is_cleaned_up_after_itself() {
        let path = {
            let answer = ProbeFile::new("sh").expect("temp dir should be usable");
            std::fs::write(&answer.path, "BEACON_RESOLVED=/bin/sh\n").unwrap();
            assert!(answer.path.is_file());
            answer.path.clone()
        };

        assert!(!path.exists(), "the probe left its scratch file behind");
    }

    #[test]
    fn two_probes_never_share_a_scratch_file() {
        let first = ProbeFile::new("claude").unwrap();
        let second = ProbeFile::new("claude").unwrap();
        assert_ne!(first.path, second.path);
    }
}
