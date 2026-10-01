use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::ProjectId;
use crate::error::{CoreError, Result};
use crate::scrollback::{DEFAULT_CAPACITY, Scrollback};
use crate::settings::ShellSpec;
use crate::tools::{resolve_program, user_shell, user_shell_args};

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

/// What a session should be started as.
///
/// Bundled rather than passed one by one, because the list was going to keep
/// growing and a call with eight positional arguments says nothing about which
/// is which. Sent by the client rather than read by the daemon, like the shell
/// alone used to be and for the same reason: a session starts with the
/// preferences set now, not the ones set when the daemon happened to start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionPrefs {
    /// `None` means the account's own shell. Ignored for a Claude session.
    pub shell: Option<ShellSpec>,
    /// Whether Beacon's own subagents are offered. Ignored for a shell.
    pub agents: bool,
}

/// How a Claude session should be started.
///
/// Beacon chooses the conversation's id rather than discovering it, so this is
/// settled before the process exists and nothing ever has to read a transcript
/// to find out what it is talking to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeLaunch {
    /// The conversation, as a UUID — the form `--session-id` accepts.
    pub session_id: String,
    /// What to call it, when it has been called something.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub start: ClaudeStart,
    /// Whether Beacon's own subagents and routing policy are offered.
    ///
    /// Sent by the client rather than read by the daemon, like the shell and
    /// for the same reason: a session starts with the preference set now, not
    /// the one that happened to be set when the daemon started.
    #[serde(default)]
    pub agents: bool,
}

/// Whether the conversation being started already exists.
///
/// The distinction is not cosmetic: `--session-id` on a conversation that has
/// already been used is refused — *"Session ID … is already in use"* — so a
/// Claude that crashed and is being brought back has to be resumed, not
/// started. Getting this wrong would turn every restart into an error message
/// where the session used to be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ClaudeStart {
    /// A conversation that does not exist yet.
    New,
    /// One that does: every start after the first, including after a crash.
    Resume,
    /// A new conversation carrying another's history.
    #[serde(rename_all = "camelCase")]
    Fork { from: String },
}

impl ClaudeLaunch {
    /// The arguments Claude Code is started with.
    ///
    /// The name is passed on all three paths, which was checked against the
    /// real CLI rather than assumed: Beacon's name for a conversation and the
    /// one Claude Code shows in its own prompt box should not drift apart.
    pub fn args(&self) -> Vec<String> {
        let mut args = match &self.start {
            ClaudeStart::New => vec!["--session-id".into(), self.session_id.clone()],
            ClaudeStart::Resume => vec!["--resume".into(), self.session_id.clone()],
            ClaudeStart::Fork { from } => vec![
                "--resume".into(),
                from.clone(),
                "--fork-session".into(),
                "--session-id".into(),
                self.session_id.clone(),
            ],
        };

        if let Some(name) = &self.name {
            args.push("--name".into());
            args.push(name.clone());
        }
        args
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
/// Makes sure the runtime directory is there and readable only by its owner.
///
/// The permissions are the access control — on Linux the temporary directory is
/// shared between users, and a session is a shell — so a directory recreated
/// here has to be as closed as the one the daemon made at startup.
///
/// On Windows the temporary directory is inside the user's profile, and what is
/// made there inherits an ACL that already admits only that user.
fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

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

/// A session's input, shared between whoever types into it and — on Windows —
/// the reader, which has one question of the pseudo-console's to answer.
type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// What a Windows pseudo-console asks before it shows anything: where is the
/// cursor?
#[cfg(windows)]
const CURSOR_QUERY: &[u8] = b"\x1b[6n";

/// Answers the pseudo-console's opening question, and keeps it out of the
/// output.
///
/// The console is created to carry on from the terminal's cursor, so the first
/// thing it does is ask the terminal where that is — and it shows nothing at
/// all until it hears back. In a window that would be xterm's to answer, but a
/// session here is started by the daemon, often with no window attached, and
/// would sit silent until one was. So the daemon answers, with the truth for a
/// session that has only just been created: the top left. The question is then
/// dropped from what is recorded, because a window replaying the scrollback
/// later would answer it again, into the shell's input, where it reads as
/// `^[[1;1R` typed at the prompt.
///
/// Only the first is answered. Programs inside the session ask the console,
/// which answers them itself; nothing else reaches here asking this.
#[cfg(windows)]
fn answer_cursor_query(bytes: &[u8], writer: &SharedWriter) -> Option<Vec<u8>> {
    let at = bytes
        .windows(CURSOR_QUERY.len())
        .position(|window| window == CURSOR_QUERY)?;

    let mut writer = writer.lock_or_recover();
    let _ = writer.write_all(b"\x1b[1;1R");
    let _ = writer.flush();

    let mut rest = bytes[..at].to_vec();
    rest.extend_from_slice(&bytes[at + CURSOR_QUERY.len()..]);
    Some(rest)
}

/// Reports a session's process ending, on Windows, when it ends.
///
/// Everywhere else the reader learns it for free: the process exits, the pty
/// closes, the read returns nothing. A Windows pseudo-console does not close
/// with its process — the output pipe stays open until the console itself is
/// closed, which here is not until the session is dropped — so a Claude that
/// quit would sit in its panel looking alive. The process is waited on
/// directly instead, by its own thread, and the reader keeps draining whatever
/// output was still on its way.
///
/// The process is opened by id while `child` still holds a handle to it, so
/// the id cannot have been reused by the time this runs.
#[cfg(windows)]
fn watch_for_exit(
    pid: u32,
    id: SessionId,
    project: ProjectId,
    events: Arc<dyn SessionEvents>,
    exit_reported: Arc<AtomicBool>,
) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, INFINITE, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    let process = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    if process.is_null() {
        tracing::warn!(session = %id, pid, "could not watch the session's process; its exit will show late");
        return;
    }

    // Handles are not `Send`; the address is, and it is only used by the
    // thread that closes it.
    let process = process as usize;
    let spawned = std::thread::Builder::new()
        .name(format!("exit-{id}"))
        .spawn(move || {
            let process = process as windows_sys::Win32::Foundation::HANDLE;
            let mut code = 0u32;
            let known = unsafe {
                WaitForSingleObject(process, INFINITE);
                let known = GetExitCodeProcess(process, &mut code) != 0;
                CloseHandle(process);
                known
            };

            if !exit_reported.swap(true, Ordering::SeqCst) {
                events.exited(&id, &project, known.then_some(code as i32));
            }
        });

    if spawned.is_err() {
        unsafe { CloseHandle(process as windows_sys::Win32::Foundation::HANDLE) };
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

    /// A session started, but without something it was meant to have.
    ///
    /// Defaulted to silence so that a listener which only cares about output
    /// stays as short as it was. The daemon overrides it, because the window is
    /// the only place the user can be told.
    fn degraded(&self, project: &ProjectId, summary: &str) {
        let _ = (project, summary);
    }
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
    writer: SharedWriter,
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
    /// Where `claude` lives, worked out once.
    ///
    /// Only an answer is kept. Finding it means asking the user's login shell,
    /// which is allowed a few seconds and can miss that deadline on a machine
    /// that is briefly busy — right after an upgrade, say. Remembering that
    /// miss would leave a daemon that outlives the window telling every
    /// session for the rest of the day that Claude Code is not installed.
    claude_path: OnceLock<PathBuf>,
    /// The socket a Claude session's hooks should report to.
    ///
    /// Only the daemon knows this, and only Claude sessions are told: a shell
    /// has no reason to be able to reach the daemon that spawned it.
    hook_socket: Mutex<Option<PathBuf>>,
    /// The MCP configuration handed to every Claude session, written beside the
    /// socket. `None` until the socket is known, or if it could not be written.
    mcp_config: Mutex<Option<PathBuf>>,
    /// Which conversation each project's next Claude should start in.
    ///
    /// Held here rather than passed through `ensure`, alongside the hook socket
    /// and the MCP configuration, because it is the same kind of thing: what a
    /// session is spawned with, decided by the daemon, read at spawn time. It
    /// also means a session that exits and is brought back by `ensure` comes
    /// back into the conversation it was in, rather than starting a new one
    /// because the caller happened not to say.
    claude_launch: Mutex<HashMap<ProjectId, ClaudeLaunch>>,
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
            claude_launch: Mutex::new(HashMap::new()),
        }
    }

    /// Says which conversation a project's Claude should be started in.
    ///
    /// Set before every start, by whoever knows whether the conversation
    /// exists. The manager does not know and must not guess: spawning a process
    /// is not the same as a conversation being written, and a session that was
    /// opened and never typed into leaves nothing to resume.
    pub fn set_claude_launch(&self, project: ProjectId, launch: ClaudeLaunch) {
        self.claude_launch.lock_or_recover().insert(project, launch);
    }

    pub fn claude_launch(&self, project: &ProjectId) -> Option<ClaudeLaunch> {
        self.claude_launch.lock_or_recover().get(project).cloned()
    }

    pub fn forget_claude_launch(&self, project: &ProjectId) {
        self.claude_launch.lock_or_recover().remove(project);
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

    /// The MCP configuration to hand this session, written again if it is gone.
    ///
    /// It lives in the per-user temporary directory, which macOS sweeps: files
    /// left untouched for a few days are deleted, and this one is written once,
    /// on the day the daemon started. A daemon that has been up for a week
    /// therefore held a path to a file that no longer existed, passed
    /// `--mcp-config` pointing at it anyway, and Claude Code refused to start —
    /// *"MCP config file not found"* — so a swept file cost the entire panel.
    ///
    /// Checked here rather than rewritten on a timer, because the only moment
    /// the answer matters is the moment a session starts. A rewrite that fails
    /// means starting without the flag: the clip drawer is worth one file, and
    /// never worth the session.
    fn mcp_config(&self, project: &ProjectId) -> Option<PathBuf> {
        let path = self.mcp_config.lock_or_recover().clone()?;
        if path.is_file() {
            return Some(path);
        }

        let dir = path.parent()?;
        tracing::info!(path = %path.display(), "the mcp configuration was swept; writing it again");

        let restored = ensure_private_dir(dir).and_then(|()| write_mcp_config(dir));
        match restored {
            Ok(path) => {
                *self.mcp_config.lock_or_recover() = Some(path.clone());
                Some(path)
            }
            Err(err) => {
                // Not fatal, and deliberately not sticky: the next session
                // tries again, and until one succeeds sessions start with
                // everything but the clip drawer. Said out loud, because a
                // feature that is quietly absent is a puzzle later.
                tracing::warn!(error = %err, "could not write the mcp configuration again");
                self.events.degraded(
                    project,
                    "Beacon's MCP configuration is missing and could not be written, so the clip \
                     drawer is off for this session. Everything else works.",
                );
                None
            }
        }
    }

    /// The command for a session kind.
    ///
    /// Shells run as login shells, like every terminal emulator: without that a
    /// GUI application's PATH is missing most of the user's tools. Claude is
    /// launched directly from its resolved path, so nothing the user's startup
    /// files print ends up in the panel above it.
    fn command_for(
        &self,
        project: &ProjectId,
        kind: SessionKind,
        shell: Option<&ShellSpec>,
        launch: Option<&ClaudeLaunch>,
    ) -> Result<CommandBuilder> {
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
                        for arg in user_shell_args() {
                            command.arg(arg);
                        }
                        command
                    }
                };
                let _ = &mut command;
                Ok(command)
            }
            SessionKind::Claude => {
                let path = match self.claude_path.get() {
                    Some(path) => path.clone(),
                    None => {
                        let found = resolve_program("claude").ok_or_else(|| {
                            CoreError::invalid(
                                "could not find the claude command. Install Claude Code, or make \
                                 sure it is on the PATH your login shell sets.",
                            )
                        })?;
                        let _ = self.claude_path.set(found.clone());
                        found
                    }
                };
                let mut command = CommandBuilder::new(&path);

                // Merged with whatever the user has configured, never replacing
                // it: `--strict-mcp-config` would silently switch off every MCP
                // server they set up themselves, which is not a trade Beacon
                // gets to make on their behalf for a drawer.
                if let Some(config) = self.mcp_config(project) {
                    // `--mcp-config` takes a *list*, so the separated form
                    // swallows whatever argument comes after it. Nothing does
                    // today; writing it joined means nothing ever can.
                    command.arg(format!("--mcp-config={}", config.display()));
                }

                // Started in a named conversation, when this build of Claude
                // Code has the flags for it. Without them the session starts
                // exactly as it did before workstreams existed — which is the
                // whole point of asking rather than assuming.
                if let Some(launch) = launch.filter(|_| crate::claude::capabilities().workstreams())
                {
                    for arg in launch.args() {
                        command.arg(arg);
                    }
                }

                // Three small agents, defined for this session only, so nothing
                // is written into the user's repository and a Claude they start
                // themselves is untouched. The routing policy travels with them
                // because on its own it would name agents that do not exist.
                if launch.is_some_and(|launch| launch.agents)
                    && crate::claude::capabilities().session_agents
                {
                    command.arg("--agents");
                    command.arg(crate::agents::definitions());

                    if crate::claude::capabilities().append_system_prompt {
                        command.arg("--append-system-prompt");
                        command.arg(crate::agents::ROUTING_POLICY);
                    }
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

        let launch = (kind == SessionKind::Claude)
            .then(|| self.claude_launch(&project))
            .flatten();
        let mut command = self.command_for(&project, kind, shell, launch.as_ref())?;
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

        // Nothing is flipped to `Resume` here, deliberately. Having started a
        // process says only that Claude Code was asked to open the
        // conversation, and Claude Code writes nothing until the first
        // exchange: a session opened and never typed into leaves no
        // conversation, and `--resume` on it answers *"No conversation found
        // with session ID"*. What the next start uses is decided from a report
        // out of the session itself, by whoever sets the launch.

        // The slave must be closed here or the reader never sees EOF when the
        // child exits.
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|err| CoreError::session("could not read from the pty", err))?;
        let writer: SharedWriter =
            Arc::new(Mutex::new(pair.master.take_writer().map_err(|err| {
                CoreError::session("could not write to the pty", err)
            })?));

        let id = SessionId::generate();
        let scrollback = Arc::new(Mutex::new(Scrollback::new(DEFAULT_CAPACITY)));
        // Whoever learns of the exit first says so, and only once: the reader
        // reaching the end, or — on Windows — the process itself ending.
        let exit_reported = Arc::new(AtomicBool::new(false));

        #[cfg(windows)]
        if let Some(pid) = child.process_id() {
            watch_for_exit(
                pid,
                id.clone(),
                project.clone(),
                Arc::clone(&self.events),
                Arc::clone(&exit_reported),
            );
        }

        {
            // The PTY read is blocking, so it gets its own thread. It ends when
            // the child closes the pty, which is also how we learn it exited.
            let id = id.clone();
            let owner = project.clone();
            let events = Arc::clone(&self.events);
            let scrollback = Arc::clone(&scrollback);
            let exit_reported = Arc::clone(&exit_reported);
            #[cfg(windows)]
            let mut unanswered = Some(Arc::clone(&writer));
            std::thread::Builder::new()
                .name(format!("pty-{id}"))
                .spawn(move || {
                    let mut chunk = [0u8; 8 * 1024];
                    loop {
                        match reader.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                #[allow(unused_mut)]
                                let mut bytes = std::borrow::Cow::Borrowed(&chunk[..n]);
                                #[cfg(windows)]
                                if let Some(writer) = &unanswered
                                    && let Some(rest) = answer_cursor_query(&bytes, writer)
                                {
                                    bytes = std::borrow::Cow::Owned(rest);
                                    unanswered = None;
                                }
                                if bytes.is_empty() {
                                    continue;
                                }
                                // Recording and numbering happen under one lock
                                // so a snapshot can never interleave with this.
                                let offset = scrollback.lock_or_recover().push(&bytes);
                                events.output(&id, &owner, offset, &bytes);
                            }
                            Err(err) => {
                                tracing::debug!(session = %id, error = %err, "pty read ended");
                                break;
                            }
                        }
                    }
                    if !exit_reported.swap(true, Ordering::SeqCst) {
                        events.exited(&id, &owner, None);
                    }
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
        let mut writer = session.writer.lock_or_recover();
        writer
            .write_all(bytes)
            .map_err(|err| CoreError::session("could not write to the session", err))?;
        writer
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

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "b57bf9d0-8020-4275-a060-a521d289beae";
    const PARENT: &str = "e4e2464c-b66a-46ca-b65b-2af448574bb5";

    fn launch(start: ClaudeStart, name: Option<&str>) -> ClaudeLaunch {
        ClaudeLaunch {
            session_id: ID.into(),
            name: name.map(str::to_string),
            start,
            agents: true,
        }
    }

    #[test]
    fn a_new_conversation_is_started_on_an_id_beacon_chose() {
        assert_eq!(
            launch(ClaudeStart::New, Some("auth-refactor")).args(),
            ["--session-id", ID, "--name", "auth-refactor"]
        );
    }

    #[test]
    fn a_conversation_that_exists_is_resumed_rather_than_started() {
        // `--session-id` on a conversation already in use is refused, so this
        // is the difference between a restart that works and an error message
        // where the session used to be.
        assert_eq!(
            launch(ClaudeStart::Resume, Some("auth-refactor")).args(),
            ["--resume", ID, "--name", "auth-refactor"]
        );
    }

    #[test]
    fn a_fork_carries_the_parent_and_lands_on_an_id_beacon_chose() {
        assert_eq!(
            launch(
                ClaudeStart::Fork {
                    from: PARENT.into()
                },
                Some("dashboard-experiment")
            )
            .args(),
            [
                "--resume",
                PARENT,
                "--fork-session",
                "--session-id",
                ID,
                "--name",
                "dashboard-experiment"
            ]
        );
    }

    #[test]
    fn a_conversation_nobody_named_is_not_given_a_name() {
        assert_eq!(launch(ClaudeStart::New, None).args(), ["--session-id", ID]);
        assert_eq!(launch(ClaudeStart::Resume, None).args(), ["--resume", ID]);
    }

    #[test]
    fn a_launch_survives_a_round_trip_across_the_socket() {
        for start in [
            ClaudeStart::New,
            ClaudeStart::Resume,
            ClaudeStart::Fork {
                from: PARENT.into(),
            },
        ] {
            let original = launch(start, Some("payments-bug"));
            let line = serde_json::to_string(&original).unwrap();
            let back: ClaudeLaunch = serde_json::from_str(&line).unwrap();
            assert_eq!(back, original);
        }
    }

    /// The bug: the file is written once, on the day the daemon starts, into a
    /// directory macOS sweeps. Days later every session was launched with
    /// `--mcp-config` pointing at nothing, and Claude Code refused to start.
    #[test]
    fn a_swept_mcp_configuration_is_written_again_rather_than_pointed_at() {
        struct Silent;
        impl SessionEvents for Silent {
            fn output(&self, _: &SessionId, _: &ProjectId, _: u64, _: &[u8]) {}
            fn exited(&self, _: &SessionId, _: &ProjectId, _: Option<i32>) {}
        }

        let dir = tempfile::tempdir().unwrap();
        let manager = SessionManager::new(Arc::new(Silent));
        let project = ProjectId("pj_x".into());
        manager.set_hook_socket(dir.path().join("daemon.sock"));

        let config = manager
            .mcp_config(&project)
            .expect("a configuration to start with");
        assert!(config.is_file());

        std::fs::remove_file(&config).unwrap();
        assert_eq!(manager.mcp_config(&project).as_ref(), Some(&config));
        assert!(
            config.is_file(),
            "the swept file should have been written again"
        );
    }

    /// The sweep can take the directory with it. Restoring it must not leave it
    /// open to anyone else on the machine.
    #[cfg(unix)]
    #[test]
    fn a_restored_runtime_directory_stays_private() {
        use std::os::unix::fs::PermissionsExt;

        struct Silent;
        impl SessionEvents for Silent {
            fn output(&self, _: &SessionId, _: &ProjectId, _: u64, _: &[u8]) {}
            fn exited(&self, _: &SessionId, _: &ProjectId, _: Option<i32>) {}
        }

        let parent = tempfile::tempdir().unwrap();
        let runtime = parent.path().join("beacon-split-test");
        std::fs::create_dir(&runtime).unwrap();

        let manager = SessionManager::new(Arc::new(Silent));
        let project = ProjectId("pj_x".into());
        manager.set_hook_socket(runtime.join("daemon.sock"));
        assert!(manager.mcp_config(&project).is_some());

        std::fs::remove_dir_all(&runtime).unwrap();
        let config = manager
            .mcp_config(&project)
            .expect("the directory to be restored");

        assert!(config.is_file());
        let mode = std::fs::metadata(&runtime).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "the runtime directory must stay private"
        );
    }

    /// What must never happen again: the file is gone, it cannot be written,
    /// and the answer is to start the session without the clip drawer while
    /// saying so — not to hand Claude Code a path to nothing and let it refuse
    /// to start at all.
    #[test]
    fn a_configuration_that_cannot_be_restored_is_reported_and_left_behind() {
        #[derive(Default)]
        struct Recorder {
            said: Mutex<Vec<String>>,
        }
        impl SessionEvents for Recorder {
            fn output(&self, _: &SessionId, _: &ProjectId, _: u64, _: &[u8]) {}
            fn exited(&self, _: &SessionId, _: &ProjectId, _: Option<i32>) {}
            fn degraded(&self, _: &ProjectId, summary: &str) {
                self.said.lock_or_recover().push(summary.to_string());
            }
        }

        let parent = tempfile::tempdir().unwrap();
        let runtime = parent.path().join("beacon-split-test");
        std::fs::create_dir(&runtime).unwrap();

        let recorder = Arc::new(Recorder::default());
        let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
        let project = ProjectId("pj_x".into());

        manager.set_hook_socket(runtime.join("daemon.sock"));
        assert!(manager.mcp_config(&project).is_some(), "written at startup");

        // The sweep took it, and the directory cannot be made again: something
        // else is sitting on the name.
        std::fs::remove_dir_all(&runtime).unwrap();
        std::fs::write(&runtime, b"in the way").unwrap();

        assert_eq!(
            manager.mcp_config(&project),
            None,
            "a session must start without the flag rather than with a broken one"
        );

        let said = recorder.said.lock_or_recover().clone();
        assert_eq!(said.len(), 1, "the user should be told exactly once");
        assert!(
            said[0].contains("clip drawer"),
            "the message should name what is missing, not the plumbing: {}",
            said[0]
        );
    }

    #[test]
    fn a_project_is_told_how_to_start_before_it_starts() {
        struct Silent;
        impl SessionEvents for Silent {
            fn output(&self, _: &SessionId, _: &ProjectId, _: u64, _: &[u8]) {}
            fn exited(&self, _: &SessionId, _: &ProjectId, _: Option<i32>) {}
        }

        let manager = SessionManager::new(Arc::new(Silent));
        let project = ProjectId("pj_x".into());
        assert!(manager.claude_launch(&project).is_none());

        manager.set_claude_launch(project.clone(), launch(ClaudeStart::New, Some("auth")));
        assert_eq!(
            manager.claude_launch(&project).unwrap().start,
            ClaudeStart::New
        );

        manager.forget_claude_launch(&project);
        assert!(manager.claude_launch(&project).is_none());
    }

    /// Stands in for a session's input, keeping what was written to it.
    #[cfg(windows)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    #[cfg(windows)]
    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[cfg(windows)]
    fn captured() -> (SharedWriter, Arc<Mutex<Vec<u8>>>) {
        let input = Arc::new(Mutex::new(Vec::new()));
        let writer: SharedWriter = Arc::new(Mutex::new(Box::new(Captured(Arc::clone(&input)))));
        (writer, input)
    }

    /// What a pseudo-console prints first, in the order it prints it.
    #[cfg(windows)]
    #[test]
    fn the_pseudo_consoles_opening_question_is_answered_and_not_recorded() {
        let (writer, input) = captured();
        let opening = b"\x1b[?9001h\x1b[?1004h\x1b[6n\x1b[?25l";

        let rest = answer_cursor_query(opening, &writer).expect("the query should be found");

        assert_eq!(input.lock().unwrap().as_slice(), b"\x1b[1;1R");
        assert_eq!(rest, b"\x1b[?9001h\x1b[?1004h\x1b[?25l");
    }

    #[cfg(windows)]
    #[test]
    fn output_without_the_question_is_left_alone() {
        let (writer, input) = captured();
        assert!(answer_cursor_query(b"PS C:\\> ", &writer).is_none());
        assert!(input.lock().unwrap().is_empty());
    }
}
