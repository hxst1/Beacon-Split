use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
    /// The command to launch, resolved from the environment where it matters.
    fn command(self) -> CommandBuilder {
        match self {
            Self::Shell => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| default_shell().to_string());
                let mut cmd = CommandBuilder::new(shell);
                // A login shell, like every terminal emulator: without it macOS
                // GUI apps inherit a PATH that is missing most of your tools.
                cmd.arg("-l");
                cmd
            }
            Self::Claude => CommandBuilder::new("claude"),
        }
    }
}

fn default_shell() -> &'static str {
    if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    }
}

/// How the host is told about things the session does on its own.
///
/// Implemented by the Tauri layer today (which forwards to the webview) and by
/// the daemon's transport later. `beacon-core` never learns which.
pub trait SessionEvents: Send + Sync + 'static {
    /// `offset` is where this chunk starts in the session's lifetime stream, so
    /// a client that replayed a snapshot can tell what it has already seen.
    fn output(&self, id: &SessionId, offset: u64, bytes: &[u8]);
    fn exited(&self, id: &SessionId, code: Option<i32>);
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
}

impl SessionManager {
    pub fn new(events: Arc<dyn SessionEvents>) -> Self {
        Self {
            events,
            sessions: Mutex::new(HashMap::new()),
            by_project: Mutex::new(HashMap::new()),
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

        let mut command = kind.command();
        command.cwd(cwd);
        // Tell the program it is talking to a capable terminal; xterm.js is.
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");

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
                                events.output(&id, offset, bytes);
                            }
                            Err(err) => {
                                tracing::debug!(session = %id, error = %err, "pty read ended");
                                break;
                            }
                        }
                    }
                    events.exited(&id, None);
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

    /// Closes a session and starts a replacement for the same project and kind.
    pub fn restart(&self, id: &SessionId, size: (u16, u16)) -> Result<SessionId> {
        let (project, kind, cwd) = {
            let sessions = self.sessions.lock_or_recover();
            let session = sessions
                .get(id)
                .ok_or_else(|| CoreError::SessionNotFound(id.to_string()))?;
            (session.project.clone(), session.kind, session.cwd.clone())
        };

        self.close(id)?;
        let new_id = self.spawn(project.clone(), kind, &cwd, size)?;
        self.by_project
            .lock_or_recover()
            .insert((project, kind), new_id.clone());
        Ok(new_id)
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
