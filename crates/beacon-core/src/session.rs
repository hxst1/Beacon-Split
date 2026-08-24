use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::ProjectId;
use crate::error::{CoreError, Result};
use crate::scrollback::{DEFAULT_CAPACITY, Scrollback};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

impl SessionId {
    fn generate() -> Self {
        Self(format!("sn_{}", Uuid::new_v4().simple()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What runs inside a session.
///
/// Both are just processes in a PTY — Beacon does not reimplement Claude Code,
/// it runs the real CLI the same way it runs a shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionKind {
    Shell,
    Claude,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Claude => "claude",
        }
    }
}

fn user_shell() -> String {
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
fn resolve_program(name: &str) -> Option<PathBuf> {
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
fn extract_resolved_path(stdout: &str) -> Option<PathBuf> {
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

/// Environment variables a spawned session must not inherit from whatever
/// launched Beacon.
///
/// The terminal identity ones matter most: with `TERM_PROGRAM=Apple_Terminal`
/// still set, macOS's `/etc/zshrc` engages Terminal.app's session save and
/// restore and sources `~/.zsh_sessions/$TERM_SESSION_ID.session`, a file that
/// belongs to a different terminal and may not exist. Beacon is not that
/// terminal and must not claim to be.
///
/// The `npm_*` group is a development-time leak: launching Beacon through a
/// package script would otherwise push that script's configuration into every
/// project shell.
const STRIPPED_ENV: &[&str] = &[
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "SHELL_SESSION_FILE",
    "SHELL_SESSION_DID_INIT",
    "ITERM_PROFILE",
    "ITERM_SESSION_ID",
    // Stale geometry from the launching terminal; the PTY sets the real size.
    "COLUMNS",
    "LINES",
    "INIT_CWD",
    "NODE_ENV",
];

/// Strips the launching terminal's identity from a probe subprocess.
///
/// The same reasoning as [`prepare_environment`]: asking the shell a question
/// while pretending to be Terminal.app runs that terminal's session machinery,
/// which prints into the answer.
fn strip_terminal_identity(command: &mut std::process::Command) {
    for key in STRIPPED_ENV {
        command.env_remove(key);
    }
}

/// Gives the session a clean, honest environment.
fn prepare_environment(command: &mut CommandBuilder) {
    for key in STRIPPED_ENV {
        command.env_remove(key);
    }
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("npm_") {
            command.env_remove(&key);
        }
    }

    // Tell the program it is talking to a capable terminal; xterm.js is.
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    // Identify ourselves, so anything that adapts to its terminal can.
    command.env("TERM_PROGRAM", "Beacon");
    command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
}

/// How the host is told about things the session does on its own.
///
/// Implemented by the Tauri layer today (which forwards to the webview) and by
/// the daemon's transport later. `beacon-core` never learns which.
pub trait SessionEvents: Send + Sync + 'static {
    /// `offset` is where this chunk starts in the session's lifetime stream, so
    /// a client that replayed a snapshot can tell what it has already seen.
    ///
    /// The project travels with the event so a listener can tell which tab just
    /// did something without keeping its own session-to-project map.
    fn output(&self, id: &SessionId, project: &ProjectId, offset: u64, bytes: &[u8]);
    fn exited(&self, id: &SessionId, project: &ProjectId, code: Option<i32>);
}

/// A session as the UI sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: SessionId,
    pub project: ProjectId,
    pub kind: SessionKind,
    pub cwd: String,
    pub running: bool,
}

struct Session {
    project: ProjectId,
    kind: SessionKind,
    cwd: PathBuf,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    scrollback: Arc<Mutex<Scrollback>>,
}

/// Owns every live PTY.
///
/// This is the piece that moves into a background daemon in Milestone 7, which
/// is why it takes its event sink as a trait object and holds no reference to a
/// window, a webview, or Tauri.
pub struct SessionManager {
    events: Arc<dyn SessionEvents>,
    sessions: Mutex<HashMap<SessionId, Session>>,
    /// One session per (project, kind), so switching tabs reuses rather than
    /// respawns.
    by_project: Mutex<HashMap<(ProjectId, SessionKind), SessionId>>,
    /// Where `claude` lives, worked out once. `None` means it was looked for
    /// and not found.
    claude_path: OnceLock<Option<PathBuf>>,
}

impl SessionManager {
    pub fn new(events: Arc<dyn SessionEvents>) -> Self {
        Self {
            events,
            sessions: Mutex::new(HashMap::new()),
            by_project: Mutex::new(HashMap::new()),
            claude_path: OnceLock::new(),
        }
    }

    /// The command for a session kind.
    ///
    /// Shells run as login shells, like every terminal emulator: without that a
    /// GUI application's PATH is missing most of the user's tools. Claude is
    /// launched directly from its resolved path, so nothing the user's startup
    /// files print ends up in the panel above it.
    fn command_for(&self, kind: SessionKind) -> Result<CommandBuilder> {
        match kind {
            SessionKind::Shell => {
                let mut command = CommandBuilder::new(user_shell());
                command.arg("-l");
                Ok(command)
            }
            SessionKind::Claude => {
                let path = self
                    .claude_path
                    .get_or_init(|| resolve_program("claude"))
                    .as_ref()
                    .ok_or_else(|| {
                        CoreError::invalid(
                            "could not find the claude command. Install Claude Code, or make sure \
                             it is on the PATH your login shell sets.",
                        )
                    })?;
                Ok(CommandBuilder::new(path))
            }
        }
    }

    /// Returns the project's existing session of this kind, spawning one if it
    /// has none or if the previous process has exited.
    pub fn ensure(
        &self,
        project: &ProjectId,
        kind: SessionKind,
        cwd: &Path,
        size: (u16, u16),
    ) -> Result<SessionId> {
        let key = (project.clone(), kind);

        if let Some(existing) = self.by_project.lock_or_recover().get(&key).cloned() {
            let mut sessions = self.sessions.lock_or_recover();
            let alive = sessions
                .get_mut(&existing)
                .is_some_and(|session| session.child.try_wait().ok().flatten().is_none());

            if alive {
                // Still running — hand back the same session.
                return Ok(existing);
            }
            // Exited while we were away; drop it and start fresh below.
            sessions.remove(&existing);
        }

        let id = self.spawn(project.clone(), kind, cwd, size)?;
        self.by_project.lock_or_recover().insert(key, id.clone());
        Ok(id)
    }

    fn spawn(
        &self,
        project: ProjectId,
        kind: SessionKind,
        cwd: &Path,
        (cols, rows): (u16, u16),
    ) -> Result<SessionId> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| CoreError::session("could not open a pty", err))?;

        let mut command = self.command_for(kind)?;
        command.cwd(cwd);
        prepare_environment(&mut command);

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|err| CoreError::session("could not start the session", err))?;
        // The slave must be closed here or the reader never sees EOF when the
        // child exits.
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| CoreError::session("could not read from the pty", err))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|err| CoreError::session("could not write to the pty", err))?;

        let id = SessionId::generate();
        let scrollback = Arc::new(Mutex::new(Scrollback::new(DEFAULT_CAPACITY)));

        {
            // The PTY read is blocking, so it gets its own thread. It ends when
            // the child closes the pty, which is also how we learn it exited.
            let id = id.clone();
            let owner = project.clone();
            let events = Arc::clone(&self.events);
            let scrollback = Arc::clone(&scrollback);
            std::thread::Builder::new()
                .name(format!("pty-{id}"))
                .spawn(move || {
                    let mut chunk = [0u8; 8 * 1024];
                    loop {
                        match reader.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                let bytes = &chunk[..n];
                                // Recording and numbering happen under one lock
                                // so a snapshot can never interleave with this.
                                let offset = scrollback.lock_or_recover().push(bytes);
                                events.output(&id, &owner, offset, bytes);
                            }
                            Err(err) => {
                                tracing::debug!(session = %id, error = %err, "pty read ended");
                                break;
                            }
                        }
                    }
                    events.exited(&id, &owner, None);
                })
                .map_err(|err| CoreError::session("could not start the reader thread", err))?;
        }

        tracing::info!(session = %id, ?kind, cwd = %cwd.display(), "session started");

        self.sessions.lock_or_recover().insert(
            id.clone(),
            Session {
                project,
                kind,
                cwd: cwd.to_path_buf(),
                master: pair.master,
                writer,
                child,
                scrollback,
            },
        );

        Ok(id)
    }

    /// Forwards keystrokes to the process.
    ///
    /// Nothing is logged here: this carries whatever the user types, which
    /// includes secrets.
    pub fn write(&self, id: &SessionId, bytes: &[u8]) -> Result<()> {
        let mut sessions = self.sessions.lock_or_recover();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| CoreError::SessionNotFound(id.to_string()))?;
        session
            .writer
            .write_all(bytes)
            .map_err(|err| CoreError::session("could not write to the session", err))?;
        session
            .writer
            .flush()
            .map_err(|err| CoreError::session("could not flush the session", err))
    }

    pub fn resize(&self, id: &SessionId, cols: u16, rows: u16) -> Result<()> {
        let sessions = self.sessions.lock_or_recover();
        let session = sessions
            .get(id)
            .ok_or_else(|| CoreError::SessionNotFound(id.to_string()))?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| CoreError::session("could not resize the session", err))
    }

    /// Everything the session has produced, plus the stream offset just past
    /// it, for rebuilding a terminal view without losing or repeating output.
    pub fn scrollback(&self, id: &SessionId) -> Result<(Vec<u8>, u64)> {
        let sessions = self.sessions.lock_or_recover();
        let session = sessions
            .get(id)
            .ok_or_else(|| CoreError::SessionNotFound(id.to_string()))?;
        Ok(session.scrollback.lock_or_recover().snapshot())
    }

    pub fn info(&self, id: &SessionId) -> Result<SessionInfo> {
        let mut sessions = self.sessions.lock_or_recover();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| CoreError::SessionNotFound(id.to_string()))?;
        Ok(SessionInfo {
            id: id.clone(),
            project: session.project.clone(),
            kind: session.kind,
            cwd: session.cwd.to_string_lossy().into_owned(),
            running: session.child.try_wait().ok().flatten().is_none(),
        })
    }

    /// Stops a session's process and forgets it.
    pub fn close(&self, id: &SessionId) -> Result<()> {
        let removed = self.sessions.lock_or_recover().remove(id);
        let Some(mut session) = removed else {
            return Err(CoreError::SessionNotFound(id.to_string()));
        };

        self.by_project
            .lock_or_recover()
            .retain(|_, value| value != id);

        if let Err(err) = session.child.kill() {
            tracing::warn!(session = %id, error = %err, "could not kill session process");
        }
        let _ = session.child.wait();
        tracing::info!(session = %id, "session closed");
        Ok(())
    }

    /// Stops every session belonging to a project.
    pub fn close_project(&self, project: &ProjectId) -> Result<()> {
        let ids: Vec<SessionId> = self
            .sessions
            .lock_or_recover()
            .iter()
            .filter(|(_, session)| &session.project == project)
            .map(|(id, _)| id.clone())
            .collect();

        for id in ids {
            self.close(&id)?;
        }
        Ok(())
    }

    /// Restarts a project's session of a given kind, starting one if it had
    /// none.
    ///
    /// Addressed by project rather than by session id: the caller wants "give
    /// this project a fresh Claude", and should not have to know which session
    /// that replaces.
    pub fn restart_for(
        &self,
        project: &ProjectId,
        kind: SessionKind,
        cwd: &Path,
        size: (u16, u16),
    ) -> Result<SessionId> {
        let existing = self
            .by_project
            .lock_or_recover()
            .get(&(project.clone(), kind))
            .cloned();

        if let Some(id) = existing {
            // Already gone is not a failure: the point is to end up running.
            let _ = self.close(&id);
        }

        self.ensure(project, kind, cwd, size)
    }
}

/// Locks that recover from a panic elsewhere instead of poisoning the app.
///
/// A panic in one session's thread must not take every other session with it.
trait LockOrRecover<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockOrRecover<T> for Mutex<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

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
