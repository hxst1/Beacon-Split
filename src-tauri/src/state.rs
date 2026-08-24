use std::sync::{Arc, Mutex};

use beacon_core::client::{DaemonClient, DaemonEvents, daemon_binary_path};
use beacon_core::protocol::Event;
use beacon_core::{Beacon, CoreError};
use tauri::{AppHandle, Emitter};

/// Beacon's persisted state is single-owner and synchronous; Tauri commands
/// arrive on a thread pool, so the lock lives here rather than inside the core.
///
/// Sessions are not here at all any more: they belong to the daemon, and this
/// holds a connection to it.
pub struct AppState {
    beacon: Mutex<Beacon>,
    /// `Err` when the daemon could not be reached. Beacon still runs — files,
    /// git and workspaces do not need it — and the reason is reported by the
    /// commands that do.
    daemon: Result<DaemonClient, String>,
}

impl AppState {
    pub fn new(beacon: Beacon, app: AppHandle) -> Self {
        let events: Arc<dyn DaemonEvents> = Arc::new(WebviewEvents { app });
        let daemon = DaemonClient::connect(&daemon_binary_path(), events).map_err(|err| {
            tracing::error!(error = %err, "could not reach the session daemon");
            err.to_string()
        });

        Self {
            beacon: Mutex::new(beacon),
            daemon,
        }
    }

    /// Recovers from a panic in another command rather than poisoning the app.
    pub fn beacon(&self) -> std::sync::MutexGuard<'_, Beacon> {
        self.beacon
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn daemon(&self) -> Result<&DaemonClient, CoreError> {
        self.daemon
            .as_ref()
            .map_err(|reason| CoreError::invalid(reason.clone()))
    }
}

/// Forwards what the daemon reports to the webview.
///
/// The only place that knows sessions are rendered in a window at all.
struct WebviewEvents {
    app: AppHandle,
}

pub const EVENT_OUTPUT: &str = "session:output";
pub const EVENT_EXIT: &str = "session:exit";
/// What a project's Claude session is doing, as Claude Code itself reports it.
pub const EVENT_ACTIVITY: &str = "session:activity";
/// Raised when the connection drops. Sessions keep running; this window is no
/// longer watching them.
pub const EVENT_DETACHED: &str = "session:detached";
/// Raised when a connection is live again — possibly to a different daemon, so
/// every session id the window was holding has to be asked for afresh.
pub const EVENT_REATTACHED: &str = "session:reattached";

impl DaemonEvents for WebviewEvents {
    fn event(&self, event: Event) {
        // Never log the payload: it carries whatever is on the user's screen.
        let delivered = match event {
            Event::Output { .. } => self.app.emit(EVENT_OUTPUT, event),
            Event::Exit { .. } => self.app.emit(EVENT_EXIT, event),
            Event::Activity { .. } => self.app.emit(EVENT_ACTIVITY, event),
        };

        if let Err(err) = delivered {
            tracing::warn!(error = %err, "could not deliver a session event");
        }
    }

    fn disconnected(&self) {
        tracing::warn!("the session daemon connection dropped");
        let _ = self.app.emit(EVENT_DETACHED, ());
    }

    fn reattached(&self) {
        tracing::info!("reattached to the session daemon");
        let _ = self.app.emit(EVENT_REATTACHED, ());
    }
}
