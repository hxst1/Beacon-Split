use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::domain::ProjectId;
use crate::error::{CoreError, Result};
use crate::protocol::{
    Envelope, Event, Message, Outcome, PROTOCOL_VERSION, Reply, Request, Response, socket_path,
};
use crate::session::{SessionInfo, SessionKind};

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
    /// The connection dropped. Sessions are still running; this client is not.
    fn disconnected(&self);
}

/// A connection to the session daemon.
///
/// The UI holds one of these where it used to hold a `SessionManager`. The
/// methods are the same shape on purpose: what changed is where the sessions
/// live, not what can be done with them.
pub struct DaemonClient {
    writer: Mutex<UnixStream>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, Sender<Outcome>>>>,
}

impl DaemonClient {
    /// Connects, starting a daemon if none is listening.
    ///
    /// `daemon_binary` is where to find it; the caller knows, because in
    /// development it sits beside the app and in a bundle it is inside it.
    pub fn connect(daemon_binary: &Path, events: Arc<dyn DaemonEvents>) -> Result<Self> {
        let client = Self::open(daemon_binary, Arc::clone(&events))?;
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
        drop(client);

        spawn_daemon(daemon_binary)?;
        let replacement = Self::attach(wait_for_daemon()?, events)?;
        replacement.hello()?;
        Ok(replacement)
    }

    /// Connects to a running daemon, or starts one and waits for it.
    fn open(daemon_binary: &Path, events: Arc<dyn DaemonEvents>) -> Result<Self> {
        match connect_once() {
            Some(stream) => Self::attach(stream, events),
            None => {
                spawn_daemon(daemon_binary)?;
                Self::attach(wait_for_daemon()?, events)
            }
        }
    }

    fn attach(stream: UnixStream, events: Arc<dyn DaemonEvents>) -> Result<Self> {
        let reader_half = stream
            .try_clone()
            .map_err(|err| CoreError::session("could not use the daemon socket", err))?;

        let pending: Arc<Mutex<HashMap<u64, Sender<Outcome>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        {
            // One reader thread demultiplexes the stream: replies go to whoever
            // is waiting for that id, everything else is an event.
            let pending = Arc::clone(&pending);
            std::thread::Builder::new()
                .name("daemon-reader".into())
                .spawn(move || {
                    for line in BufReader::new(reader_half).lines() {
                        let Ok(line) = line else { break };
                        if line.trim().is_empty() {
                            continue;
                        }

                        match serde_json::from_str::<Message>(&line) {
                            Ok(Message::Response(Response { id, outcome })) => {
                                if let Some(waiting) = pending.lock_or_recover().remove(&id) {
                                    let _ = waiting.send(outcome);
                                }
                            }
                            Ok(Message::Event(event)) => events.event(event),
                            Err(err) => {
                                tracing::warn!(error = %err, "could not read a daemon message");
                            }
                        }
                    }

                    // Wake anything still waiting rather than leaving it to time out.
                    for (_, waiting) in pending.lock_or_recover().drain() {
                        let _ = waiting.send(Outcome::Err("the daemon went away".into()));
                    }
                    events.disconnected();
                })
                .map_err(|err| CoreError::session("could not start the daemon reader", err))?;
        }

        Ok(Self {
            writer: Mutex::new(stream),
            next_id: AtomicU64::new(1),
            pending,
        })
    }

    fn request(&self, request: Request) -> Result<Reply> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver): (Sender<Outcome>, Receiver<Outcome>) = channel();
        self.pending.lock_or_recover().insert(id, sender);

        let line = serde_json::to_string(&Envelope { id, request })
            .map_err(|err| CoreError::session("could not encode a daemon request", err))?;

        {
            let mut writer = self.writer.lock_or_recover();
            writer
                .write_all(line.as_bytes())
                .and_then(|_| writer.write_all(b"\n"))
                .and_then(|_| writer.flush())
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
        cwd: &Path,
        size: (u16, u16),
    ) -> Result<SessionInfo> {
        match self.request(Request::Ensure {
            project: project.clone(),
            kind,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
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
        cwd: &Path,
        size: (u16, u16),
    ) -> Result<SessionInfo> {
        match self.request(Request::Restart {
            project: project.clone(),
            kind,
            cwd: cwd.to_path_buf(),
            cols: size.0,
            rows: size.1,
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

    pub fn shutdown(&self) -> Result<()> {
        self.request(Request::Shutdown {}).map(|_| ())
    }
}

fn unexpected() -> CoreError {
    CoreError::invalid("the session daemon answered with something unexpected")
}

fn connect_once() -> Option<UnixStream> {
    UnixStream::connect(socket_path()).ok()
}

/// Starts the daemon detached, so it is not a child that dies with us.
fn spawn_daemon(binary: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    if !binary.exists() {
        return Err(CoreError::invalid(format!(
            "could not find the session daemon at {}",
            binary.display()
        )));
    }

    let mut command = Command::new(binary);
    command
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

fn wait_for_daemon() -> Result<UnixStream> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(stream) = connect_once() {
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
