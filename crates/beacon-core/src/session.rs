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
use crate::settings::ShellSpec;
use crate::tools::{resolve_program, user_shell};

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
pub(crate) const STRIPPED_ENV: &[&str] = &[
    // Terminal identity. See the note above.
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
    // Injected by a package script, not by the user.
    "INIT_CWD",
    "NODE_ENV",
    // Claude Code's own per-process state, when Beacon was launched from
    // inside a session. Without this the `claude` Beacon starts sees the
    // parent's CLAUDE_CODE_CHILD_SESSION marker, concludes it is a nested
    // session, and turns transcript saving off. The messaging socket and token
    // are the parent's private channel and have no business in a project shell.
    //
    // Only per-process state is listed. Configuration such as ANTHROPIC_API_KEY,
    // ANTHROPIC_BASE_URL or CLAUDE_CODE_USE_BEDROCK belongs to the user and is
    // deliberately passed through, which is why this is a list and not a
    // CLAUDE_* prefix rule.
    "CLAUDECODE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
];

/// Writes the MCP configuration Claude sessions are started with, and returns
/// where it went.
///
/// Beacon deliberately does not register this server in the user's own Claude
/// configuration. It is passed per session with `--mcp-config`, which means
/// nothing is installed, nothing needs uninstalling, a Beacon that is deleted
/// leaves no trace in `~/.claude.json`, and a Claude the user runs in their own
/// terminal is completely unaffected. The cost is that clips only work in
/// sessions Beacon started — which is the whole scope of the feature.
///
/// No environment is declared here: the server needs `BEACON_SOCKET` and
/// `BEACON_PROJECT`, and it gets them by being a child of a session that
/// already has them. Writing them into this file instead would freeze one
/// project's id into a file every project's session reads.
fn write_mcp_config(dir: &Path) -> std::io::Result<PathBuf> {
    let binary = std::env::current_exe()?;
    let path = dir.join("mcp.json");

    let config = serde_json::json!({
        "mcpServers": {
            "beacon": {
                "type": "stdio",
                "command": binary,
                "args": ["mcp"],
            }
        }
    });

    // Through a temporary file: a session starting while this is being written
    // would otherwise read half a document and start with no clip tool at all.
    let temporary = dir.join("mcp.json.tmp");
    std::fs::write(&temporary, serde_json::to_vec(&config)?)?;
    std::fs::rename(&temporary, &path)?;
    Ok(path)
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
    ensure_utf8_locale(command);
}

/// Promises the session a UTF-8 world.
///
/// Launched from the Dock, Beacon inherits no locale at all: setting one is
/// Terminal's doing, not the system's. A session that inherits nothing leaves
/// every tool guessing, and on macOS the guess is Mac OS Roman, which is how
/// `pbcopy` turns an accent into two bytes of noise on its way to the
/// clipboard. We only fill the gap; a locale the user chose is left alone.
fn ensure_utf8_locale(command: &mut CommandBuilder) {
    let chosen = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()));
    if !chosen {
        command.env("LANG", "en_US.UTF-8");
    }
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
///
/// Deserializable because it crosses the daemon socket in both directions, not
/// only from the backend to the webview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: SessionId,
    pub project: ProjectId,
    pub kind: SessionKind,
    /// Which of a project's sessions of this kind. Claude has one; terminals
    /// can have several, and this is how they are told apart across restarts.
    pub slot: u32,
    pub cwd: String,
    pub running: bool,
}

struct Session {
    project: ProjectId,
    kind: SessionKind,
    slot: u32,
    cwd: PathBuf,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    scrollback: Arc<Mutex<Scrollback>>,
    /// The last size the process was told about.
    size: (u16, u16),
}

/// Owns every live PTY.
///
/// This is the piece that moves into a background daemon in Milestone 7, which
/// is why it takes its event sink as a trait object and holds no reference to a
/// window, a webview, or Tauri.
pub struct SessionManager {
    events: Arc<dyn SessionEvents>,
    sessions: Mutex<HashMap<SessionId, Session>>,
    /// One session per (project, kind, slot), so switching tabs reuses rather
    /// than respawns, and a project can hold several terminals at once.
    by_project: Mutex<HashMap<(ProjectId, SessionKind, u32), SessionId>>,
    /// Where `claude` lives, worked out once. `None` means it was looked for
    /// and not found.
    claude_path: OnceLock<Option<PathBuf>>,
    /// The socket a Claude session's hooks should report to.
    ///
    /// Only the daemon knows this, and only Claude sessions are told: a shell
    /// has no reason to be able to reach the daemon that spawned it.
    hook_socket: Mutex<Option<PathBuf>>,
    /// The MCP configuration handed to every Claude session, written beside the
    /// socket. `None` until the socket is known, or if it could not be written.
    mcp_config: Mutex<Option<PathBuf>>,
}

impl SessionManager {
    pub fn new(events: Arc<dyn SessionEvents>) -> Self {
        Self {
            events,
            sessions: Mutex::new(HashMap::new()),
            by_project: Mutex::new(HashMap::new()),
            claude_path: OnceLock::new(),
            hook_socket: Mutex::new(None),
            mcp_config: Mutex::new(None),
        }
    }

    /// Tells the manager where Claude Code's hooks should report.
    ///
    /// Also writes the MCP configuration, which lives beside the socket for the
    /// same reason the socket does: it is runtime state that names this
    /// daemon's binary, it should not be synced, and it should not survive a
    /// reboot.
    pub fn set_hook_socket(&self, socket: PathBuf) {
        let config = socket.parent().and_then(|dir| match write_mcp_config(dir) {
            Ok(path) => Some(path),
            Err(err) => {
                // Not fatal. Sessions still start, hooks still report, and the
                // only thing missing is the clip drawer.
                tracing::warn!(error = %err, "could not write the MCP configuration");
                None
            }
        });

        *self.hook_socket.lock_or_recover() = Some(socket);
        *self.mcp_config.lock_or_recover() = config;
    }

    /// The command for a session kind.
    ///
    /// Shells run as login shells, like every terminal emulator: without that a
    /// GUI application's PATH is missing most of the user's tools. Claude is
    /// launched directly from its resolved path, so nothing the user's startup
    /// files print ends up in the panel above it.
    fn command_for(&self, kind: SessionKind, shell: Option<&ShellSpec>) -> Result<CommandBuilder> {
        match kind {
            SessionKind::Shell => {
                // What the user configured, or their account's shell as a login
                // shell — which is what every terminal emulator does, and
                // without it a GUI application's PATH is missing most of their
                // tools.
                let mut command = match shell {
                    Some(spec) => {
                        let mut command = CommandBuilder::new(&spec.program);
                        for arg in &spec.args {
                            command.arg(arg);
                        }
                        command
                    }
                    None => {
                        let mut command = CommandBuilder::new(user_shell());
                        command.arg("-l");
                        command
                    }
                };
                let _ = &mut command;
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
                let mut command = CommandBuilder::new(path);

                // Merged with whatever the user has configured, never replacing
                // it: `--strict-mcp-config` would silently switch off every MCP
                // server they set up themselves, which is not a trade Beacon
                // gets to make on their behalf for a drawer.
                if let Some(config) = self.mcp_config.lock_or_recover().as_ref() {
                    // `--mcp-config` takes a *list*, so the separated form
                    // swallows whatever argument comes after it. Nothing does
                    // today; writing it joined means nothing ever can.
                    command.arg(format!("--mcp-config={}", config.display()));
                }

                Ok(command)
            }
        }
    }

    /// Returns the project's existing session of this kind, spawning one if it
    /// has none or if the previous process has exited.
    pub fn ensure(
        &self,
        project: &ProjectId,
        kind: SessionKind,
        slot: u32,
        cwd: &Path,
        size: (u16, u16),
        shell: Option<&ShellSpec>,
    ) -> Result<SessionId> {
        let key = (project.clone(), kind, slot);

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

        let id = self.spawn(project.clone(), kind, slot, cwd, size, shell)?;
        self.by_project.lock_or_recover().insert(key, id.clone());
        Ok(id)
    }

    fn spawn(
        &self,
        project: ProjectId,
        kind: SessionKind,
        slot: u32,
        cwd: &Path,
        (cols, rows): (u16, u16),
        shell: Option<&ShellSpec>,
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

        let mut command = self.command_for(kind, shell)?;
        command.cwd(cwd);
        prepare_environment(&mut command);

        // A Claude session is told how to reach us, so its hooks can say what
        // it is doing. Without these the hook is inert, which is what makes it
        // safe to register once and forget about.
        if kind == SessionKind::Claude
            && let Some(socket) = self.hook_socket.lock_or_recover().as_ref()
        {
            command.env("BEACON_SOCKET", socket);
            command.env("BEACON_PROJECT", project.as_str());
        }

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
                slot,
                cwd: cwd.to_path_buf(),
                master: pair.master,
                writer,
                child,
                scrollback,
                size: (cols, rows),
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
        // A grid this small is a client that measured a panel mid-layout, not a
        // window someone actually made that narrow. Honour it — the client is
        // entitled to be believed — but say so, because the symptom is output
        // wrapped at two columns and nothing else would point here.
        if cols < 20 || rows < 4 {
            tracing::warn!(session = %id, cols, rows, "implausibly small terminal size");
        }

        let mut sessions = self.sessions.lock_or_recover();
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| CoreError::SessionNotFound(id.to_string()))?;

        // Only when it actually changes. A mismatch between what the process
        // believes and what is on screen is invisible until output arrives
        // scrambled, so the size it was last told is worth having in the log.
        if session.size != (cols, rows) {
            tracing::info!(session = %id, cols, rows, "terminal resized");
            session.size = (cols, rows);
        }

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

    /// How many sessions are alive, for deciding whether the daemon has work.
    pub fn count(&self) -> usize {
        self.sessions.lock_or_recover().len()
    }

    /// Every live session, so a reattaching client can find its work again.
    pub fn list(&self) -> Vec<SessionInfo> {
        let mut sessions = self.sessions.lock_or_recover();
        let ids: Vec<SessionId> = sessions.keys().cloned().collect();
        ids.iter()
            .filter_map(|id| {
                let session = sessions.get_mut(id)?;
                Some(SessionInfo {
                    id: id.clone(),
                    project: session.project.clone(),
                    kind: session.kind,
                    slot: session.slot,
                    cwd: session.cwd.to_string_lossy().into_owned(),
                    running: session.child.try_wait().ok().flatten().is_none(),
                })
            })
            .collect()
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
            slot: session.slot,
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
        slot: u32,
        cwd: &Path,
        size: (u16, u16),
        shell: Option<&ShellSpec>,
    ) -> Result<SessionId> {
        let existing = self
            .by_project
            .lock_or_recover()
            .get(&(project.clone(), kind, slot))
            .cloned();

        if let Some(id) = existing {
            // Already gone is not a failure: the point is to end up running.
            let _ = self.close(&id);
        }

        self.ensure(project, kind, slot, cwd, size, shell)
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
