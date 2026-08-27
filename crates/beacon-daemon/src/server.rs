use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beacon_core::clips::{Clip, ClipBook, ClipStore, now_seconds};
use beacon_core::domain::ProjectId;
use beacon_core::protocol::{
    Envelope, Event, Greeting, Outcome, PROTOCOL_VERSION, Reply, Request, Response,
};
use beacon_core::session::{SessionEvents, SessionId, SessionManager};

/// How long the daemon stays up with nothing to do.
///
/// It must outlive the window — that is the point — but a daemon with no
/// sessions and nobody attached is just a process nobody asked for.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Everything a connected client can be sent to.
type Clients = Arc<Mutex<Vec<Arc<Mutex<UnixStream>>>>>;

struct Broadcaster {
    clients: Clients,
}

impl Broadcaster {
    fn send(&self, event: &Event) {
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };

        // A client that has gone away is dropped rather than retried: it will
        // reattach and replay from the scrollback when it comes back.
        let mut clients = self.clients.lock_or_recover();
        clients.retain(|client| {
            let mut stream = client.lock_or_recover();
            stream
                .write_all(line.as_bytes())
                .and_then(|_| stream.write_all(b"\n"))
                .and_then(|_| stream.flush())
                .is_ok()
        });
    }
}

impl Daemon {
    /// Files a clip and writes the book straight through to disk.
    ///
    /// Written on every clip rather than on a timer: clips arrive a handful of
    /// times an hour, the write is a small atomic rename, and the failure this
    /// avoids — the daemon reaching its idle timeout with an unsaved clip — is
    /// exactly the one nobody would think to look for.
    fn file_clip(&self, clip: Clip) {
        self.clips.lock_or_recover().add(clip);
        self.persist_clips();
    }

    fn persist_clips(&self) {
        let book = self.clips.lock_or_recover().clone();
        if let Err(err) = self.clip_store.save(&book) {
            // Not fatal, and not worth failing the request over: the clip is in
            // memory and already on its way to the window, which is where the
            // user is about to copy it from.
            tracing::warn!(error = %err, "could not save the clip book");
        }
    }

    fn broadcast(&self, event: &Event) {
        Broadcaster {
            clients: Arc::clone(&self.clients),
        }
        .send(event);
    }
}

impl SessionEvents for Broadcaster {
    fn output(&self, id: &SessionId, project: &ProjectId, offset: u64, bytes: &[u8]) {
        use base64::Engine as _;
        self.send(&Event::Output {
            id: id.clone(),
            project: project.clone(),
            offset,
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }

    fn exited(&self, id: &SessionId, project: &ProjectId, code: Option<i32>) {
        self.send(&Event::Exit {
            id: id.clone(),
            project: project.clone(),
            code,
        });
    }
}

struct Daemon {
    socket: std::path::PathBuf,
    /// The last usage reported per project.
    ///
    /// Retained, unlike activity: a window that has just attached should see
    /// what a session costs immediately rather than waiting for its next turn.
    usage: Mutex<std::collections::BTreeMap<ProjectId, beacon_core::protocol::UsageReport>>,
    /// Things Claude produced for the user to paste elsewhere.
    ///
    /// Held here rather than in the window for the same reason sessions are:
    /// the window is the thing that closes. A clip filed while Beacon is not
    /// showing must still be there when it is.
    clips: Mutex<ClipBook>,
    /// The only writer of `clips.json`. See `ClipStore`.
    clip_store: ClipStore,
    sessions: Arc<SessionManager>,
    clients: Clients,
    attached: AtomicUsize,
    stopping: Arc<AtomicBool>,
    /// When the last client left, for the idle timeout.
    idle_since: Mutex<Option<Instant>>,
}

/// Accepts connections until asked to stop or left idle for long enough.
pub fn serve(listener: UnixListener, socket: std::path::PathBuf) {
    let clients: Clients = Arc::new(Mutex::new(Vec::new()));
    let events: Arc<dyn SessionEvents> = Arc::new(Broadcaster {
        clients: Arc::clone(&clients),
    });

    let sessions = Arc::new(SessionManager::new(events));
    sessions.set_hook_socket(socket.clone());

    let clip_store = ClipStore::open_default();

    let daemon = Arc::new(Daemon {
        socket: socket.clone(),
        usage: Mutex::new(std::collections::BTreeMap::new()),
        clips: Mutex::new(clip_store.load()),
        clip_store,
        sessions,
        clients,
        attached: AtomicUsize::new(0),
        stopping: Arc::new(AtomicBool::new(false)),
        idle_since: Mutex::new(Some(Instant::now())),
    });

    // The accept loop blocks, so the idle check gets its own thread and stops
    // the daemon by closing the socket out from under it.
    spawn_idle_watch(Arc::clone(&daemon));

    for stream in listener.incoming() {
        if daemon.stopping.load(Ordering::SeqCst) {
            break;
        }

        match stream {
            Ok(stream) => {
                let daemon = Arc::clone(&daemon);
                std::thread::spawn(move || handle(daemon, stream));
            }
            Err(err) => {
                if daemon.stopping.load(Ordering::SeqCst) {
                    break;
                }
                tracing::warn!(error = %err, "could not accept a connection");
            }
        }
    }

    let _ = std::fs::remove_file(&socket);
}

fn spawn_idle_watch(daemon: Arc<Daemon>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(15));
            if daemon.stopping.load(Ordering::SeqCst) {
                return;
            }

            let attached = daemon.attached.load(Ordering::SeqCst);
            let live = daemon.sessions.count();
            if attached > 0 || live > 0 {
                *daemon.idle_since.lock_or_recover() = None;
                continue;
            }

            let mut idle_since = daemon.idle_since.lock_or_recover();
            match *idle_since {
                Some(since) if since.elapsed() >= IDLE_TIMEOUT => {
                    drop(idle_since);
                    tracing::info!("nothing running and nobody attached; stopping");
                    stop(&daemon);
                    return;
                }
                Some(_) => {}
                None => *idle_since = Some(Instant::now()),
            }
        }
    });
}

/// Ends the accept loop by closing the socket it is blocked on.
fn stop(daemon: &Daemon) {
    daemon.stopping.store(true, Ordering::SeqCst);
    let _ = std::fs::remove_file(&daemon.socket);
    // Connecting wakes `incoming()`, which then sees the stopping flag.
    let _ = UnixStream::connect(&daemon.socket);
    std::process::exit(0);
}

fn handle(daemon: Arc<Daemon>, stream: UnixStream) {
    let Ok(reader_half) = stream.try_clone() else {
        return;
    };
    let writer = Arc::new(Mutex::new(stream));

    daemon.clients.lock_or_recover().push(Arc::clone(&writer));
    daemon.attached.fetch_add(1, Ordering::SeqCst);
    *daemon.idle_since.lock_or_recover() = None;
    tracing::info!(
        clients = daemon.attached.load(Ordering::SeqCst),
        "client attached"
    );

    for line in BufReader::new(reader_half).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        tracing::debug!(line = %line, "request");

        let envelope: Envelope = match serde_json::from_str(&line) {
            Ok(envelope) => envelope,
            Err(err) => {
                // Answer anyway. Staying silent leaves the client waiting for a
                // reply that is never coming, which turns a clear protocol bug
                // into a mysterious twenty-second hang.
                tracing::warn!(error = %err, "could not read a request");
                if let Some(id) = request_id(&line) {
                    reply(
                        &writer,
                        Response {
                            id,
                            outcome: Outcome::Err(format!("the daemon could not read that: {err}")),
                        },
                    );
                }
                continue;
            }
        };

        let shutting_down = matches!(envelope.request, Request::Shutdown {});
        let outcome = dispatch(&daemon, envelope.request);
        reply(
            &writer,
            Response {
                id: envelope.id,
                outcome,
            },
        );

        if shutting_down {
            tracing::info!("asked to stop");
            stop(&daemon);
            return;
        }
    }

    daemon
        .clients
        .lock_or_recover()
        .retain(|client| !Arc::ptr_eq(client, &writer));
    let remaining = daemon.attached.fetch_sub(1, Ordering::SeqCst) - 1;
    if remaining == 0 {
        *daemon.idle_since.lock_or_recover() = Some(Instant::now());
    }
    // Sessions are deliberately left running: a client detaching is a window
    // closing, not work being abandoned.
    tracing::info!(
        clients = remaining,
        sessions = daemon.sessions.count(),
        "client detached"
    );
}

/// Digs the correlation id out of a request the daemon could not otherwise
/// parse, so it can still be answered.
fn request_id(line: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("id")?
        .as_u64()
}

fn reply(writer: &Arc<Mutex<UnixStream>>, response: Response) {
    // A reply that cannot be encoded must not vanish: the client would wait for
    // it until the request timed out, and the real fault would be invisible.
    let line = match serde_json::to_string(&response) {
        Ok(line) => line,
        Err(err) => {
            tracing::error!(error = %err, id = response.id, "could not encode a reply");
            serde_json::to_string(&Response {
                id: response.id,
                outcome: Outcome::Err(format!("the daemon could not encode its reply: {err}")),
            })
            .unwrap_or_default()
        }
    };
    let mut stream = writer.lock_or_recover();
    let _ = stream.write_all(line.as_bytes());
    let _ = stream.write_all(b"\n");
    let _ = stream.flush();
}

fn dispatch(daemon: &Daemon, request: Request) -> Outcome {
    use base64::Engine as _;

    let sessions = &daemon.sessions;

    let result = match request {
        Request::Hello { version } => {
            if version != PROTOCOL_VERSION {
                // Not an error the client should retry: it has to replace us.
                tracing::info!(
                    client = version,
                    ours = PROTOCOL_VERSION,
                    "version mismatch"
                );
            }
            Ok(Reply::Greeting(Greeting {
                version: PROTOCOL_VERSION,
                pid: std::process::id(),
                sessions: sessions.count(),
            }))
        }

        Request::Ensure {
            project,
            kind,
            slot,
            cwd,
            cols,
            rows,
            shell,
        } => sessions
            .ensure(&project, kind, slot, &cwd, (cols, rows), shell.as_ref())
            .and_then(|id| sessions.info(&id))
            .map(Reply::Session),

        Request::Write { id, data } => sessions.write(&id, data.as_bytes()).map(|_| Reply::Done),

        Request::Resize { id, cols, rows } => sessions.resize(&id, cols, rows).map(|_| Reply::Done),

        Request::Scrollback { id } => {
            sessions
                .scrollback(&id)
                .map(|(bytes, end_offset)| Reply::Scrollback {
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                    end_offset,
                })
        }

        Request::Close { id } => sessions.close(&id).map(|_| Reply::Done),

        Request::Restart {
            project,
            kind,
            slot,
            cwd,
            cols,
            rows,
            shell,
        } => sessions
            .restart_for(&project, kind, slot, &cwd, (cols, rows), shell.as_ref())
            .and_then(|id| sessions.info(&id))
            .map(Reply::Session),

        Request::CloseProject { project } => sessions.close_project(&project).map(|_| Reply::Done),

        Request::Report {
            project,
            activity,
            detail,
        } => {
            // Straight through to every window. The daemon does not keep this:
            // it is what a project is doing *now*, and a client that was not
            // connected has nothing to catch up on.
            daemon.broadcast(&Event::Activity {
                project,
                activity,
                detail,
            });
            Ok(Reply::Done)
        }

        Request::ReportUsage { usage } => {
            daemon
                .usage
                .lock_or_recover()
                .insert(usage.project.clone(), usage.clone());
            daemon.broadcast(&Event::Usage(usage));
            Ok(Reply::Done)
        }

        Request::Usage {} => Ok(Reply::Usage {
            reports: daemon.usage.lock_or_recover().values().cloned().collect(),
        }),

        Request::Clip {
            project,
            title,
            body,
            kind,
        } => {
            // Never logged, at any level. A clip is an API key as often as it
            // is an email, and the whole point is that the user chose where it
            // goes.
            Clip::new(project, title, body, kind, now_seconds()).map(|clip| {
                daemon.file_clip(clip.clone());
                daemon.broadcast(&Event::Clip(clip));
                Reply::Done
            })
        }

        Request::Clips {} => Ok(Reply::Clips {
            clips: daemon.clips.lock_or_recover().clips().to_vec(),
        }),

        Request::ForgetClips { id } => {
            let remaining = {
                let mut book = daemon.clips.lock_or_recover();
                book.forget(id.as_ref());
                book.clips().to_vec()
            };
            daemon.persist_clips();
            // The whole book, not a delta: it is small, and a drawer rebuilt
            // from the truth cannot drift from one that missed an event.
            daemon.broadcast(&Event::Clips {
                clips: remaining.clone(),
            });
            Ok(Reply::Clips { clips: remaining })
        }

        Request::List {} => Ok(Reply::Sessions {
            sessions: sessions.list(),
        }),

        Request::Shutdown {} => Ok(Reply::Done),
    };

    match result {
        Ok(reply) => Outcome::Ok(reply),
        Err(error) => Outcome::Err(error.to_string()),
    }
}

/// Locks that recover from a panic elsewhere rather than poisoning the daemon.
///
/// One client's thread failing must not take every session with it.
trait LockOrRecover<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockOrRecover<T> for Mutex<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
