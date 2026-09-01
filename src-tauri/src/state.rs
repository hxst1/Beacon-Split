use std::sync::{Arc, Mutex};

use beacon_core::client::{DaemonClient, DaemonEvents, daemon_binary_path};
use beacon_core::domain::ProjectId;
use beacon_core::protocol::Event;
use beacon_core::session::SessionId;
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

/// What the window receives for each event.
///
/// Spelled out rather than reusing the protocol enum: the shapes the frontend
/// reads are part of this boundary, and sharing a type whose serialisation is
/// tagged for a different transport is how they silently stopped matching.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputPayload {
    id: SessionId,
    project: ProjectId,
    offset: u64,
    /// Base64-encoded bytes.
    data: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExitPayload {
    id: SessionId,
    project: ProjectId,
    code: Option<i32>,
}

/// The whole drawer, for the events that replace it rather than add to it.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ClipsPayload {
    clips: Vec<beacon_core::clips::Clip>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityPayload {
    project: ProjectId,
    activity: beacon_core::protocol::ClaudeActivity,
    detail: Option<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentPayload {
    project: ProjectId,
    agent: String,
    agent_type: Option<String>,
    running: bool,
    summary: Option<String>,
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
/// What a project's Claude session is costing.
pub const EVENT_USAGE: &str = "session:usage";
/// A subagent started or finished inside a project's Claude session.
pub const EVENT_AGENT: &str = "session:agent";
/// A clip was filed by a Claude session, for the user to copy.
pub const EVENT_CLIP: &str = "clips:added";
/// The drawer changed wholesale — something was forgotten, or all of it was.
pub const EVENT_CLIPS: &str = "clips:replaced";
/// Raised when the connection drops. Sessions keep running; this window is no
/// longer watching them.
pub const EVENT_DETACHED: &str = "session:detached";
/// Raised when a connection is live again — possibly to a different daemon, so
/// every session id the window was holding has to be asked for afresh.
pub const EVENT_REATTACHED: &str = "session:reattached";

impl DaemonEvents for WebviewEvents {
    fn event(&self, event: Event) {
        // Emitted as the payload the window expects, not as the enum that
        // carried it here. `Event` is adjacently tagged, so emitting it whole
        // wraps everything in `{event, data}` — and a listener reading `id` off
        // that finds nothing, drops the chunk, and shows an empty terminal
        // while the daemon fills up with output nobody sees.
        //
        // Never log the payload: it carries whatever is on the user's screen.
        let delivered = match event {
            Event::Output {
                id,
                project,
                offset,
                data,
            } => self.app.emit(
                EVENT_OUTPUT,
                OutputPayload {
                    id,
                    project,
                    offset,
                    data,
                },
            ),
            Event::Exit { id, project, code } => {
                self.app.emit(EVENT_EXIT, ExitPayload { id, project, code })
            }
            Event::Activity {
                project,
                activity,
                detail,
            } => self.app.emit(
                EVENT_ACTIVITY,
                ActivityPayload {
                    project,
                    activity,
                    detail,
                },
            ),
            Event::Usage(report) => self.app.emit(EVENT_USAGE, report),
            Event::Agent {
                project,
                agent,
                agent_type,
                running,
                summary,
            } => self.app.emit(
                EVENT_AGENT,
                AgentPayload {
                    project,
                    agent,
                    agent_type,
                    running,
                    summary,
                },
            ),
            // Never logged, at any level: a clip is as likely to be a token as
            // an email, and it exists precisely so the user chooses where it
            // goes.
            Event::Clip(clip) => self.app.emit(EVENT_CLIP, clip),
            Event::Clips { clips } => self.app.emit(EVENT_CLIPS, ClipsPayload { clips }),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The window reads `id`, `offset` and `data` off the top of the payload.
    ///
    /// This broke once by sharing the protocol enum with the webview: `Event`
    /// is adjacently tagged, so emitting it whole wraps everything in
    /// `{event, data}`, every listener found `id` undefined, and terminals sat
    /// empty while the daemon filled with output nobody saw. Nothing about
    /// reading the code showed it.
    #[test]
    fn event_payloads_are_flat() {
        let output = serde_json::to_value(OutputPayload {
            id: SessionId("sn_x".into()),
            project: ProjectId("pj_x".into()),
            offset: 12,
            data: "aGk=".into(),
        })
        .unwrap();

        assert_eq!(output.get("id").and_then(|v| v.as_str()), Some("sn_x"));
        assert_eq!(output.get("offset").and_then(|v| v.as_u64()), Some(12));
        assert!(
            output.get("event").is_none(),
            "a tag here would bury every field one level down: {output}"
        );

        let exit = serde_json::to_value(ExitPayload {
            id: SessionId("sn_x".into()),
            project: ProjectId("pj_x".into()),
            code: Some(0),
        })
        .unwrap();
        assert_eq!(exit.get("id").and_then(|v| v.as_str()), Some("sn_x"));
        assert!(exit.get("event").is_none());

        let clip = serde_json::to_value(beacon_core::clips::Clip {
            id: beacon_core::domain::ClipId("cl_x".into()),
            project: ProjectId("pj_x".into()),
            title: "Staging keys".into(),
            body: "API_KEY=abc".into(),
            kind: beacon_core::clips::ClipKind::Variable,
            created_at: 1_800_000_000,
        })
        .unwrap();
        assert_eq!(clip.get("id").and_then(|v| v.as_str()), Some("cl_x"));
        assert_eq!(
            clip.get("body").and_then(|v| v.as_str()),
            Some("API_KEY=abc")
        );
        assert_eq!(clip.get("kind").and_then(|v| v.as_str()), Some("variable"));
        assert!(clip.get("event").is_none());
        // The window reads `createdAt`; a snake_case field would be silently
        // undefined and every clip would claim to be from 1970.
        assert!(clip.get("createdAt").is_some(), "{clip}");

        let activity = serde_json::to_value(ActivityPayload {
            project: ProjectId("pj_x".into()),
            activity: beacon_core::protocol::ClaudeActivity::Waiting,
            detail: Some("Bash".into()),
        })
        .unwrap();
        assert_eq!(
            activity.get("project").and_then(|v| v.as_str()),
            Some("pj_x")
        );
        assert!(activity.get("event").is_none());
    }
}
