use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::clips::{Clip, ClipKind};
use crate::domain::{ClipId, ProjectId};
use crate::session::{SessionId, SessionInfo, SessionKind};
use crate::settings::ShellSpec;

/// The wire contract between Beacon and its session daemon.
///
/// Bumped whenever a message changes shape — including when one is *added*,
/// which is the case that is easy to forget: a daemon from an older build
/// simply rejects the new request, and the version check that exists to replace
/// it never fires because nobody moved the number.
///
/// A client that finds a daemon speaking a different version asks it to quit
/// and starts one it understands, rather than guessing: a half-understood
/// session is worse than a new one.
///
/// Version 2 added `Report`, `ReportUsage` and `Usage`.
/// Version 3 gave sessions a slot, so a project can hold several terminals, and
/// let the client say which shell to run.
/// Version 4 added `Clip`, `Clips` and `ForgetClips` — the drawer of things to
/// copy. Three new requests, so by the rule above the number had to move, and
/// upgrading to it replaces a running daemon and the sessions it holds. Paid
/// once, knowingly: the alternative is an older daemon rejecting every clip
/// while the window shows an empty drawer and no reason for it.
///
/// `ClaudeActivity::Idle` was added without a version, deliberately. The rule
/// is about the set of requests: a daemon that meets one it does not know
/// leaves a client waiting on a session it cannot use. A value it does not know
/// inside a `Report` costs one dropped report — the daemon answers with an
/// error and carries on, and the tab keeps saying what it already said. Paying
/// for that with a forced daemon replacement would kill every running session
/// on upgrade, which is a far worse trade than a report that goes missing until
/// the daemon is next restarted.
pub const PROTOCOL_VERSION: u32 = 4;

/// Newline-delimited JSON, one message per line.
///
/// Chosen over anything framed or binary because the traffic is small, the
/// contents are inspectable with `nc` when something goes wrong, and the whole
/// codec is two lines of `serde_json`.
/// What a Claude session is doing, as reported by Claude Code itself.
///
/// These come from hooks rather than from reading the terminal. Milestone 3
/// left `dev server` and `error` unimplemented because inferring them from
/// output is guesswork; this is the difference between guessing and being told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaudeActivity {
    /// Running a tool, or thinking.
    Working,
    /// Stopped and waiting for the user — a permission prompt, a question.
    /// The state worth interrupting someone for.
    Waiting,
    /// Finished its turn.
    Done,
    /// Open, with nothing claimed about it — a session that has just started,
    /// resumed, or been cleared.
    ///
    /// Worth a state of its own rather than reusing `Done`: `waiting` and
    /// `done` never expire, because both can honestly last hours. That makes a
    /// session resumed after a permission prompt keep shouting for attention it
    /// no longer needs, and a fresh session claim it finished something it
    /// never started. This says only that Claude is there.
    Idle,
    /// The session ended.
    Ended,
}

/// What a Claude session is costing, as Claude Code itself reports it.
///
/// Every field is optional because Claude Code fills in what it knows: rate
/// limits are absent on plans without them, and the context window is unknown
/// until the first turn. A missing number is shown as missing rather than as
/// zero, which would read as "you have used none of it".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub project: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// How much of the context window this session is using, 0..100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_used_percentage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_used_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_size: Option<u64>,
    /// How much of the five-hour allowance is gone, 0..100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_used_percentage: Option<f32>,
    /// Unix seconds when that window resets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_resets_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_used_percentage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum Request {
    /// First message on a connection. Establishes that both sides agree.
    Hello { version: u32 },
    /// Returns the project's session of this kind, starting one if needed.
    #[serde(rename_all = "camelCase")]
    Ensure {
        project: ProjectId,
        kind: SessionKind,
        /// Which of the project's sessions of this kind.
        #[serde(default)]
        slot: u32,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        /// What to run. Sent by the client rather than read by the daemon, so a
        /// session starts with the shell configured now — not the one that was
        /// configured when the daemon happened to start.
        #[serde(default)]
        shell: Option<ShellSpec>,
    },
    #[serde(rename_all = "camelCase")]
    Write { id: SessionId, data: String },
    #[serde(rename_all = "camelCase")]
    Resize { id: SessionId, cols: u16, rows: u16 },
    /// Everything the session has produced, for rebuilding a view.
    #[serde(rename_all = "camelCase")]
    Scrollback { id: SessionId },
    #[serde(rename_all = "camelCase")]
    Close { id: SessionId },
    #[serde(rename_all = "camelCase")]
    Restart {
        project: ProjectId,
        kind: SessionKind,
        #[serde(default)]
        slot: u32,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        #[serde(default)]
        shell: Option<ShellSpec>,
    },
    #[serde(rename_all = "camelCase")]
    CloseProject { project: ProjectId },
    /// Reported by a Claude Code hook running inside a session.
    ///
    /// Sent by a short-lived process, not by the window: the hook connects,
    /// says one thing, and exits.
    #[serde(rename_all = "camelCase")]
    Report {
        project: ProjectId,
        activity: ClaudeActivity,
        /// What it is doing, when there is something worth naming — the tool it
        /// just started, for instance.
        detail: Option<String>,
    },
    /// Reported by Claude Code's status line, running inside a session.
    #[serde(rename_all = "camelCase")]
    ReportUsage { usage: UsageReport },
    /// Filed by the MCP server running inside a Claude session: something the
    /// user asked for in order to paste it somewhere else.
    ///
    /// The daemon stamps the id and the time rather than the sender. The sender
    /// is a process that lives for one call and has no way to know what is
    /// already in the book, and two clips filed in the same second by different
    /// sessions must still be distinguishable.
    #[serde(rename_all = "camelCase")]
    Clip {
        project: ProjectId,
        title: String,
        body: String,
        #[serde(default)]
        kind: ClipKind,
    },
    /// Everything in the clip book, newest first.
    ///
    /// Kept by the daemon and written to disk, unlike activity: a clip is worth
    /// something precisely *after* the turn that produced it, which is the case
    /// activity explicitly is not.
    Clips {},
    /// Drops one clip, or the whole book when `id` is absent.
    #[serde(rename_all = "camelCase")]
    ForgetClips {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<ClipId>,
    },
    /// The last usage reported for each project.
    ///
    /// Unlike activity, this is worth keeping: a window that has just attached
    /// should show what it costs now, not wait for the next turn to find out.
    Usage {},
    /// Which sessions are alive, so a reattaching client can find its work.
    ///
    /// Carries a body it does not need: a unit variant serialises without a
    /// `params` field, and `#[serde(flatten)]` cannot read an adjacently tagged
    /// enum back without one.
    List {},
    /// Asks the daemon to stop. Used when a client finds a version it does not
    /// speak.
    Shutdown {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// Correlates a reply with its request.
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Greeting {
    pub version: u32,
    pub pid: u32,
    /// How many sessions were already running when this client arrived.
    pub sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum Reply {
    Greeting(Greeting),
    Session(SessionInfo),
    #[serde(rename_all = "camelCase")]
    Scrollback {
        /// Base64-encoded bytes. PTY output is not guaranteed to be valid UTF-8
        /// at a chunk boundary, so it is never coerced into a string.
        data: String,
        /// Stream offset just past the snapshot.
        end_offset: u64,
    },
    /// A struct variant, not a newtype around the list: an internally tagged
    /// enum cannot carry a bare sequence, and serde only finds out at runtime.
    #[serde(rename_all = "camelCase")]
    Sessions {
        sessions: Vec<SessionInfo>,
    },
    /// A struct variant for the same reason as `Sessions`.
    #[serde(rename_all = "camelCase")]
    Usage {
        reports: Vec<UsageReport>,
    },
    /// A struct variant for the same reason as `Sessions`.
    #[serde(rename_all = "camelCase")]
    Clips {
        clips: Vec<Clip>,
    },
    Done,
}

/// What comes back for one request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub id: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Ok(Reply),
    Err(String),
}

/// Sent to every connected client, unprompted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Output {
        id: SessionId,
        project: ProjectId,
        offset: u64,
        /// Base64-encoded bytes.
        data: String,
    },
    #[serde(rename_all = "camelCase")]
    Exit {
        id: SessionId,
        project: ProjectId,
        code: Option<i32>,
    },
    /// A project's Claude session reported what it is costing.
    Usage(UsageReport),
    /// A clip was filed, by this window's session or by another's.
    ///
    /// Broadcast rather than answered to the sender: the sender is the MCP
    /// server, which is not the thing that shows the drawer.
    Clip(Clip),
    /// Every clip was dropped, or one was. Carries the book rather than a
    /// delta, because it is small and a drawer rebuilt from the truth cannot
    /// drift from one that missed an event.
    #[serde(rename_all = "camelCase")]
    Clips { clips: Vec<Clip> },
    /// A project's Claude session said what it is doing.
    #[serde(rename_all = "camelCase")]
    Activity {
        project: ProjectId,
        activity: ClaudeActivity,
        detail: Option<String>,
    },
}

/// One line from the daemon: either a reply, or something that just happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Response(Response),
    Event(Event),
}

/// Where the daemon listens.
///
/// A socket under the per-user temporary directory rather than the config
/// directory: it is runtime state, it should not be synced, and it should not
/// survive a reboot. Access control is the containing directory's permissions —
/// the socket is only reachable by the user who owns it.
pub fn socket_path() -> PathBuf {
    socket_dir().join("daemon.sock")
}

pub fn socket_dir() -> PathBuf {
    // On Linux this is shared between users, so the name has to distinguish
    // them. On macOS the temporary directory is already per-user.
    let user = std::env::var("USER").unwrap_or_else(|_| "beacon".to_string());
    std::env::temp_dir().join(format!("beacon-split-{user}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_survives_a_round_trip() {
        let envelope = Envelope {
            id: 7,
            request: Request::Ensure {
                project: ProjectId::generate(),
                kind: SessionKind::Claude,
                slot: 0,
                cwd: PathBuf::from("/tmp/project"),
                cols: 80,
                rows: 24,
                shell: None,
            },
        };

        let line = serde_json::to_string(&envelope).unwrap();
        let back: Envelope = serde_json::from_str(&line).unwrap();
        assert_eq!(back.id, 7);
        assert!(matches!(back.request, Request::Ensure { cols: 80, .. }));
    }

    /// Every variant, because the one that broke was the one not covered:
    /// a unit variant serialises without `params`, and flatten cannot read it
    /// back.
    #[test]
    fn every_request_survives_a_round_trip() {
        let project = ProjectId::generate();
        let id = SessionId("sn_x".into());
        let cwd = PathBuf::from("/tmp/project");

        let requests = vec![
            Request::Hello { version: 1 },
            Request::Ensure {
                project: project.clone(),
                kind: SessionKind::Shell,
                slot: 0,
                cwd: cwd.clone(),
                cols: 80,
                rows: 24,
                shell: None,
            },
            Request::Write {
                id: id.clone(),
                data: "ls\n".into(),
            },
            Request::Resize {
                id: id.clone(),
                cols: 100,
                rows: 40,
            },
            Request::Scrollback { id: id.clone() },
            Request::Close { id: id.clone() },
            Request::Restart {
                project: project.clone(),
                kind: SessionKind::Claude,
                slot: 0,
                cwd,
                cols: 80,
                rows: 24,
                shell: None,
            },
            Request::CloseProject { project },
            Request::List {},
            Request::Shutdown {},
            Request::Report {
                project: ProjectId("pj_y".into()),
                activity: ClaudeActivity::Waiting,
                detail: Some("Bash".into()),
            },
            Request::ReportUsage {
                usage: sample_usage(),
            },
            Request::Usage {},
            Request::Clip {
                project: ProjectId("pj_y".into()),
                title: "Staging keys".into(),
                body: "API_KEY=abc".into(),
                kind: ClipKind::Variable,
            },
            Request::Clips {},
            Request::ForgetClips {
                id: Some(ClipId("cl_x".into())),
            },
        ];

        // A guard, not a formality. Adding a request without moving
        // PROTOCOL_VERSION leaves older daemons rejecting it instead of being
        // replaced, which is exactly what happened once already.
        assert_eq!(
            requests.len(),
            16,
            "the set of requests changed: PROTOCOL_VERSION must change with it"
        );

        for (index, request) in requests.into_iter().enumerate() {
            let envelope = Envelope {
                id: index as u64,
                request,
            };
            let line = serde_json::to_string(&envelope).unwrap();
            let back: Envelope = serde_json::from_str(&line)
                .unwrap_or_else(|err| panic!("{line} did not round-trip: {err}"));
            assert_eq!(back.id, index as u64);
        }
    }

    #[test]
    fn a_reply_and_an_event_are_told_apart_on_the_same_stream() {
        let response = serde_json::to_string(&Response {
            id: 1,
            outcome: Outcome::Ok(Reply::Done),
        })
        .unwrap();
        let event = serde_json::to_string(&Event::Exit {
            id: SessionId("sn_x".into()),
            project: ProjectId("pj_x".into()),
            code: Some(0),
        })
        .unwrap();

        assert!(matches!(
            serde_json::from_str::<Message>(&response).unwrap(),
            Message::Response(_)
        ));
        assert!(matches!(
            serde_json::from_str::<Message>(&event).unwrap(),
            Message::Event(_)
        ));
    }

    /// The mirror of the request test, and for the same reason: the variant
    /// that broke was a newtype around a `Vec`, which an internally tagged enum
    /// cannot serialise at all.
    #[test]
    fn every_reply_survives_a_round_trip() {
        let info = SessionInfo {
            id: SessionId("sn_x".into()),
            project: ProjectId("pj_x".into()),
            kind: SessionKind::Shell,
            slot: 0,
            cwd: "/tmp/project".into(),
            running: true,
        };

        let replies = vec![
            Reply::Greeting(Greeting {
                version: PROTOCOL_VERSION,
                pid: 1234,
                sessions: 2,
            }),
            Reply::Session(info.clone()),
            Reply::Scrollback {
                data: "aGk=".into(),
                end_offset: 12,
            },
            Reply::Sessions {
                sessions: vec![info],
            },
            Reply::Usage {
                reports: vec![sample_usage()],
            },
            Reply::Clips {
                clips: vec![sample_clip()],
            },
            Reply::Done,
        ];

        assert_eq!(
            replies.len(),
            7,
            "the set of replies changed: PROTOCOL_VERSION must change with it"
        );

        for (index, reply) in replies.into_iter().enumerate() {
            let response = Response {
                id: index as u64,
                outcome: Outcome::Ok(reply),
            };
            let line = serde_json::to_string(&response)
                .unwrap_or_else(|err| panic!("reply {index} could not be encoded: {err}"));
            let back: Response = serde_json::from_str(&line)
                .unwrap_or_else(|err| panic!("{line} did not round-trip: {err}"));
            assert_eq!(back.id, index as u64);
        }
    }

    #[test]
    fn an_error_outcome_carries_its_message() {
        let line = serde_json::to_string(&Response {
            id: 2,
            outcome: Outcome::Err("no such session".into()),
        })
        .unwrap();

        let back: Response = serde_json::from_str(&line).unwrap();
        match back.outcome {
            Outcome::Err(message) => assert_eq!(message, "no such session"),
            Outcome::Ok(_) => panic!("expected an error"),
        }
    }

    fn sample_clip() -> Clip {
        Clip {
            id: ClipId("cl_x".into()),
            project: ProjectId("pj_x".into()),
            title: "Staging keys".into(),
            body: "API_KEY=abc".into(),
            kind: ClipKind::Variable,
            created_at: 1_800_000_000,
        }
    }

    /// A clip event is a newtype around a struct inside an adjacently tagged
    /// enum — the exact shape that could not be serialised when `Sessions` was
    /// written as one, so it is worth a test rather than an assumption.
    #[test]
    fn a_clip_event_survives_the_same_stream_as_a_reply() {
        let event = serde_json::to_string(&Event::Clip(sample_clip())).unwrap();
        let back = serde_json::from_str::<Message>(&event).unwrap();
        match back {
            Message::Event(Event::Clip(clip)) => {
                assert_eq!(clip.body, "API_KEY=abc");
                // The body is what lands on the clipboard: it must survive the
                // wire byte for byte, newlines and all.
                let multiline = Clip {
                    body: "FOO=1\n  BAR=2".into(),
                    ..sample_clip()
                };
                let line = serde_json::to_string(&Event::Clip(multiline)).unwrap();
                match serde_json::from_str::<Message>(&line).unwrap() {
                    Message::Event(Event::Clip(clip)) => {
                        assert_eq!(clip.body, "FOO=1\n  BAR=2")
                    }
                    other => panic!("expected a clip, got {other:?}"),
                }
            }
            other => panic!("expected a clip event, got {other:?}"),
        }
    }

    #[test]
    fn a_clip_defaults_to_plain_text_when_the_sender_says_nothing() {
        // The MCP server may omit `kind`; a missing one must not be a parse
        // failure that drops the clip silently.
        let line = r#"{"id":1,"method":"clip","params":{"project":"pj_x","title":"t","body":"b"}}"#;
        let back: Envelope = serde_json::from_str(line).unwrap();
        match back.request {
            Request::Clip { kind, .. } => assert_eq!(kind, ClipKind::Text),
            other => panic!("expected a clip, got {other:?}"),
        }
    }

    fn sample_usage() -> UsageReport {
        UsageReport {
            project: ProjectId("pj_x".into()),
            model: Some("claude-sonnet-4-6".into()),
            context_used_percentage: Some(37.5),
            context_used_tokens: Some(75_000),
            context_size: Some(200_000),
            five_hour_used_percentage: Some(12.0),
            five_hour_resets_at: Some(1_800_000_000),
            seven_day_used_percentage: None,
            seven_day_resets_at: None,
        }
    }

    #[test]
    fn a_usage_report_keeps_the_difference_between_none_and_zero() {
        // Absent means "not known", which is not the same as "none used" — and
        // showing the second when you mean the first is a lie about how much
        // room is left.
        let report = sample_usage();
        let line = serde_json::to_string(&report).unwrap();
        assert!(!line.contains("sevenDay"), "got {line}");

        let back: UsageReport = serde_json::from_str(&line).unwrap();
        assert_eq!(back.seven_day_used_percentage, None);
        assert_eq!(back.five_hour_used_percentage, Some(12.0));
    }

    #[test]
    fn an_activity_event_is_told_apart_from_a_reply() {
        let event = serde_json::to_string(&Event::Activity {
            project: ProjectId("pj_x".into()),
            activity: ClaudeActivity::Waiting,
            detail: None,
        })
        .unwrap();

        assert!(matches!(
            serde_json::from_str::<Message>(&event).unwrap(),
            Message::Event(Event::Activity { .. })
        ));
    }

    #[test]
    fn the_socket_lives_outside_the_config_directory() {
        let path = socket_path();
        assert!(path.ends_with("daemon.sock"));
        assert!(
            !path.to_string_lossy().contains("Application Support"),
            "runtime state does not belong with synced configuration"
        );
    }
}
