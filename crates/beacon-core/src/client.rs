use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::agent::AgentKind;
use crate::domain::ProjectId;
use crate::error::{CoreError, Result};
use crate::protocol::{
    Envelope, Event, Message, Outcome, PROTOCOL_VERSION, Reply, Request, Response, socket_path,
};
use crate::session::{SessionInfo, SessionKind, SessionPrefs};
use crate::transport::LocalStream;

/// How long a request waits before giving up.
///
/// Generous: starting a session runs the user's login shell, which on a busy
/// machine with a heavy `.zshrc` is not instant.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// How long to wait for a freshly started daemon to begin listening.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Receives what the daemon reports without being asked.
pub trait DaemonEvents: Send + Sync + 'static {
    fn event(&self, event: Event);
    /// The connection dropped. Sessions may still be running; we are no longer
    /// hearing about them.
    fn disconnected(&self);
    /// A connection is live again.
    ///
    /// Possibly to a different daemon, so anything holding a session id has to
    /// ask for it again rather than assume it is still valid.
    fn reattached(&self);
}

/// How long to wait between attempts to get back to the daemon.
const RECONNECT_BACKOFF: &[Duration] = &[
    Duration::from_millis(200),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
];

/// State shared between the client and the threads that read for it.
struct Shared {
    /// `None` between losing a connection and getting another.
    stream: Mutex<Option<LocalStream>>,
    pending: Mutex<HashMap<u64, Sender<Outcome>>>,
    next_id: AtomicU64,
    binary: PathBuf,
    /// Where this client's daemon listens. Explicit so a second, isolated
    /// Beacon is possible — and so tests cannot reach a daemon someone is using.
    socket: PathBuf,
    events: Arc<dyn DaemonEvents>,
    /// Set when the client is dropped, so a reconnect loop stops trying.
    stopped: AtomicBool,
    /// Guards against two reconnect loops racing each other.
    reconnecting: AtomicBool,
    /// Which connection is the live one.
    ///
    /// A reader thread outlives the stream it reads: it notices the end only
    /// after the connection is gone, by which time another may already have
    /// taken its place. Without this it would tear that replacement down —
    /// which is how a planned daemon swap ends with a window convinced it is
    /// offline.
    generation: AtomicU64,
    /// Set while a daemon is being deliberately replaced, so the disconnect
    /// that follows is not reported to the window as a daemon lost.
    replacing: AtomicBool,
    /// One connection attempt at a time, so a reconnect loop and a caller
    /// cannot each start a daemon of their own.
    opening: Mutex<()>,
}

/// A connection to the session daemon.
///
/// The UI holds one of these where it used to hold a `SessionManager`. The
/// methods are the same shape on purpose: what changed is where the sessions
/// live, not what can be done with them.
///
/// The connection repairs itself. A daemon that is restarted — during an
/// upgrade, or because someone stopped it — must not leave the window
/// permanently deaf, since the whole point of the daemon is that it outlives
/// things.
pub struct DaemonClient {
    shared: Arc<Shared>,
}

impl DaemonClient {
    /// Connects, starting a daemon if none is listening.
    ///
    /// `daemon_binary` is where to find it; the caller knows, because in
    /// development it sits beside the app and in a bundle it is inside it.
    pub fn connect(daemon_binary: &Path, events: Arc<dyn DaemonEvents>) -> Result<Self> {
        Self::connect_at(daemon_binary, &socket_path(), events)
    }

    /// Connects to a daemon on a particular socket, starting one if needed.
    pub fn connect_at(
        daemon_binary: &Path,
        socket: &Path,
        events: Arc<dyn DaemonEvents>,
    ) -> Result<Self> {
        let client = Self {
            shared: Shared::new(daemon_binary, socket, events),
        };
        client.shared.open()?;

        let greeting = client.hello()?;
        if greeting.version == PROTOCOL_VERSION {
            tracing::info!(
                pid = greeting.pid,
                sessions = greeting.sessions,
                "attached to the session daemon"
            );
            return Ok(client);
        }

        // A daemon left over from another version would answer in a shape we do
        // not understand, and a half-understood session is worse than a new one.
        //
        // This is the first thing that happens after an upgrade that moved the
        // protocol, so it has to be quiet: the swap is expected, and a window
        // that has not finished opening should not be told the daemon was lost.
        tracing::info!(
            theirs = greeting.version,
            ours = PROTOCOL_VERSION,
            "replacing a daemon speaking a different protocol"
        );
        client.shared.replace()?;

        // The daemon that took its place was started from the binary beside
        // this one, and is normally this version. When it is not, that binary
        // is itself left over: on Windows an installer cannot overwrite a
        // program that is running, and the daemon — or a Claude session's MCP
        // server, which is the same file — usually is. Carrying on would talk
        // this protocol to a daemon that does not speak it, and replacing it
        // again would only start the same file again. So it is stopped, and the
        // window is told what happened and what fixes it.
        let replacement = client.hello()?;
        if replacement.version != PROTOCOL_VERSION {
            tracing::error!(
                theirs = replacement.version,
                ours = PROTOCOL_VERSION,
                binary = %client.shared.binary.display(),
                "the daemon on disk is from another version"
            );
            let _ = client.request(Request::Shutdown {});
            return Err(CoreError::invalid(format!(
                "the session daemon at {} is from another version of Beacon (protocol {}, this \
                 window speaks {}), so it was not replaced when Beacon was updated. Close \
                 Beacon and install it again.",
                client.shared.binary.display(),
                replacement.version,
                PROTOCOL_VERSION
            )));
        }
        Ok(client)
    }

    fn request(&self, request: Request) -> Result<Reply> {
        self.shared.request(request)
    }

    fn hello(&self) -> Result<crate::protocol::Greeting> {
        match self.request(Request::Hello {
            version: PROTOCOL_VERSION,
        })? {
            Reply::Greeting(greeting) => Ok(greeting),
            _ => Err(CoreError::invalid("the daemon did not introduce itself")),
        }
    }

    // ---- the same surface the session manager offers -----------------------

    pub fn ensure(
        &self,
        project: &ProjectId,
        kind: SessionKind,
        slot: u32,
        cwd: &Path,
        size: (u16, u16),
        prefs: SessionPrefs,
    ) -> Result<SessionInfo> {
        match self.request(Request::Ensure {
            project: project.clone(),
            kind,
            slot,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
            shell: prefs.shell,
            agents: Some(prefs.agents),
        })? {
            Reply::Session(info) => Ok(info),
            _ => Err(unexpected()),
        }
    }

    pub fn write(&self, id: &crate::session::SessionId, data: &str) -> Result<()> {
        self.request(Request::Write {
            id: id.clone(),
            data: data.to_string(),
        })
        .map(|_| ())
    }

    pub fn resize(&self, id: &crate::session::SessionId, cols: u16, rows: u16) -> Result<()> {
        self.request(Request::Resize {
            id: id.clone(),
            cols,
            rows,
        })
        .map(|_| ())
    }

    /// The retained output and the offset just past it.
    pub fn scrollback(&self, id: &crate::session::SessionId) -> Result<(String, u64)> {
        match self.request(Request::Scrollback { id: id.clone() })? {
            Reply::Scrollback { data, end_offset } => Ok((data, end_offset)),
            _ => Err(unexpected()),
        }
    }

    pub fn close(&self, id: &crate::session::SessionId) -> Result<()> {
        self.request(Request::Close { id: id.clone() }).map(|_| ())
    }

    pub fn restart(
        &self,
        project: &ProjectId,
        kind: SessionKind,
        slot: u32,
        cwd: &Path,
        size: (u16, u16),
        prefs: SessionPrefs,
    ) -> Result<SessionInfo> {
        match self.request(Request::Restart {
            project: project.clone(),
            kind,
            slot,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
            shell: prefs.shell,
            agents: Some(prefs.agents),
        })? {
            Reply::Session(info) => Ok(info),
            _ => Err(unexpected()),
        }
    }

    pub fn close_project(&self, project: &ProjectId) -> Result<()> {
        self.request(Request::CloseProject {
            project: project.clone(),
        })
        .map(|_| ())
    }

    pub fn list(&self) -> Result<Vec<SessionInfo>> {
        match self.request(Request::List {})? {
            Reply::Sessions { sessions } => Ok(sessions),
            _ => Err(unexpected()),
        }
    }

    /// What each project's Claude session last reported it was costing.
    pub fn usage(&self) -> Result<Vec<crate::protocol::UsageReport>> {
        match self.request(Request::Usage {})? {
            Reply::Usage { reports } => Ok(reports),
            _ => Err(unexpected()),
        }
    }

    /// A project's conversations with one agent, most recently active first,
    /// and which one it is in.
    pub fn workstreams(
        &self,
        project: &ProjectId,
        agent: AgentKind,
    ) -> Result<(
        Vec<crate::workstreams::Workstream>,
        Option<crate::workstreams::WorkstreamId>,
    )> {
        match self.request(Request::Workstreams {
            project: project.clone(),
            agent,
        })? {
            Reply::Workstreams {
                workstreams,
                current,
            } => Ok((workstreams, current)),
            _ => Err(unexpected()),
        }
    }

    /// Starts a new conversation and puts the project's agent in it.
    pub fn start_workstream(
        &self,
        project: &ProjectId,
        agent: AgentKind,
        name: Option<String>,
        cwd: &Path,
        size: (u16, u16),
        prefs: SessionPrefs,
    ) -> Result<(crate::workstreams::Workstream, SessionInfo)> {
        self.opened(Request::StartWorkstream {
            project: project.clone(),
            agent,
            name,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
            shell: prefs.shell,
            agents: Some(prefs.agents),
        })
    }

    /// Returns the project to a conversation it already has.
    pub fn resume_workstream(
        &self,
        project: &ProjectId,
        id: &crate::workstreams::WorkstreamId,
        cwd: &Path,
        size: (u16, u16),
        prefs: SessionPrefs,
    ) -> Result<(crate::workstreams::Workstream, SessionInfo)> {
        self.opened(Request::ResumeWorkstream {
            project: project.clone(),
            id: id.clone(),
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
            shell: prefs.shell,
            agents: Some(prefs.agents),
        })
    }

    /// Starts a new conversation carrying another's history.
    pub fn fork_workstream(
        &self,
        project: &ProjectId,
        from: &crate::workstreams::WorkstreamId,
        name: Option<String>,
        cwd: &Path,
        size: (u16, u16),
        prefs: SessionPrefs,
    ) -> Result<(crate::workstreams::Workstream, SessionInfo)> {
        self.opened(Request::ForkWorkstream {
            project: project.clone(),
            from: from.clone(),
            name,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
            shell: prefs.shell,
            agents: Some(prefs.agents),
        })
    }

    /// Renames one, or takes its name away when given `None`.
    pub fn rename_workstream(
        &self,
        project: &ProjectId,
        id: &crate::workstreams::WorkstreamId,
        name: Option<String>,
    ) -> Result<()> {
        self.request(Request::RenameWorkstream {
            project: project.clone(),
            id: id.clone(),
            name,
        })
        .map(|_| ())
    }

    /// The three requests that end in a Claude session share a reply.
    fn opened(&self, request: Request) -> Result<(crate::workstreams::Workstream, SessionInfo)> {
        match self.request(request)? {
            Reply::Workstream {
                workstream,
                session,
            } => Ok((*workstream, session)),
            _ => Err(unexpected()),
        }
    }

    /// Everything in the clip drawer, newest first.
    pub fn clips(&self) -> Result<Vec<crate::clips::Clip>> {
        match self.request(Request::Clips {})? {
            Reply::Clips { clips } => Ok(clips),
            _ => Err(unexpected()),
        }
    }

    /// Drops one clip, or the whole drawer when given `None`.
    ///
    /// Returns what is left rather than nothing, so the window that asked does
    /// not have to guess — and matches the broadcast every other window gets.
    pub fn forget_clips(
        &self,
        id: Option<crate::domain::ClipId>,
    ) -> Result<Vec<crate::clips::Clip>> {
        match self.request(Request::ForgetClips { id })? {
            Reply::Clips { clips } => Ok(clips),
            _ => Err(unexpected()),
        }
    }

    pub fn shutdown(&self) -> Result<()> {
        self.request(Request::Shutdown {}).map(|_| ())
    }
}

impl Drop for DaemonClient {
    fn drop(&mut self) {
        // Otherwise the reconnect loop would outlive the client it serves.
        self.shared.stopped.store(true, Ordering::SeqCst);
    }
}

impl Shared {
    fn new(binary: &Path, socket: &Path, events: Arc<dyn DaemonEvents>) -> Arc<Self> {
        Arc::new(Shared {
            stream: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            binary: binary.to_path_buf(),
            socket: socket.to_path_buf(),
            events,
            stopped: AtomicBool::new(false),
            reconnecting: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            replacing: AtomicBool::new(false),
            opening: Mutex::new(()),
        })
    }

    /// Connects to a running daemon, or starts one and waits for it.
    ///
    /// One attempt at a time, and a no-op when somebody else got there first:
    /// a reconnect loop racing a caller would otherwise start a second daemon
    /// and leave the window attached twice, hearing every event two times.
    ///
    /// Never starts one for a client that has been dropped. The reconnect loop
    /// asks before it gets here, but the client can go between that and this
    /// — or while this waits its turn behind another attempt — and it is here,
    /// last, that the answer decides whether a daemon nobody will attach to
    /// gets started. (A drop in the instant between this check and the start
    /// still gets one; with no sessions and nobody attached, it stops itself
    /// after its idle timeout.)
    fn open(self: &Arc<Self>) -> Result<()> {
        let _one_at_a_time = self.opening.lock_or_recover();
        if self.stream.lock_or_recover().is_some() {
            return Ok(());
        }

        let stream = match connect_once(&self.socket) {
            Some(stream) => stream,
            None => {
                if self.stopped.load(Ordering::SeqCst) {
                    return Err(CoreError::invalid(
                        "the client was dropped; not starting a daemon for it",
                    ));
                }
                spawn_daemon(&self.binary, &self.socket)?;
                wait_for_daemon(&self.socket)?
            }
        };
        self.adopt(stream)
    }

    /// Stops the daemon on the other end and connects to the one that takes
    /// its place, without any of it reaching the window as a lost connection.
    ///
    /// For an upgrade: the daemon still listening is the one the previous
    /// version left behind, and swapping it is the expected thing to happen,
    /// not a failure to report.
    fn replace(self: &Arc<Self>) -> Result<()> {
        self.replacing.store(true, Ordering::SeqCst);

        // Answered, or cut off mid-answer when the daemon goes first. Either
        // is a success here; what matters is that it is no longer listening.
        let _ = self.request(Request::Shutdown {});

        // The reader thread may not have noticed the end yet, and a stream
        // nobody can write to would make `open` decide there was nothing to do.
        self.forget_connection();

        let result = self.open();
        self.replacing.store(false, Ordering::SeqCst);
        result
    }

    /// Drops the current connection and retires its reader.
    fn forget_connection(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        *self.stream.lock_or_recover() = None;
    }

    /// Takes over a connection and starts reading from it.
    fn adopt(self: &Arc<Self>, stream: LocalStream) -> Result<()> {
        let reader_half = stream
            .try_clone()
            .map_err(|err| CoreError::session("could not use the daemon socket", err))?;

        // Claimed before the reader starts, so it can tell its own connection
        // from the one that outlives it.
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        *self.stream.lock_or_recover() = Some(stream);

        let shared = Arc::clone(self);
        std::thread::Builder::new()
            .name("daemon-reader".into())
            .spawn(move || {
                // One reader thread demultiplexes the stream: replies go to
                // whoever is waiting for that id, everything else is an event.
                for line in BufReader::new(reader_half).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<Message>(&line) {
                        Ok(Message::Response(Response { id, outcome })) => {
                            if let Some(waiting) = shared.pending.lock_or_recover().remove(&id) {
                                let _ = waiting.send(outcome);
                            }
                        }
                        Ok(Message::Event(event)) => shared.events.event(event),
                        Err(err) => {
                            tracing::warn!(error = %err, "could not read a daemon message");
                        }
                    }
                }

                shared.handle_disconnect(generation);
            })
            .map_err(|err| CoreError::session("could not start the daemon reader", err))?;

        Ok(())
    }

    fn handle_disconnect(self: &Arc<Self>, generation: u64) {
        // A newer connection is already in place; this reader is reporting the
        // end of a stream nobody is using any more.
        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }

        *self.stream.lock_or_recover() = None;

        // Wake anything still waiting rather than leaving it to time out.
        for (_, waiting) in self.pending.lock_or_recover().drain() {
            let _ = waiting.send(Outcome::Err("the daemon went away".into()));
        }

        // A daemon we are deliberately swapping out, or a client on its way
        // out: neither is a connection anybody needs to hear about losing.
        if self.replacing.load(Ordering::SeqCst) || self.stopped.load(Ordering::SeqCst) {
            return;
        }

        self.events.disconnected();
        self.start_reconnecting();
    }

    /// Keeps trying to get back, with a backoff that levels off rather than
    /// giving up: a window left open overnight should recover on its own.
    fn start_reconnecting(self: &Arc<Self>) {
        if self.reconnecting.swap(true, Ordering::SeqCst) {
            return;
        }

        let shared = Arc::clone(self);
        std::thread::Builder::new()
            .name("daemon-reconnect".into())
            .spawn(move || {
                tracing::info!("lost the session daemon; trying to get back");

                for attempt in 0.. {
                    if shared.stopped.load(Ordering::SeqCst) {
                        break;
                    }

                    let wait = RECONNECT_BACKOFF
                        .get(attempt)
                        .copied()
                        .unwrap_or_else(|| *RECONNECT_BACKOFF.last().expect("not empty"));
                    std::thread::sleep(wait);

                    // Asked again after the wait, which is most of the time
                    // this loop spends: a client dropped during it must not
                    // go on to start a daemon nobody will ever attach to.
                    if shared.stopped.load(Ordering::SeqCst) {
                        break;
                    }

                    if shared.open().is_ok() {
                        tracing::info!("back on the session daemon");
                        // Possibly a different daemon, so nothing holding a
                        // session id can assume it is still valid.
                        shared.events.reattached();
                        break;
                    }
                }

                shared.reconnecting.store(false, Ordering::SeqCst);
            })
            .ok();
    }

    fn request(self: &Arc<Self>, request: Request) -> Result<Reply> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver): (Sender<Outcome>, Receiver<Outcome>) = channel();
        self.pending.lock_or_recover().insert(id, sender);

        let line = serde_json::to_string(&Envelope { id, request })
            .map_err(|err| CoreError::session("could not encode a daemon request", err))?;

        {
            let mut guard = self.stream.lock_or_recover();
            let Some(stream) = guard.as_mut() else {
                self.pending.lock_or_recover().remove(&id);
                return Err(CoreError::invalid(
                    "not connected to the session daemon; still trying",
                ));
            };

            stream
                .write_all(line.as_bytes())
                .and_then(|_| stream.write_all(b"\n"))
                .and_then(|_| stream.flush())
                .map_err(|err| {
                    self.pending.lock_or_recover().remove(&id);
                    CoreError::session("could not reach the session daemon", err)
                })?;
        }

        match receiver.recv_timeout(REQUEST_TIMEOUT) {
            Ok(Outcome::Ok(reply)) => Ok(reply),
            Ok(Outcome::Err(message)) => Err(CoreError::invalid(message)),
            Err(_) => {
                self.pending.lock_or_recover().remove(&id);
                Err(CoreError::invalid("the session daemon did not answer"))
            }
        }
    }
}

fn unexpected() -> CoreError {
    CoreError::invalid("the session daemon answered with something unexpected")
}

fn connect_once(socket: &Path) -> Option<LocalStream> {
    LocalStream::connect(socket).ok()
}

/// Starts the daemon detached, so it is not a child that dies with us.
fn spawn_daemon(binary: &Path, socket: &Path) -> Result<()> {
    use std::process::{Command, Stdio};

    if !binary.exists() {
        return Err(CoreError::invalid(format!(
            "could not find the session daemon at {}",
            binary.display()
        )));
    }

    let directory = socket
        .parent()
        .ok_or_else(|| CoreError::invalid("the socket path has no directory"))?;

    let mut command = Command::new(binary);
    command
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null());

    // Its stderr goes to a file beside the socket rather than to nowhere.
    //
    // A daemon that will not start says why — `path must be shorter than
    // SUN_LEN`, `another daemon is already listening` — and that sentence used
    // to be thrown away, leaving the window with "the session daemon did not
    // start listening" in every panel and nothing to act on.
    //
    // Truncated on each start, so it holds one run and cannot grow without
    // bound. Safe to write because the daemon already never logs a payload:
    // what it carries is whatever is on the user's screen.
    match std::fs::File::create(log_path(directory)) {
        Ok(file) => {
            command.stderr(file);
        }
        // Not being able to log is not a reason not to start.
        Err(_) => {
            command.stderr(Stdio::null());
        }
    }

    detach(command).map_err(|err| CoreError::session("could not start the session daemon", err))
}

/// Starts the daemon in a new session, so it survives the window closing and
/// does not receive the signals sent to Beacon's process group.
#[cfg(unix)]
fn detach(mut command: std::process::Command) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            // Detaches from the controlling terminal and the process group.
            if libc_setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().map(|_| ())
}

/// Starts the daemon with no console and in a process group of its own.
///
/// The Windows equivalent of a new session. Without a console it cannot be
/// handed a Ctrl+C meant for whatever terminal launched Beacon, and it never
/// flashes a window of its own. It is also asked to leave any job object the
/// window belongs to: a launcher that kills its job when it exits would
/// otherwise take every running session with it. Not every job allows that,
/// and one that does not refuses the whole spawn, so the second attempt stays
/// in the job rather than not starting at all.
#[cfg(windows)]
fn detach(mut command: std::process::Command) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const ERROR_ACCESS_DENIED: i32 = 5;

    keep_standard_handles_to_ourselves();

    let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    command.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB);
    match command.spawn() {
        Ok(_) => Ok(()),
        Err(err) if err.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
            tracing::info!("this job does not allow breaking away; starting the daemon inside it");
            command.creation_flags(flags);
            command.spawn().map(|_| ())
        }
        Err(err) => Err(err),
    }
}

/// Stops this process's own stdin, stdout and stderr being inherited.
///
/// Windows hands a child every handle its parent marked inheritable, whatever
/// the child's own stdio was set to — and the standard handles a process is
/// started with are marked that way. A daemon that outlives the window would
/// then hold the window's stdout open for as long as it runs, and whoever is
/// reading the other end — `tauri dev`, a test runner, a pipe in a shell —
/// would wait for an end of output that never comes.
///
/// Process-wide, and harmless for later children: Rust duplicates a handle it
/// is asked to pass on rather than relying on this flag.
#[cfg(windows)]
fn keep_standard_handles_to_ourselves() {
    use windows_sys::Win32::Foundation::{
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        unsafe {
            let handle = GetStdHandle(which);
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

// `setsid` without pulling in a libc dependency for one call.
#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "setsid"]
    fn setsid_raw() -> i32;
}

#[cfg(unix)]
fn libc_setsid() -> i32 {
    unsafe { setsid_raw() }
}

fn wait_for_daemon(socket: &Path) -> Result<LocalStream> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(stream) = connect_once(socket) {
            return Ok(stream);
        }
        std::thread::sleep(Duration::from_millis(40));
    }

    // Whatever the daemon managed to say on its way out. It is the difference
    // between a message somebody can act on and one that only says the thing
    // they can already see.
    let said = socket.parent().and_then(complaint);
    Err(CoreError::invalid(match said {
        Some(said) => format!("the session daemon did not start listening: {said}"),
        None => "the session daemon did not start listening".to_string(),
    }))
}

/// Where a daemon started here writes what it could not do.
fn log_path(directory: &Path) -> PathBuf {
    directory.join("daemon.log")
}

/// The last thing the daemon complained about, if it complained.
///
/// The last line rather than the file: earlier lines are a daemon that started
/// fine and then said ordinary things, and the question being answered is why
/// this one did not. Trimmed of the timestamp and level tracing puts in front,
/// because neither helps and both crowd out the sentence.
fn complaint(directory: &Path) -> Option<String> {
    let text = std::fs::read_to_string(log_path(directory)).ok()?;
    let line = text.lines().rev().find(|line| !line.trim().is_empty())?;

    // `2026-09-17T10:00:10.481625Z ERROR could not listen error=…`
    let said = line
        .split_once(" ERROR ")
        .map(|(_, rest)| rest)
        .unwrap_or(line)
        .trim();

    // Kept short: this goes in a panel, not in a log viewer.
    let said: String = said.chars().take(200).collect();
    (!said.is_empty()).then_some(said)
}

/// Where the daemon binary sits relative to the running executable.
///
/// Beside it in a development build, and inside the bundle in a packaged one —
/// both of which are "next to the executable" on the platforms Beacon targets.
/// On Windows it is `beacon-daemon.exe`, and a path without the suffix names a
/// file that does not exist.
pub fn daemon_binary_path() -> PathBuf {
    let name = format!("beacon-daemon{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

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

    struct Ignored;

    impl DaemonEvents for Ignored {
        fn event(&self, _: Event) {}
        fn disconnected(&self) {}
        fn reattached(&self) {}
    }

    /// A client with nothing listening on its socket, and a "daemon" that is
    /// only a file: enough for `spawn_daemon` to get as far as creating the log
    /// it writes beside the socket, which is how a test can tell it was called.
    fn unattached(dir: &Path) -> Arc<Shared> {
        let binary = dir.join("not-a-daemon");
        std::fs::write(&binary, b"").unwrap();
        Shared::new(
            &binary,
            &dir.join(crate::transport::SOCKET_FILE),
            Arc::new(Ignored),
        )
    }

    fn tried_to_start_a_daemon(dir: &Path) -> bool {
        log_path(dir).exists()
    }

    /// The race the reconnect loop had: it asked whether the client was gone
    /// only before its wait, so a client dropped during the wait — which is
    /// most of the time the loop spends — still got a daemon started for it.
    #[test]
    fn a_client_dropped_while_the_reconnect_loop_waits_starts_no_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let shared = unattached(dir.path());

        shared.start_reconnecting();
        // Well inside the first wait of the backoff.
        std::thread::sleep(RECONNECT_BACKOFF[0] / 4);
        shared.stopped.store(true, Ordering::SeqCst);

        let deadline = Instant::now() + Duration::from_secs(5);
        while shared.reconnecting.load(Ordering::SeqCst) {
            assert!(
                Instant::now() < deadline,
                "the reconnect loop never stopped"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!tried_to_start_a_daemon(dir.path()));
    }

    /// The narrower window: dropped after the loop's last look, or while
    /// `open` waited its turn behind another attempt.
    #[test]
    fn opening_for_a_dropped_client_starts_no_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let shared = unattached(dir.path());
        shared.stopped.store(true, Ordering::SeqCst);

        assert!(shared.open().is_err());
        assert!(!tried_to_start_a_daemon(dir.path()));
    }

    /// The other side of both: a live client with no daemon does start one.
    /// Without this, the two above would pass just as well if nothing were
    /// ever started at all.
    #[test]
    fn opening_for_a_live_client_does_start_a_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let shared = unattached(dir.path());

        assert!(shared.open().is_err(), "the stand-in is not a daemon");
        assert!(tried_to_start_a_daemon(dir.path()));
    }
}
