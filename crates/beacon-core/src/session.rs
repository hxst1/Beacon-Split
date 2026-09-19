use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentKind;
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
    Codex,
}

impl SessionKind {
    /// Which agent this kind runs, or `None` for a plain shell.
    ///
    /// One variant per agent rather than one carrying an [`AgentKind`], which
    /// would have read better here and cost more everywhere else: the kind is
    /// part of the key a project's sessions are filed under and part of the
    /// wire, and `"claude"` is already both. A variant added beside it changes
    /// nothing that already works.
    ///
    /// The upside falls out of that key: two agents in one project are two
    /// kinds, so they no more collide than a shell and a Claude do today.
    /// The kind of session an agent runs in.
    ///
    /// The inverse of [`SessionKind::agent`], and the reason both exist: the
    /// daemon holds conversations by agent and sessions by kind, so it crosses
    /// between them often enough that writing the match out each time was
    /// three chances to write it differently.
    pub fn for_agent(agent: AgentKind) -> Self {
        match agent {
            AgentKind::Claude => SessionKind::Claude,
            AgentKind::Codex => SessionKind::Codex,
        }
    }

    pub fn agent(self) -> Option<AgentKind> {
        match self {
            SessionKind::Shell => None,
            SessionKind::Claude => Some(AgentKind::Claude),
            SessionKind::Codex => Some(AgentKind::Codex),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Claude => "claude",
            Self::Codex => "codex",
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

/// How an agent session should be started.
///
/// Held in the process and never written down: it is what the next spawn of a
/// project's agent should do, which is a fact about now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentLaunch {
    pub agent: AgentKind,
    /// Beacon's id for the conversation, as a UUID.
    ///
    /// Passed to an agent that accepts one. Codex does not, so for Codex this
    /// is Beacon's own handle on the conversation and never reaches the command
    /// line — the id Codex knows it by arrives in
    /// [`ConversationStart::Resume`], because that is the only place it is
    /// needed and the only place it is known.
    pub session_id: String,
    /// What to call it, when it has been called something.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub start: ConversationStart,
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
/// The distinction is not cosmetic. `--session-id` on a conversation that has
/// already been used is refused — *"Session ID … is already in use"* — and
/// `--resume` on one that has never been spoken in answers *"No conversation
/// found with session ID"*. Getting this wrong turns a restart into an error
/// message where the session used to be, in one direction or the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ConversationStart {
    /// A conversation that does not exist yet.
    New,
    /// One that does, carrying the id *the agent* knows it by.
    ///
    /// The id travels with the variant rather than being taken from
    /// [`AgentLaunch::session_id`] because the two are not always the same
    /// number: Codex names its own conversations, so resuming one means using
    /// the name it reported and not the one Beacon chose.
    #[serde(rename_all = "camelCase")]
    Resume { id: String },
    /// A new conversation carrying another's history, named as its agent knows
    /// the other.
    #[serde(rename_all = "camelCase")]
    Fork { from: String },
}

impl AgentLaunch {
    /// The arguments the agent is started with.
    ///
    /// Both programs, because the words differ more than the ideas do: Claude
    /// Code takes flags for all three cases, while Codex resumes and forks
    /// through subcommands and has no way to be told an id or a name at all.
    ///
    /// Subcommands come first for Codex, and anything global has to precede
    /// them — driving the real CLI answers `unexpected argument` to a global
    /// flag written after `exec`, and there is no reason to think `resume` is
    /// more forgiving.
    pub fn args(&self) -> Vec<String> {
        match self.agent {
            AgentKind::Claude => self.claude_args(),
            AgentKind::Codex => self.codex_args(),
        }
    }

    /// The name is passed on all three paths, which was checked against the
    /// real CLI rather than assumed: Beacon's name for a conversation and the
    /// one Claude Code shows in its own prompt box should not drift apart.
    fn claude_args(&self) -> Vec<String> {
        let mut args = match &self.start {
            ConversationStart::New => vec!["--session-id".into(), self.session_id.clone()],
            ConversationStart::Resume { id } => vec!["--resume".into(), id.clone()],
            ConversationStart::Fork { from } => vec![
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

    /// Nothing at all for a new conversation: Codex is told neither what to
    /// call it nor what id to give it, and says both afterwards.
    ///
    /// The name is deliberately not passed anywhere. Codex has no `--name`,
    /// and resuming by a name it was given interactively is reported broken in
    /// versions people are running — so Beacon keeps its own name and uses the
    /// id for everything that has to be right.
    fn codex_args(&self) -> Vec<String> {
        match &self.start {
            ConversationStart::New => Vec::new(),
            ConversationStart::Resume { id } => vec!["resume".into(), id.clone()],
            ConversationStart::Fork { from } => vec!["fork".into(), from.clone()],
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
/// Makes sure the runtime directory is there and readable only by its owner.
///
/// The permissions are the access control — on Linux the temporary directory is
/// shared between users, and a session is a shell — so a directory recreated
/// here has to be as closed as the one the daemon made at startup.
fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
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
    /// Where `claude` lives, worked out once.
    ///
    /// Only an answer is kept. Finding it means asking the user's login shell,
    /// which is allowed a few seconds and can miss that deadline on a machine
    /// that is briefly busy — right after an upgrade, say. Remembering that
    /// miss would leave a daemon that outlives the window telling every
    /// session for the rest of the day that Claude Code is not installed.
    /// Where each agent's program was found, once it has been.
    ///
    /// Successes only. A miss is never remembered: where an agent lives comes
    /// from the login shell, which can fail to answer on a machine that is
    /// briefly busy, and the daemon outlives the window — so one missed answer
    /// used to mean every session for the rest of the day was told the program
    /// was not installed.
    program_paths: Mutex<HashMap<AgentKind, PathBuf>>,
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
    /// Keyed by the agent as well as the project, so a project running both
    /// does not have one launch standing in for the other.
    agent_launch: Mutex<HashMap<(ProjectId, AgentKind), AgentLaunch>>,
}

impl SessionManager {
    pub fn new(events: Arc<dyn SessionEvents>) -> Self {
        Self {
            events,
            sessions: Mutex::new(HashMap::new()),
            by_project: Mutex::new(HashMap::new()),
            program_paths: Mutex::new(HashMap::new()),
            hook_socket: Mutex::new(None),
            mcp_config: Mutex::new(None),
            agent_launch: Mutex::new(HashMap::new()),
        }
    }

    /// Says which conversation a project's Claude should be started in.
    ///
    /// Set before every start, by whoever knows whether the conversation
    /// exists. The manager does not know and must not guess: spawning a process
    /// is not the same as a conversation being written, and a session that was
    /// opened and never typed into leaves nothing to resume.
    /// The agent is taken from the launch itself rather than passed beside it:
    /// two arguments that have to agree are two chances to disagree.
    pub fn set_agent_launch(&self, project: ProjectId, launch: AgentLaunch) {
        self.agent_launch
            .lock_or_recover()
            .insert((project, launch.agent), launch);
    }

    pub fn agent_launch(&self, project: &ProjectId, agent: AgentKind) -> Option<AgentLaunch> {
        self.agent_launch
            .lock_or_recover()
            .get(&(project.clone(), agent))
            .cloned()
    }

    pub fn forget_agent_launch(&self, project: &ProjectId, agent: AgentKind) {
        self.agent_launch
            .lock_or_recover()
            .remove(&(project.clone(), agent));
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

    /// Where an agent's program lives, asked once per agent and kept.
    fn program_path(&self, agent: AgentKind) -> Result<PathBuf> {
        if let Some(path) = self.program_paths.lock_or_recover().get(&agent) {
            return Ok(path.clone());
        }

        let found = resolve_program(agent.program()).ok_or_else(|| {
            CoreError::invalid(format!(
                "could not find the {} command. Install {}, or make sure it is on the PATH your \
                 login shell sets.",
                agent.program(),
                agent.label(),
            ))
        })?;
        self.program_paths
            .lock_or_recover()
            .insert(agent, found.clone());
        Ok(found)
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
        launch: Option<&AgentLaunch>,
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
                        command.arg("-l");
                        command
                    }
                };
                let _ = &mut command;
                Ok(command)
            }
            SessionKind::Claude | SessionKind::Codex => {
                let agent = kind.agent().expect("a shell is handled above");
                let path = self.program_path(agent)?;
                let mut command = CommandBuilder::new(&path);

                // The `PATH` the user's login shell sets, rather than the one
                // Beacon inherited.
                //
                // A shell session works its own out, because it is a login
                // shell. An agent is run directly — so that nothing anyone's
                // startup files print lands in the panel — and inherits
                // Beacon's, which launched from the Dock is the bare
                // `/usr/bin:/bin:/usr/sbin:/sbin`. Everything the user
                // installed is missing from it: `node`, `cargo`, whatever a
                // version manager put on theirs. The agent cannot run those,
                // and neither can the hooks it starts, which is how this was
                // found — a hook failing with `node: command not found` in
                // every session.
                //
                // The program's own directory goes in front of it, because an
                // agent installed with npm is a script that needs the
                // interpreter npm put beside it.
                command.env("PATH", crate::tools::session_path(&path));

                // Merged with whatever the user has configured, never replacing
                // it: `--strict-mcp-config` would silently switch off every MCP
                // server they set up themselves, which is not a trade Beacon
                // gets to make on their behalf for a drawer.
                //
                // Claude Code only. Codex has no per-invocation flag for this
                // at all — its servers come from a config file — so the drawer
                // reaches it another way or not at all.
                if agent == AgentKind::Claude
                    && let Some(config) = self.mcp_config(project)
                {
                    // `--mcp-config` takes a *list*, so the separated form
                    // swallows whatever argument comes after it. Nothing does
                    // today; writing it joined means nothing ever can.
                    command.arg(format!("--mcp-config={}", config.display()));
                }

                // Started in a named conversation, when the installed agent has
                // what that needs. Without it the session starts exactly as it
                // did before workstreams existed — which is the whole point of
                // asking rather than assuming.
                if let Some(launch) = launch.filter(|_| agent.workstreams()) {
                    for arg in launch.args() {
                        command.arg(arg);
                    }
                }

                // Three small agents, defined for this session only, so nothing
                // is written into the user's repository and a Claude they start
                // themselves is untouched. The routing policy travels with them
                // because on its own it would name agents that do not exist.
                //
                // Claude Code only, and not for want of trying elsewhere: these
                // are defined in the shape `--agents` takes.
                if agent == AgentKind::Claude
                    && launch.is_some_and(|launch| launch.agents)
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

        let launch = kind
            .agent()
            .and_then(|agent| self.agent_launch(&project, agent));
        let mut command = self.command_for(&project, kind, shell, launch.as_ref())?;
        command.cwd(cwd);
        prepare_environment(&mut command);

        // An agent session is told how to reach us, so its hooks can say what
        // it is doing. Without these the hook is inert, which is what makes it
        // safe to register once and forget about: a session somebody started
        // in their own terminal has none of this and reports nothing.
        //
        // Which agent is named too, and it matters more than it looks. A
        // project can be running both, and one of them reports a conversation
        // id Beacon did not choose — so without this the daemon would have a
        // number and no idea whose it was.
        if let Some(agent) = kind.agent()
            && let Some(socket) = self.hook_socket.lock_or_recover().as_ref()
        {
            command.env("BEACON_SOCKET", socket);
            command.env("BEACON_PROJECT", project.as_str());
            command.env("BEACON_AGENT", agent.as_str());
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

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "b57bf9d0-8020-4275-a060-a521d289beae";
    const PARENT: &str = "e4e2464c-b66a-46ca-b65b-2af448574bb5";

    fn launch(start: ConversationStart, name: Option<&str>) -> AgentLaunch {
        AgentLaunch {
            agent: AgentKind::Claude,
            session_id: ID.into(),
            name: name.map(str::to_string),
            start,
            agents: true,
        }
    }

    #[test]
    fn a_new_conversation_is_started_on_an_id_beacon_chose() {
        assert_eq!(
            launch(ConversationStart::New, Some("auth-refactor")).args(),
            ["--session-id", ID, "--name", "auth-refactor"]
        );
    }

    #[test]
    fn a_conversation_that_exists_is_resumed_rather_than_started() {
        // `--session-id` on a conversation already in use is refused, so this
        // is the difference between a restart that works and an error message
        // where the session used to be.
        assert_eq!(
            launch(
                ConversationStart::Resume { id: ID.into() },
                Some("auth-refactor")
            )
            .args(),
            ["--resume", ID, "--name", "auth-refactor"]
        );
    }

    #[test]
    fn a_fork_carries_the_parent_and_lands_on_an_id_beacon_chose() {
        assert_eq!(
            launch(
                ConversationStart::Fork {
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
        assert_eq!(
            launch(ConversationStart::New, None).args(),
            ["--session-id", ID]
        );
        assert_eq!(
            launch(ConversationStart::Resume { id: ID.into() }, None).args(),
            ["--resume", ID]
        );
    }

    #[test]
    fn a_launch_survives_a_round_trip_across_the_socket() {
        for start in [
            ConversationStart::New,
            ConversationStart::Resume { id: ID.into() },
            ConversationStart::Fork {
                from: PARENT.into(),
            },
        ] {
            let original = launch(start, Some("payments-bug"));
            let line = serde_json::to_string(&original).unwrap();
            let back: AgentLaunch = serde_json::from_str(&line).unwrap();
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
        assert!(manager.agent_launch(&project, AgentKind::Claude).is_none());

        manager.set_agent_launch(
            project.clone(),
            launch(ConversationStart::New, Some("auth")),
        );
        assert_eq!(
            manager
                .agent_launch(&project, AgentKind::Claude)
                .unwrap()
                .start,
            ConversationStart::New
        );

        // Filed under the agent as well as the project: the other agent's
        // launch is a different thing and must not be found here.
        assert!(
            manager.agent_launch(&project, AgentKind::Codex).is_none(),
            "a Claude launch is not Codex's"
        );

        manager.forget_agent_launch(&project, AgentKind::Claude);
        assert!(manager.agent_launch(&project, AgentKind::Claude).is_none());
    }
}
