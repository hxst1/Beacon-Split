use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beacon_core::agent::AgentKind;
use beacon_core::clips::{Clip, ClipBook, ClipStore, now_seconds};
use beacon_core::domain::ProjectId;
use beacon_core::error::{CoreError, Result};
use beacon_core::protocol::{
    ClaudeActivity, Envelope, Event, Greeting, Outcome, PROTOCOL_VERSION, Reply, Request, Response,
};
use beacon_core::session::{
    AgentLaunch, ConversationStart, SessionEvents, SessionId, SessionKind, SessionManager,
};
use beacon_core::settings::ShellSpec;
use beacon_core::sign_in::{self, Next, SignInWatch};
use beacon_core::transport::{LocalListener, LocalStream};
use beacon_core::workstreams::{Workstream, WorkstreamBook, WorkstreamId, WorkstreamStore};

/// How long the daemon stays up with nothing to do.
///
/// It must outlive the window — that is the point — but a daemon with no
/// sessions and nobody attached is just a process nobody asked for.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// How rarely what the status line reports reaches disk.
///
/// The status line runs on every assistant message. What it says about a
/// conversation — its model, how full it is — is worth keeping across a
/// restart and is not worth a write each time.
const WORKSTREAM_SAVE_INTERVAL: Duration = Duration::from_secs(30);

/// Everything a connected client can be sent to.
type Clients = Arc<Mutex<Vec<Arc<Mutex<LocalStream>>>>>;

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

    fn persist_workstreams(&self) {
        let book = self.workstreams.lock_or_recover().clone();
        *self.workstreams_saved_at.lock_or_recover() = Instant::now();
        if let Err(err) = self.workstream_store.save(&book) {
            // Not fatal: the book is in memory and the sessions it describes
            // are running. Losing it costs the names, not the work.
            tracing::warn!(error = %err, "could not save the workstream book");
        }
    }

    /// Writes the book, but not more often than [`WORKSTREAM_SAVE_INTERVAL`].
    ///
    /// For what the status line reports, which arrives on every assistant
    /// message. The model and the context percentage are worth keeping and not
    /// worth a disk write each; a daemon that is asked to stop writes them out
    /// on the way.
    fn persist_workstreams_soon(&self) {
        let due = self.workstreams_saved_at.lock_or_recover().elapsed() >= WORKSTREAM_SAVE_INTERVAL;
        if due {
            self.persist_workstreams();
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

    fn degraded(&self, project: &ProjectId, summary: &str) {
        self.send(&Event::Degraded {
            project: project.clone(),
            summary: summary.to_string(),
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
    /// Every project's Claude conversations, and which one it is in.
    workstreams: Mutex<WorkstreamBook>,
    /// The only writer of `workstreams.json`, for the same reason as the clips.
    workstream_store: WorkstreamStore,
    /// When the book last reached disk, so the status line cannot turn a write
    /// per assistant message into a write per assistant message.
    workstreams_saved_at: Mutex<Instant>,
    sessions: Arc<SessionManager>,
    /// Claude sessions that started before anyone signed in. See
    /// `beacon_core::sign_in`.
    sign_in: Mutex<SignInWatch>,
    clients: Clients,
    attached: AtomicUsize,
    stopping: Arc<AtomicBool>,
    /// When the last client left, for the idle timeout.
    idle_since: Mutex<Option<Instant>>,
}

/// Accepts connections until asked to stop or left idle for long enough.
pub fn serve(listener: LocalListener, socket: std::path::PathBuf) {
    let clients: Clients = Arc::new(Mutex::new(Vec::new()));
    let events: Arc<dyn SessionEvents> = Arc::new(Broadcaster {
        clients: Arc::clone(&clients),
    });

    let sessions = Arc::new(SessionManager::new(events));
    sessions.set_hook_socket(socket.clone());

    let clip_store = ClipStore::open_default();
    let workstream_store = WorkstreamStore::open_default();

    let daemon = Arc::new(Daemon {
        socket: socket.clone(),
        usage: Mutex::new(std::collections::BTreeMap::new()),
        clips: Mutex::new(clip_store.load()),
        clip_store,
        workstreams: Mutex::new(workstream_store.load()),
        workstream_store,
        workstreams_saved_at: Mutex::new(Instant::now()),
        sessions,
        sign_in: Mutex::new(SignInWatch::new()),
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
    // The last thing before the process ends, because what the status line
    // reported since the last throttled write is only in memory.
    daemon.persist_workstreams();
    let _ = std::fs::remove_file(&daemon.socket);
    // Connecting wakes `incoming()`, which then sees the stopping flag.
    let _ = LocalStream::connect(&daemon.socket);
    std::process::exit(0);
}

fn handle(daemon: Arc<Daemon>, stream: LocalStream) {
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
        // The length, not the line: a done report carries what Claude wrote,
        // and what a session says stays out of the logs.
        tracing::debug!(bytes = line.len(), "request");

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
        watch_sign_in(&daemon, &outcome);
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

/// Starts watching for a sign-in when a reply hands out a Claude session that
/// may be sitting on Claude Code's sign-in screen. See `beacon_core::sign_in`.
///
/// Read off the reply because every way of opening a Claude session ends in one
/// carrying it — ensure, restart, and the three workstream requests — so no new
/// way of opening one can be added and forget this. A session reattached to has
/// the same id, which the watch already knows and ignores.
fn watch_sign_in(daemon: &Arc<Daemon>, outcome: &Outcome) {
    let session = match outcome {
        Outcome::Ok(Reply::Session(session)) => session,
        Outcome::Ok(Reply::Workstream { session, .. }) => session,
        _ => return,
    };
    if session.kind != SessionKind::Claude || !session.running {
        return;
    }

    let start = daemon
        .sign_in
        .lock_or_recover()
        .observe(session.id.clone(), Instant::now());
    if start {
        spawn_sign_in_watch(Arc::clone(daemon));
    }
}

/// Asks Claude Code whether anyone is signed in, for as long as some session
/// is known or suspected to be waiting for it, then gets out of the way.
///
/// A thread that exists only while there is something to watch: on a machine
/// that is signed in, which is nearly always, it asks once and ends.
fn spawn_sign_in_watch(daemon: Arc<Daemon>) {
    let watcher = Arc::clone(&daemon);
    let spawned = std::thread::Builder::new()
        .name("sign-in-watch".into())
        .spawn(move || {
            let daemon = watcher;
            let mut wait = sign_in::FIRST_LOOK;
            loop {
                std::thread::sleep(wait);
                if daemon.stopping.load(Ordering::SeqCst) {
                    return;
                }

                let asked_at = Instant::now();
                // Found already: the session being watched was started with it.
                let answer = daemon
                    .sessions
                    .known_program(AgentKind::Claude)
                    .and_then(|claude| beacon_core::claude::signed_in(&claude));

                let next = daemon.sign_in.lock_or_recover().answer(
                    answer,
                    asked_at,
                    Instant::now(),
                    |id| daemon.sessions.is_alive(id),
                    |id| daemon.sessions.last_return(id),
                );
                match next {
                    Next::AskAgainIn(after) => wait = after,
                    Next::Stop => return,
                    Next::Restart(waiting) => {
                        restart_signed_in(&daemon, &waiting);
                        return;
                    }
                }
            }
        });

    if let Err(err) = spawned {
        // Not fatal: the panels behave exactly as they did before this
        // existed, and the next Claude session to start tries again.
        tracing::warn!(error = %err, "could not start watching for a sign-in");
        daemon.sign_in.lock_or_recover().abandon();
    }
}

/// Starts again the Claude sessions left on the sign-in screen, now that
/// someone has signed in, so each finds the credential Claude Code stored.
///
/// Exactly what the Resume button does — the same preparation, so the
/// conversation is resumed if anything was said in it — and then the window is
/// told, because the process it is showing has just gone.
fn restart_signed_in(daemon: &Arc<Daemon>, waiting: &[SessionId]) {
    for id in waiting {
        // Closed, or restarted by hand, while the answer was on its way.
        let (Ok(info), Some(cwd)) = (daemon.sessions.info(id), daemon.sessions.cwd(id)) else {
            continue;
        };
        let agents = daemon
            .sessions
            .agent_launch(&info.project, AgentKind::Claude)
            .is_none_or(|launch| launch.agents);

        prepare_agent(daemon, &info.project, AgentKind::Claude, agents);
        // `restart_for` replaces whatever is in the panel's place, so check that
        // it is still this session as late as possible: Resume pressed, or a
        // workstream switched, in the meantime has put a signed-in one there,
        // and that must not be the one thrown away.
        if daemon
            .sessions
            .current(&info.project, info.kind, info.slot)
            .as_ref()
            != Some(id)
        {
            continue;
        }
        match daemon.sessions.restart_for(
            &info.project,
            info.kind,
            info.slot,
            &cwd,
            (info.cols, info.rows),
            None,
        ) {
            Ok(restarted) => {
                tracing::info!(
                    project = info.project.as_str(),
                    "signed in; started a waiting session again"
                );
                daemon
                    .sign_in
                    .lock_or_recover()
                    .signed_in_already(restarted);
                daemon.broadcast(&Event::Restarted {
                    project: info.project,
                    kind: info.kind,
                    slot: info.slot,
                });
            }
            Err(err) => {
                tracing::warn!(
                    project = info.project.as_str(),
                    error = %err,
                    "could not start a waiting session again"
                );
            }
        }
    }
}

/// Digs the correlation id out of a request the daemon could not otherwise
/// parse, so it can still be answered.
/// Makes sure a project has a conversation to be in, and that the manager
/// knows how to start it.
///
/// Called before every Claude session starts, because the manager's idea of how
/// to start one lives in the process and the book lives on disk: a daemon that
/// has just come back knows which conversation a project was in, and nothing
/// else about it.
fn prepare_agent(daemon: &Daemon, project: &ProjectId, agent: AgentKind, agents: bool) {
    let created = {
        let mut book = daemon.workstreams.lock_or_recover();
        match book.current(project, agent) {
            Some(_) => false,
            None => {
                book.start(project.clone(), None, agent);
                true
            }
        }
    };
    if created {
        daemon.persist_workstreams();
    }

    let Some(stream) = daemon
        .workstreams
        .lock_or_recover()
        .current(project, agent)
        .cloned()
    else {
        return;
    };

    let stream = learn_from_disk(daemon, stream);
    let start = start_for(&daemon.workstreams.lock_or_recover(), &stream);
    set_launch(daemon, project, &stream, start, agents);
}

/// Marks a Claude conversation resumable when its transcript is on disk, even
/// though no hook ever said so.
///
/// The hooks are how Beacon usually learns a conversation exists, and they
/// are optional. A user who never installed them used to get `Session ID … is
/// already in use` on every start after the first, because the book still
/// said the conversation had never been written and `--session-id` was asked
/// for again. Claude Code's own transcript settles it either way.
fn learn_from_disk(daemon: &Daemon, stream: Workstream) -> Workstream {
    let written = stream.agent == AgentKind::Claude
        && !stream.resumable
        && stream
            .resume_id()
            .is_some_and(beacon_core::claude::conversation_written);
    if !written {
        return stream;
    }

    let mut book = daemon.workstreams.lock_or_recover();
    book.mark_resumable(&stream.id);
    let updated = book.get(&stream.id).cloned().unwrap_or(stream);
    drop(book);
    daemon.persist_workstreams();
    tracing::info!(conversation = %updated.id, "found the conversation on disk; resuming it");
    updated
}

/// Which flag the next start of a conversation uses.
///
/// A function of the conversation and nothing else. Everything it needs is
/// recorded on the workstream and survives a daemon that went away: deciding
/// this from what the project happened to be launched with last time is how it
/// went wrong before, in both of its halves.
///
/// `resumable` is the first question because it is the one Claude Code is
/// strict about — `--session-id` is refused once a conversation exists,
/// `--resume` until it does. It means "something has been said in it", not
/// "Beacon has started a process for it": a session opened and never typed into
/// writes nothing, and resuming it answers *"No conversation found with session
/// ID"*, which is what pressing Resume used to do.
///
/// `forked_from` is the second, and is read from the book rather than from the
/// launch the manager is holding. The manager's copy is per project and lives
/// only in the process, so looking there lost the ancestry of a fork nobody had
/// typed in yet as soon as the user glanced at another conversation and came
/// back — or as soon as the daemon was replaced. The book remembers, so the
/// fork is repeated and its history is carried, instead of being replaced by an
/// empty conversation wearing the same id.
///
/// Taking no launch is the guard: there is nothing here that a stale one could
/// mislead.
///
/// The ids it puts in the answer are the agent's, not Beacon's. They are the
/// same number for Claude Code, whose ids Beacon chooses; for Codex they are
/// whatever it reported, and until it has reported there is nothing to resume
/// by, however certain the book is that something was said.
fn start_for(book: &WorkstreamBook, stream: &Workstream) -> ConversationStart {
    if stream.resumable {
        return match stream.resume_id() {
            Some(id) => ConversationStart::Resume { id: id.to_string() },
            // Said in, but never identified. Starting fresh is the only honest
            // answer: resuming needs a name to resume by.
            None => ConversationStart::New,
        };
    }

    // A fork nobody has typed into yet is still a fork: starting it again has
    // to carry the history it was forked from, not replace it with an empty
    // conversation of the same id.
    match fork_from(book, stream) {
        Some(from) => ConversationStart::Fork { from },
        None => ConversationStart::New,
    }
}

/// The parent of a fork, named as the parent's own agent knows it.
fn fork_from(book: &WorkstreamBook, stream: &Workstream) -> Option<String> {
    let from = stream.forked_from.as_ref()?;

    match book.get(from) {
        Some(parent) => parent.resume_id().map(str::to_string),
        // The row is gone — the per-project cap drops the least recently used,
        // and a parent can be dropped while the conversation it names is alive
        // in the agent. Whether that is recoverable depends on who chose the
        // id: Beacon's own is the row's name and survives losing the row, while
        // an id Codex reported existed nowhere else.
        None => match stream.agent {
            AgentKind::Claude => Some(from.to_string()),
            AgentKind::Codex => None,
        },
    }
}

/// Absent means the client did not say, which is treated as yes.
fn wanted(agents: Option<bool>) -> bool {
    agents.unwrap_or(true)
}

fn set_launch(
    daemon: &Daemon,
    project: &ProjectId,
    stream: &Workstream,
    start: ConversationStart,
    agents: bool,
) {
    daemon.sessions.set_agent_launch(
        project.clone(),
        AgentLaunch {
            agent: stream.agent,
            session_id: stream.id.to_string(),
            name: stream.name.clone(),
            start,
            agents,
        },
    );
}

/// Replaces the project's session for this conversation's agent with one
/// started in that conversation.
fn into_agent(
    daemon: &Daemon,
    project: &ProjectId,
    stream: Workstream,
    cwd: &std::path::Path,
    size: (u16, u16),
    shell: Option<&ShellSpec>,
) -> Result<Reply> {
    let kind = SessionKind::for_agent(stream.agent);
    let id = daemon
        .sessions
        .restart_for(project, kind, 0, cwd, size, shell)?;

    Ok(Reply::Workstream {
        workstream: Box::new(stream),
        session: daemon.sessions.info(&id)?,
    })
}

/// Returns a project to a conversation it already has.
///
/// The guard that matters is the second one: two Claude processes in the same
/// conversation write over each other's transcript, and the first thing anyone
/// would notice is history going missing. `claude agents --json` is Claude
/// Code's own answer to what is running, so this is checked rather than
/// assumed — and when it cannot be asked, the resume goes ahead rather than
/// being blocked by a question nobody can answer.
fn resume_workstream(
    daemon: &Daemon,
    project: ProjectId,
    id: WorkstreamId,
    cwd: &std::path::Path,
    size: (u16, u16),
    shell: Option<&ShellSpec>,
    agents: bool,
) -> Result<Reply> {
    // Which agent's conversation this is decides which of the project's
    // current conversations it is being compared against, and which session
    // would have to be replaced.
    let Some(agent) = daemon
        .workstreams
        .lock_or_recover()
        .get(&id)
        .map(|stream| stream.agent)
    else {
        return Err(CoreError::invalid(
            "that conversation is not one of this project's",
        ));
    };
    let kind = SessionKind::for_agent(agent);

    let current = daemon
        .workstreams
        .lock_or_recover()
        .current(&project, agent)
        .map(|stream| stream.id.clone());

    // Already in it. Not a restart: killing a live agent to put it back where
    // it already was would throw away whatever it was in the middle of.
    if current.as_ref() == Some(&id) {
        prepare_agent(daemon, &project, agent, agents);
        let session = daemon
            .sessions
            .ensure(&project, kind, 0, cwd, size, shell)?;

        let stream = daemon
            .workstreams
            .lock_or_recover()
            .get(&id)
            .cloned()
            .ok_or_else(|| CoreError::invalid("there is no such conversation"))?;

        return Ok(Reply::Workstream {
            workstream: Box::new(stream),
            session: daemon.sessions.info(&session)?,
        });
    }

    if beacon_core::claude::is_running(id.as_str()) == Some(true) {
        return Err(CoreError::invalid(
            "that conversation is already open in another Claude. Close it there, or fork it to \
             carry its history into a new one.",
        ));
    }

    let stream = daemon
        .workstreams
        .lock_or_recover()
        .resume(&project, &id)
        .ok_or_else(|| CoreError::invalid("that conversation is not one of this project's"))?;
    daemon.persist_workstreams();

    let stream = learn_from_disk(daemon, stream);
    let start = start_for(&daemon.workstreams.lock_or_recover(), &stream);
    set_launch(daemon, &project, &stream, start, agents);
    into_agent(daemon, &project, stream, cwd, size, shell)
}

fn request_id(line: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("id")?
        .as_u64()
}

fn reply(writer: &Arc<Mutex<LocalStream>>, response: Response) {
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
            agents,
        } => {
            if let Some(agent) = kind.agent() {
                prepare_agent(daemon, &project, agent, wanted(agents));
            }
            sessions
                .ensure(&project, kind, slot, &cwd, (cols, rows), shell.as_ref())
                .and_then(|id| sessions.info(&id))
                .map(Reply::Session)
        }

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
            agents,
        } => {
            if let Some(agent) = kind.agent() {
                prepare_agent(daemon, &project, agent, wanted(agents));
            }
            sessions
                .restart_for(&project, kind, slot, &cwd, (cols, rows), shell.as_ref())
                .and_then(|id| sessions.info(&id))
                .map(Reply::Session)
        }

        Request::CloseProject { project } => sessions.close_project(&project).map(|_| Reply::Done),

        Request::Report {
            project,
            agent,
            activity,
            detail,
            session,
            reply,
        } => {
            // Two things can be learned from one report, and they are not the
            // same thing. Which conversation it came from is worth knowing at
            // any activity — for an agent that names its own conversations,
            // the first report is the only chance to connect the name it chose
            // to the one Beacon is holding. That a conversation *exists* is
            // narrower: only something inside a turn proves it, and `idle` is
            // the session merely opening, which writes nothing.
            if let Some(session) = session {
                let mut book = daemon.workstreams.lock_or_recover();
                let mut changed = false;

                if let Some(found) = book.attribute(&project, agent, &session) {
                    changed = found.learned;
                    if activity != ClaudeActivity::Idle {
                        changed |= book.mark_resumable(&found.id);
                    }
                }

                drop(book);
                if changed {
                    daemon.persist_workstreams();
                }
            }

            // Straight through to every window. The daemon does not keep this:
            // it is what a project is doing *now*, and a client that was not
            // connected has nothing to catch up on.
            daemon.broadcast(&Event::Activity {
                project,
                activity,
                detail,
                reply,
            });
            Ok(Reply::Done)
        }

        Request::ReportAgent {
            project,
            agent,
            agent_type,
            running,
            summary,
        } => {
            // Straight through and kept nowhere. A subagent that ran for twelve
            // seconds is worth seeing while it runs and worth nothing after,
            // and a window that was not connected has nothing to catch up on.
            daemon.broadcast(&Event::Agent {
                project,
                agent,
                agent_type,
                running,
                summary,
            });
            Ok(Reply::Done)
        }

        Request::ReportUsage { usage } => {
            // Folded into the conversation it names, matched on the session id
            // rather than on whichever one the project happens to be in — a
            // Claude somebody started in their own terminal reports through the
            // same status line.
            if daemon.workstreams.lock_or_recover().observe(&usage) {
                daemon.persist_workstreams_soon();
            }
            daemon
                .usage
                .lock_or_recover()
                .insert(usage.project.clone(), (*usage).clone());
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

        Request::Workstreams { project, agent } => {
            let book = daemon.workstreams.lock_or_recover();
            Ok(Reply::Workstreams {
                workstreams: book
                    .for_project(&project, agent)
                    .into_iter()
                    .cloned()
                    .collect(),
                current: book
                    .current(&project, agent)
                    .map(|stream| stream.id.clone()),
            })
        }

        Request::StartWorkstream {
            project,
            agent,
            name,
            cwd,
            cols,
            rows,
            shell,
            agents,
        } => {
            let stream = daemon
                .workstreams
                .lock_or_recover()
                .start(project.clone(), name, agent);
            daemon.persist_workstreams();
            set_launch(
                daemon,
                &project,
                &stream,
                ConversationStart::New,
                wanted(agents),
            );
            into_agent(daemon, &project, stream, &cwd, (cols, rows), shell.as_ref())
        }

        Request::ResumeWorkstream {
            project,
            id,
            cwd,
            cols,
            rows,
            shell,
            agents,
        } => resume_workstream(
            daemon,
            project,
            id,
            &cwd,
            (cols, rows),
            shell.as_ref(),
            wanted(agents),
        ),

        Request::ForkWorkstream {
            project,
            from,
            name,
            cwd,
            cols,
            rows,
            shell,
            agents,
        } => {
            // A conversation Claude Code has never seen cannot be resumed, and
            // `--fork-session` resumes before it forks. Refused here, where the
            // reason can be said, rather than in the terminal as whatever error
            // the CLI produces after the old session has already been closed.
            let parent = daemon.workstreams.lock_or_recover().get(&from).cloned();
            match parent {
                None => Err(CoreError::invalid(
                    "that conversation is not one of this project's",
                )),
                Some(parent) if !parent.resumable => Err(CoreError::invalid(
                    "nothing has been said in that conversation yet, so there is nothing to fork",
                )),
                Some(_) => {
                    let stream = daemon
                        .workstreams
                        .lock_or_recover()
                        .fork(&project, &from, name);
                    match stream {
                        None => Err(CoreError::invalid(
                            "that conversation is not one of this project's",
                        )),
                        Some(stream) => {
                            daemon.persist_workstreams();
                            set_launch(
                                daemon,
                                &project,
                                &stream,
                                ConversationStart::Fork {
                                    from: from.to_string(),
                                },
                                wanted(agents),
                            );
                            into_agent(daemon, &project, stream, &cwd, (cols, rows), shell.as_ref())
                        }
                    }
                }
            }
        }

        Request::RenameWorkstream { project, id, name } => {
            if !daemon
                .workstreams
                .lock_or_recover()
                .rename(&id, name.clone())
            {
                return Outcome::Err("there is no such conversation".into());
            }
            daemon.persist_workstreams();

            // The manager holds the name it would pass to `--name`, so it has
            // to hear about this too or the next start would carry the old one.
            // Which launch that is depends on the conversation's agent, so the
            // row is read first — and copied out, rather than held while the
            // manager's own lock is taken.
            let renamed = daemon.workstreams.lock_or_recover().get(&id).cloned();
            if let Some(stream) = renamed
                && let Some(launch) = daemon.sessions.agent_launch(&project, stream.agent)
                && launch.session_id == id.as_str()
            {
                daemon.sessions.set_agent_launch(
                    project.clone(),
                    AgentLaunch {
                        name: stream.name.clone(),
                        ..launch
                    },
                );
            }
            Ok(Reply::Done)
        }

        Request::Shutdown {} => {
            // On the way out, so what the status line reported since the last
            // throttled write is not lost.
            daemon.persist_workstreams();
            Ok(Reply::Done)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: &str = "cafb8c86-53eb-49c4-a8b8-609e5cbc0f49";
    const REPORTED: &str = "01a0a9d6-e991-70f0-8c36-aa613bd90216";

    /// A book holding one conversation, handed back with it.
    fn book_with(agent: AgentKind, resumable: bool) -> (WorkstreamBook, Workstream) {
        let mut book = WorkstreamBook::default();
        let stream = book.start(ProjectId("pj_x".into()), None, agent);
        if resumable {
            book.mark_resumable(&stream.id);
        }
        let stream = book.get(&stream.id).cloned().expect("just started");
        (book, stream)
    }

    #[test]
    fn a_conversation_nobody_has_spoken_in_is_created_rather_than_resumed() {
        // The bug this guards: Beacon had already started a Claude for this
        // conversation, so the launch said `Resume` — but nothing was typed,
        // Claude Code wrote nothing, and pressing Resume answered "No
        // conversation found with session ID".
        let (book, stream) = book_with(AgentKind::Claude, false);
        assert_eq!(start_for(&book, &stream), ConversationStart::New);
    }

    #[test]
    fn a_claude_conversation_that_exists_is_resumed_by_beacons_own_id() {
        let (book, stream) = book_with(AgentKind::Claude, true);
        assert_eq!(
            start_for(&book, &stream),
            ConversationStart::Resume {
                id: stream.id.to_string()
            }
        );
    }

    #[test]
    fn a_codex_conversation_is_resumed_by_the_id_codex_reported() {
        let (mut book, stream) = book_with(AgentKind::Codex, true);

        // Something has been said in it, but Codex has not said what it calls
        // it. There is nothing to resume by, so it has to start fresh rather
        // than be resumed by a number Codex would not recognise.
        assert_eq!(start_for(&book, &stream), ConversationStart::New);

        book.learn_session_id(&stream.id, REPORTED);
        let stream = book.get(&stream.id).cloned().unwrap();
        assert_eq!(
            start_for(&book, &stream),
            ConversationStart::Resume {
                id: REPORTED.into()
            }
        );
    }

    #[test]
    fn a_fork_nobody_has_spoken_in_is_forked_again_rather_than_emptied() {
        // Starting it as new would keep the id and lose the history it was
        // forked from, which is the whole reason it exists.
        let mut book = WorkstreamBook::default();
        let project = ProjectId("pj_x".into());
        let parent = book.start(project.clone(), None, AgentKind::Claude);
        book.mark_resumable(&parent.id);
        let forked = book.fork(&project, &parent.id, None).unwrap();

        assert_eq!(
            start_for(&book, &forked),
            ConversationStart::Fork {
                from: parent.id.to_string()
            }
        );
    }

    #[test]
    fn a_codex_fork_names_its_parent_as_codex_knows_it() {
        let mut book = WorkstreamBook::default();
        let project = ProjectId("pj_x".into());
        let parent = book.start(project.clone(), None, AgentKind::Codex);
        book.mark_resumable(&parent.id);
        book.learn_session_id(&parent.id, REPORTED);

        let forked = book.fork(&project, &parent.id, None).unwrap();

        assert_eq!(
            start_for(&book, &forked),
            ConversationStart::Fork {
                from: REPORTED.into()
            },
            "forking Beacon's id would name a conversation Codex never had"
        );
    }

    #[test]
    fn a_fork_whose_parent_has_no_reported_id_starts_fresh_instead() {
        let mut book = WorkstreamBook::default();
        let project = ProjectId("pj_x".into());
        let parent = book.start(project.clone(), None, AgentKind::Codex);
        let forked = book.fork(&project, &parent.id, None).unwrap();

        assert_eq!(start_for(&book, &forked), ConversationStart::New);
    }

    #[test]
    fn a_claude_fork_survives_losing_its_parents_row() {
        // The per-project cap drops the least recently used, and it can drop a
        // parent while the conversation it names is still alive in the agent.
        // Beacon chose that id, so the row is only where it was written down.
        let mut book = WorkstreamBook::default();
        let project = ProjectId("pj_x".into());
        let mut orphan = book.start(project.clone(), None, AgentKind::Claude);
        orphan.forked_from = Some(WorkstreamId(PARENT.into()));

        assert_eq!(
            start_for(&book, &orphan),
            ConversationStart::Fork {
                from: PARENT.into()
            }
        );
    }

    #[test]
    fn the_arguments_each_agent_is_started_with() {
        let claude = AgentLaunch {
            agent: AgentKind::Claude,
            session_id: PARENT.into(),
            name: Some("payments-bug".into()),
            start: ConversationStart::New,
            agents: true,
        };
        assert_eq!(
            claude.args(),
            ["--session-id", PARENT, "--name", "payments-bug"]
        );

        // Codex is told nothing: not the id, not the name. It reports both.
        let codex = AgentLaunch {
            agent: AgentKind::Codex,
            name: Some("payments-bug".into()),
            ..claude.clone()
        };
        assert!(codex.args().is_empty());

        let resumed = AgentLaunch {
            start: ConversationStart::Resume {
                id: REPORTED.into(),
            },
            ..codex.clone()
        };
        assert_eq!(resumed.args(), ["resume", REPORTED]);

        let forked = AgentLaunch {
            start: ConversationStart::Fork {
                from: REPORTED.into(),
            },
            ..codex
        };
        assert_eq!(forked.args(), ["fork", REPORTED]);
    }
}
