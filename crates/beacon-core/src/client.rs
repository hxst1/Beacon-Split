use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::domain::ProjectId;
use crate::error::{CoreError, Result};
use crate::protocol::{
    Envelope, Event, Message, Outcome, PROTOCOL_VERSION, Reply, Request, Response, socket_path,
};
use crate::session::{SessionInfo, SessionKind};
use crate::settings::ShellSpec;

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
    stream: Mutex<Option<UnixStream>>,
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
        let shared = Arc::new(Shared {
            stream: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            binary: daemon_binary.to_path_buf(),
            socket: socket.to_path_buf(),
            events,
            stopped: AtomicBool::new(false),
            reconnecting: AtomicBool::new(false),
        });

        let client = Self { shared };
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
        tracing::info!(
            theirs = greeting.version,
            ours = PROTOCOL_VERSION,
            "replacing a daemon speaking a different protocol"
        );
        let _ = client.shutdown();

        client.shared.open()?;
        client.hello()?;
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
        shell: Option<ShellSpec>,
    ) -> Result<SessionInfo> {
        match self.request(Request::Ensure {
            project: project.clone(),
            kind,
            slot,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
            shell,
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
        shell: Option<ShellSpec>,
    ) -> Result<SessionInfo> {
        match self.request(Request::Restart {
            project: project.clone(),
            kind,
            slot,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
            shell,
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
    /// Connects to a running daemon, or starts one and waits for it.
    fn open(self: &Arc<Self>) -> Result<()> {
        let stream = match connect_once(&self.socket) {
            Some(stream) => stream,
            None => {
                spawn_daemon(&self.binary, &self.socket)?;
                wait_for_daemon(&self.socket)?
            }
        };
        self.adopt(stream)
    }

    /// Takes over a connection and starts reading from it.
    fn adopt(self: &Arc<Self>, stream: UnixStream) -> Result<()> {
        let reader_half = stream
            .try_clone()
            .map_err(|err| CoreError::session("could not use the daemon socket", err))?;

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

                shared.handle_disconnect();
            })
            .map_err(|err| CoreError::session("could not start the daemon reader", err))?;

        Ok(())
    }

    fn handle_disconnect(self: &Arc<Self>) {
        *self.stream.lock_or_recover() = None;

        // Wake anything still waiting rather than leaving it to time out.
        for (_, waiting) in self.pending.lock_or_recover().drain() {
            let _ = waiting.send(Outcome::Err("the daemon went away".into()));
        }

        self.events.disconnected();
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }
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

fn connect_once(socket: &Path) -> Option<UnixStream> {
    UnixStream::connect(socket).ok()
}

/// Starts the daemon detached, so it is not a child that dies with us.
fn spawn_daemon(binary: &Path, socket: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
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
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // A new session, so it survives the window closing and does not receive the
    // signals sent to Beacon's process group.
    unsafe {
        command.pre_exec(|| {
            // Detaches from the controlling terminal and the process group.
            if libc_setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|err| CoreError::session("could not start the session daemon", err))
}

// `setsid` without pulling in a libc dependency for one call.
unsafe extern "C" {
    #[link_name = "setsid"]
    fn setsid_raw() -> i32;
}

fn libc_setsid() -> i32 {
    unsafe { setsid_raw() }
}

fn wait_for_daemon(socket: &Path) -> Result<UnixStream> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(stream) = connect_once(socket) {
            return Ok(stream);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Err(CoreError::invalid(
        "the session daemon did not start listening",
    ))
}

/// Where the daemon binary sits relative to the running executable.
///
/// Beside it in a development build, and inside the bundle in a packaged one —
/// both of which are "next to the executable" on the platforms Beacon targets.
pub fn daemon_binary_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("beacon-daemon")))
        .unwrap_or_else(|| PathBuf::from("beacon-daemon"))
}

trait LockOrRecover<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockOrRecover<T> for Mutex<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
