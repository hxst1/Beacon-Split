use std::sync::{Arc, Mutex};

use beacon_core::domain::ProjectId;
use beacon_core::{Beacon, SessionEvents, SessionId, SessionManager};
use tauri::{AppHandle, Emitter};

/// Beacon's persisted state is single-owner and synchronous; Tauri commands
/// arrive on a thread pool, so the lock lives here rather than inside the core.
///
/// The session manager does its own locking — it is shared with PTY reader
/// threads — so it is not behind this mutex.
pub struct AppState {
    beacon: Mutex<Beacon>,
    pub sessions: Arc<SessionManager>,
}

impl AppState {
    pub fn new(beacon: Beacon, app: AppHandle) -> Self {
        let events: Arc<dyn SessionEvents> = Arc::new(WebviewEvents { app });
        Self {
            beacon: Mutex::new(beacon),
            sessions: Arc::new(SessionManager::new(events)),
        }
    }

    /// Recovers from a panic in another command rather than poisoning the app.
    pub fn beacon(&self) -> std::sync::MutexGuard<'_, Beacon> {
        self.beacon
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Forwards session activity to the webview.
///
/// This is the only place that knows sessions are rendered in a window at all.
/// The daemon will provide a different implementation over its transport, and
/// `beacon-core` will not be able to tell.
struct WebviewEvents {
    app: AppHandle,
}

/// Payload for [`EVENT_OUTPUT`].
///
/// PTY output is arbitrary bytes — an escape sequence can be split mid-way
/// across reads — so it is base64-encoded rather than coerced into a string,
/// and decoded back to bytes before reaching xterm.js.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputPayload {
    id: SessionId,
    /// Travels with the event so the UI can show which project is busy without
    /// tracking sessions itself.
    project: ProjectId,
    /// Where this chunk starts in the session's lifetime stream.
    offset: u64,
    data: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExitPayload {
    id: SessionId,
    project: ProjectId,
    code: Option<i32>,
}

pub const EVENT_OUTPUT: &str = "session:output";
pub const EVENT_EXIT: &str = "session:exit";

impl SessionEvents for WebviewEvents {
    fn output(&self, id: &SessionId, project: &ProjectId, offset: u64, bytes: &[u8]) {
        use base64::Engine as _;
        let payload = OutputPayload {
            id: id.clone(),
            project: project.clone(),
            offset,
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        };
        // Never log the payload: this carries whatever is on the user's screen.
        if let Err(err) = self.app.emit(EVENT_OUTPUT, payload) {
            tracing::warn!(session = %id, error = %err, "could not deliver session output");
        }
    }

    fn exited(&self, id: &SessionId, project: &ProjectId, code: Option<i32>) {
        let payload = ExitPayload {
            id: id.clone(),
            project: project.clone(),
            code,
        };
        if let Err(err) = self.app.emit(EVENT_EXIT, payload) {
            tracing::warn!(session = %id, error = %err, "could not deliver session exit");
        }
    }
}
