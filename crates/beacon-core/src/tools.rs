//! Finding the programs Beacon runs.
//!
//! Shared between spawning sessions and checking whether the machine has what
//! Beacon needs: both have to look in the same places, or a preflight check
//! would pass while the thing it checked still failed to start.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

/// The shell a terminal runs when the user has not chosen one.
///
/// On unix, the account's shell. Windows has no such setting — `SHELL`, when
/// it is set at all, is something Git Bash or MSYS left behind and names a
/// path only they understand — so it is PowerShell: version 7 when it has been
/// installed, because nobody installs it to keep using the old one, and
/// Windows PowerShell otherwise, which every Windows has.
pub fn user_shell() -> String {
    if cfg!(windows) {
        let on_path = std::env::var_os("PATH")
            .and_then(|paths| find_in(std::env::split_paths(&paths), "pwsh"));
        return on_path
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(windows_powershell);
    }

    std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/bin/zsh".to_string()
        } else {
            "/bin/bash".to_string()
        }
    })
}

/// How [`user_shell`] is started so it is the shell any other terminal gives.
///
/// A login shell on unix, which is what reads the profile that sets the PATH.
/// PowerShell reads its profile anyway; `-NoLogo` only drops the banner, which
/// every new tab would otherwise open with.
pub fn user_shell_args() -> &'static [&'static str] {
    if cfg!(windows) { &["-NoLogo"] } else { &["-l"] }
}

/// Windows PowerShell, by full path, so a PATH that has lost System32 does not
/// leave a terminal with nothing to run.
fn windows_powershell() -> String {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "powershell.exe".to_string())
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
/// Windows has no login shell to ask, and does not need one: an application
/// started from the Start menu inherits the PATH the user set, which is the one
/// every terminal there reads too. So it starts at our own PATH. What it finds
/// may be npm's `.cmd` shim rather than a program, and [`launchable`] looks
/// through that.
///
/// Runs once per program and is cached; it costs one short subprocess.
pub fn resolve_program(name: &str) -> Option<PathBuf> {
    resolve_anywhere(name).map(launchable)
}

fn resolve_anywhere(name: &str) -> Option<PathBuf> {
    let shell_answers: &[&[&str]] = if cfg!(unix) {
        &[&["-l", "-i", "-c"], &["-l", "-c"]]
    } else {
        &[]
    };
    for args in shell_answers {
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

/// The program behind what was found, when what was found is npm's shim.
///
/// A package installed with `npm install -g` is reached on Windows through a
/// `.cmd` file that runs the real thing. Only cmd.exe can run one, and it
/// re-reads every argument by its own rules — quotes, `^`, `%`, and no
/// newlines at all — which is exactly what a session's `--agents` JSON and its
/// multi-line `--append-system-prompt` are made of. So the shim is read rather
/// than run: the line that starts the program names it relative to the shim,
/// and when that is an executable, the executable is what Beacon runs. Claude
/// Code's npm package ships one, so for it the shim costs nothing.
///
/// Anything else is returned as it was found: a shim that runs a script still
/// starts, through cmd.exe, and only arguments cmd mangles suffer.
pub fn launchable(path: PathBuf) -> PathBuf {
    let is_batch = path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
    });
    if !cfg!(windows) || !is_batch {
        return path;
    }

    match npm_shim_target(&path) {
        Some(target) => {
            tracing::debug!(shim = %path.display(), program = %target.display(), "looked through an npm shim");
            target
        }
        None => path,
    }
}

/// The executable an npm `.cmd` shim starts, if that is what it starts.
///
/// cmd-shim writes the target as `"%dp0%\…"`, and older npm as `"%~dp0\…"`;
/// both mean "the shim's own directory".
fn npm_shim_target(shim: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(shim).ok()?;
    let directory = shim.parent()?;

    text.lines().rev().find_map(|line| {
        let start = ["\"%dp0%\\", "\"%~dp0\\"]
            .iter()
            .find_map(|prefix| line.find(prefix).map(|at| at + prefix.len()))?;
        let rest = &line[start..];
        let target = directory.join(&rest[..rest.find('"')?]);

        let executable = target
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
        (executable && target.is_file()).then_some(target)
    })
}

/// Git for Windows' bash, which Claude Code runs its commands through there.
///
/// Found the way Claude Code finds it: the path it is told in
/// `CLAUDE_CODE_GIT_BASH_PATH`, or the `bin\bash.exe` of the Git that is on the
/// PATH — `git.exe` itself sits in `cmd\` or `mingw64\bin\` beside it.
pub fn git_bash() -> Option<PathBuf> {
    let told = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH")
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    if told.is_some() {
        return told;
    }

    let git = resolve_program("git")?;
    git.ancestors()
        .skip(1)
        .take(3)
        .map(|dir| dir.join("bin").join("bash.exe"))
        .find(|path| path.is_file())
}

/// Keeps a background subprocess from opening a console window.
///
/// Beacon's window and its daemon have no console. On Windows a console
/// program started from either gets one of its own — a window that flashes up
/// for every git status and every version check — unless asked not to. Not
/// for sessions: those get the pseudo-console their PTY provides.
pub fn hide_console_window(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
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

/// Runs a program for what it prints, and gives up if it will not finish.
///
/// `Command::output()` would be shorter and would block forever on a program
/// that never exits — which is the wrong trade on a path that runs while a
/// session is being started. Returns `None` when the program could not be run,
/// would not finish, or failed; every caller here treats "no answer" as "cannot
/// do that", which hides a feature rather than breaking one.
///
/// Output is drained on its own thread for the same reason git's is: a parent
/// that waits for exit while the pipe is full waits forever on a child that is
/// waiting to be read.
pub fn capture_briefly(command: &mut std::process::Command, limit: Duration) -> Option<String> {
    hide_console_window(command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        use std::io::Read;
        let mut stdout = stdout;
        let _ = stdout.read_to_string(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Err(_) => return None,
            Ok(None) => {}
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            // The reader thread is abandoned rather than joined: whatever is
            // holding the program up may be holding the pipe open too.
            return None;
        }

        std::thread::sleep(Duration::from_millis(10));
    };

    status.success().then(|| reader.join().ok())?
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
///
/// On Windows a program is found by name plus one of `PATHEXT`'s extensions,
/// directory by directory, as the system itself searches. The bare name is not
/// enough and must not count: npm leaves an extensionless shell script called
/// `claude` beside `claude.cmd`, which is a file and cannot be run.
fn find_in(dirs: impl IntoIterator<Item = PathBuf>, name: &str) -> Option<PathBuf> {
    let candidates = executable_names(name);
    dirs.into_iter()
        .flat_map(|dir| {
            candidates
                .iter()
                .map(move |candidate| dir.join(candidate))
                .collect::<Vec<_>>()
        })
        .find(|path| path.is_file())
}

/// The file names a program called `name` may have on this platform.
fn executable_names(name: &str) -> Vec<String> {
    if !cfg!(windows) {
        return vec![name.to_string()];
    }

    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let already_has_one = extensions.split(';').any(|extension| {
        !extension.is_empty()
            && name
                .to_ascii_uppercase()
                .ends_with(&extension.to_ascii_uppercase())
    });
    if already_has_one {
        return vec![name.to_string()];
    }

    extensions
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{name}{}", extension.to_ascii_lowercase()))
        .collect()
}

/// Where the installers people actually use put their binaries.
///
/// Claude Code's own installer writes to `~/.local/bin`, which is on the PATH
/// of an interactive shell and nothing else — the exact gap this closes.
fn install_locations() -> Vec<PathBuf> {
    if cfg!(windows) {
        return windows_install_locations();
    }

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

/// The same, for Windows.
///
/// Only consulted when the PATH Beacon inherited does not have the program —
/// which happens when it was installed after the daemon started, since a
/// running process never sees PATH change. Claude Code's installer writes to
/// `~/.local/bin` here too; `winget` links portable programs into one folder;
/// npm, pnpm, Volta, Scoop and Git for Windows each have their own.
fn windows_install_locations() -> Vec<PathBuf> {
    let home = crate::paths::home_dir();
    let from_env = |key: &str| std::env::var_os(key).map(PathBuf::from);

    let mut dirs = vec![
        home.join(".local").join("bin"),
        home.join(".bun").join("bin"),
        home.join(".cargo").join("bin"),
        home.join("scoop").join("shims"),
    ];
    if let Some(roaming) = from_env("APPDATA") {
        dirs.push(roaming.join("npm"));
    }
    if let Some(local) = from_env("LOCALAPPDATA") {
        dirs.extend([
            local.join("Microsoft").join("WinGet").join("Links"),
            local.join("pnpm"),
            local.join("Volta").join("bin"),
            local.join("Programs").join("Git").join("cmd"),
        ]);
    }
    if let Some(programs) = from_env("ProgramFiles") {
        dirs.push(programs.join("Git").join("cmd"));
    }
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

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[test]
    fn the_last_marker_wins_when_a_shell_echoes_the_script() {
        let echoed = "BEACON_RESOLVED=$(command -v sh)\nBEACON_RESOLVED=/bin/sh\n";
        assert_eq!(
            extract_resolved_path(echoed),
            Some(PathBuf::from("/bin/sh"))
        );
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
    #[test]
    fn a_shell_that_answers_and_then_hangs_still_counts_as_an_answer() {
        // Writing to a file rather than a pipe is what makes this work: the
        // answer survives the shell being killed for taking too long.
        assert_eq!(
            resolve_via_hanging_shell("sh"),
            Some(PathBuf::from("/bin/sh"))
        );
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
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

    /// npm leaves an extensionless shell script beside its `.cmd` shim. It is
    /// a file, and it cannot be run on Windows; finding it would be finding
    /// nothing.
    #[cfg(windows)]
    #[test]
    fn a_program_is_found_by_its_windows_name_and_never_as_a_bare_script() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tool"), "#!/bin/sh\n").unwrap();
        std::fs::write(dir.path().join("tool.cmd"), "@echo off\r\n").unwrap();

        let found = find_in([dir.path().to_path_buf()], "tool").expect("the shim");
        assert!(
            found
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd")),
            "found {}",
            found.display()
        );
    }

    /// The shape `npm install -g` writes for a package whose bin is a program,
    /// which is what Claude Code's package is.
    #[cfg(windows)]
    #[test]
    fn an_npm_shim_is_looked_through_to_the_program_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("node_modules").join("pkg").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tool.exe"), "").unwrap();

        let shim = dir.path().join("tool.cmd");
        std::fs::write(
            &shim,
            "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\n\
             SETLOCAL\r\nCALL :find_dp0\r\n\"%dp0%\\node_modules\\pkg\\bin\\tool.exe\"   %*\r\n",
        )
        .unwrap();

        assert_eq!(launchable(shim), bin.join("tool.exe"));
    }

    /// A shim that starts a script through node has no program to look
    /// through to, and is left to cmd.exe to run.
    #[cfg(windows)]
    #[test]
    fn a_shim_that_runs_a_script_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("tool.cmd");
        std::fs::write(
            &shim,
            "@ECHO off\r\n\"%_prog%\"  \"%dp0%\\node_modules\\pkg\\cli.js\" %*\r\n",
        )
        .unwrap();

        assert_eq!(launchable(shim.clone()), shim);
    }
}
