use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use beacon_core::domain::ProjectId;
use beacon_core::protocol::{ClaudeActivity, Envelope, Request};

/// Runs as a Claude Code hook and tells the daemon what the session is doing.
///
/// Registered once in the user's Claude settings and inert everywhere else: a
/// Claude started outside Beacon has no `BEACON_SOCKET`, so this exits without
/// doing anything. It must also never fail in a way Claude would notice — a
/// hook that errors is a hook that interferes with someone's work, and knowing
/// what a tab is doing is not worth that.
pub fn run() -> ! {
    let _ = report();
    // Always zero. Anything else changes how Claude behaves.
    std::process::exit(0)
}

fn report() -> Option<()> {
    let socket = std::env::var("BEACON_SOCKET").ok()?;
    let project = std::env::var("BEACON_PROJECT").ok()?;

    let mut payload = String::new();
    std::io::stdin().read_to_string(&mut payload).ok()?;
    let event: serde_json::Value = serde_json::from_str(&payload).ok()?;

    // A subagent is not the session, so it does not get a session state.
    if let Some(request) = agent_report(&event, &project) {
        return send(&socket, request);
    }

    let (activity, detail) = interpret(&event)?;
    let session = event
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    send(
        &socket,
        Request::Report {
            project: ProjectId(project),
            activity,
            detail,
            session,
        },
    )
}

/// A subagent starting or finishing, when that is what this event is.
///
/// `agent_type` is optional because Claude Code reports it empty in practice —
/// seen on a real `SubagentStop` — and an empty string on screen would read as
/// a nameless agent rather than as an unnamed one.
pub fn agent_report(event: &serde_json::Value, project: &str) -> Option<Request> {
    let running = match event.get("hook_event_name")?.as_str()? {
        "SubagentStart" => true,
        "SubagentStop" => false,
        _ => return None,
    };

    let text = |key: &str| -> Option<String> {
        let value = event.get(key)?.as_str()?.trim();
        (!value.is_empty()).then(|| value.to_string())
    };

    Some(Request::ReportAgent {
        project: ProjectId(project.to_string()),
        agent: text("agent_id")?,
        agent_type: text("agent_type"),
        running,
        // Only what fits on one line of a panel header. The whole message is a
        // subagent's final answer, which can be pages, and none of it belongs
        // in a status row.
        summary: text("last_assistant_message").map(|message| first_line(&message, 100)),
    })
}

/// The first line, cut to a length, without splitting a character in half.
fn first_line(text: &str, limit: usize) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    if line.chars().count() <= limit {
        return line.trim().to_string();
    }
    let cut: String = line.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}

fn send(socket: &str, request: Request) -> Option<()> {
    let line = serde_json::to_string(&Envelope {
        // Nothing is waiting for a reply, so the correlation id is a formality.
        id: 0,
        request,
    })
    .ok()?;

    let mut stream = UnixStream::connect(socket).ok()?;
    stream.write_all(line.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;
    stream.flush().ok()?;
    Some(())
}

/// Turns a hook event into what a tab should say.
///
/// Deliberately narrow. Only events that change what someone would do about a
/// project are worth a state; the rest are noise on a tab.
pub fn interpret(event: &serde_json::Value) -> Option<(ClaudeActivity, Option<String>)> {
    let name = event.get("hook_event_name")?.as_str()?;
    let tool = event
        .get("tool_name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    Some(match name {
        // The one worth interrupting someone for.
        "PermissionRequest" | "Notification" | "Elicitation" => (ClaudeActivity::Waiting, tool),
        "PreToolUse" => (ClaudeActivity::Working, tool),
        "UserPromptSubmit" => (ClaudeActivity::Working, None),
        "Stop" | "StopFailure" => (ClaudeActivity::Done, None),
        // Fires on startup, on resume, and after a clear or a compact. Its job
        // is to take back whatever the tab was still claiming: a session
        // resumed after a permission prompt was left asking for an answer it no
        // longer wants.
        "SessionStart" => (ClaudeActivity::Idle, None),
        "SessionEnd" => (ClaudeActivity::Ended, None),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(json: serde_json::Value) -> Option<(ClaudeActivity, Option<String>)> {
        interpret(&json)
    }

    #[test]
    fn a_permission_request_is_what_a_tab_should_shout_about() {
        let (activity, detail) = event(serde_json::json!({
            "hook_event_name": "PermissionRequest",
            "tool_name": "Bash",
            "session_id": "abc"
        }))
        .unwrap();

        assert_eq!(activity, ClaudeActivity::Waiting);
        assert_eq!(detail.as_deref(), Some("Bash"));
    }

    #[test]
    fn starting_a_tool_is_working_and_says_which() {
        let (activity, detail) = event(serde_json::json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Edit"
        }))
        .unwrap();

        assert_eq!(activity, ClaudeActivity::Working);
        assert_eq!(detail.as_deref(), Some("Edit"));
    }

    #[test]
    fn finishing_a_turn_is_done() {
        let (activity, _) = event(serde_json::json!({ "hook_event_name": "Stop" })).unwrap();
        assert_eq!(activity, ClaudeActivity::Done);
    }

    #[test]
    fn a_turn_that_ended_in_an_error_still_means_claude_stopped() {
        // What the tab needs to say is "it is not working any more", which is
        // the same either way; the terminal shows what went wrong.
        let (activity, _) = event(serde_json::json!({ "hook_event_name": "StopFailure" })).unwrap();
        assert_eq!(activity, ClaudeActivity::Done);
    }

    #[test]
    fn a_session_that_just_started_claims_nothing_about_itself() {
        let (activity, detail) = event(serde_json::json!({
            "hook_event_name": "SessionStart",
            "source": "startup"
        }))
        .unwrap();

        assert_eq!(activity, ClaudeActivity::Idle);
        assert_eq!(detail, None);
    }

    #[test]
    fn resuming_or_clearing_drops_a_claim_that_has_stopped_being_true() {
        // The reason the event is worth a hook at all: `waiting` never expires,
        // so without this a resumed session asks for an answer forever.
        for source in ["resume", "clear", "compact"] {
            let (activity, _) = event(serde_json::json!({
                "hook_event_name": "SessionStart",
                "source": source
            }))
            .unwrap();
            assert_eq!(activity, ClaudeActivity::Idle, "source {source}");
        }
    }

    #[test]
    fn a_report_carries_the_conversation_it_came_from() {
        // Proof that the conversation exists, which is what decides whether the
        // next start resumes it or tries to create one that is already there.
        let event = serde_json::json!({
            "hook_event_name": "Stop",
            "session_id": "cafb8c86-53eb-49c4-a8b8-609e5cbc0f49"
        });
        assert_eq!(
            event.get("session_id").and_then(|v| v.as_str()),
            Some("cafb8c86-53eb-49c4-a8b8-609e5cbc0f49")
        );
        assert_eq!(interpret(&event).unwrap().0, ClaudeActivity::Done);
    }

    #[test]
    fn a_session_that_only_opened_is_not_proof_of_a_conversation() {
        // `SessionStart` fires before anything has been said, and Claude Code
        // writes nothing until the first exchange. Treating it as proof is what
        // made a restart answer "No conversation found with session ID".
        let (activity, _) = event(serde_json::json!({
            "hook_event_name": "SessionStart",
            "source": "startup",
            "session_id": "cafb8c86-53eb-49c4-a8b8-609e5cbc0f49"
        }))
        .unwrap();
        assert_eq!(activity, ClaudeActivity::Idle);
    }

    #[test]
    fn a_subagent_starting_is_reported_as_an_agent_not_as_a_session_state() {
        let request = agent_report(
            &serde_json::json!({
                "hook_event_name": "SubagentStart",
                "agent_id": "a0718b64719533846",
                "agent_type": "beacon-explorer"
            }),
            "pj_x",
        )
        .unwrap();

        match request {
            Request::ReportAgent {
                agent,
                agent_type,
                running,
                summary,
                ..
            } => {
                assert_eq!(agent, "a0718b64719533846");
                assert_eq!(agent_type.as_deref(), Some("beacon-explorer"));
                assert!(running);
                assert_eq!(summary, None);
            }
            other => panic!("expected an agent report, got {other:?}"),
        }
    }

    #[test]
    fn a_subagent_stopping_carries_one_line_of_what_it_found() {
        let request = agent_report(
            &serde_json::json!({
                "hook_event_name": "SubagentStop",
                "agent_id": "a0718b64719533846",
                "agent_type": "beacon-explorer",
                "last_assistant_message": "Found 4 relevant files.\n\nsrc/a.rs:12 — the parser\nsrc/b.rs:88 — the caller"
            }),
            "pj_x",
        )
        .unwrap();

        match request {
            Request::ReportAgent {
                running, summary, ..
            } => {
                assert!(!running);
                assert_eq!(summary.as_deref(), Some("Found 4 relevant files."));
            }
            other => panic!("expected an agent report, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_agent_type_is_absent_rather_than_empty() {
        // Seen on a real SubagentStop from Claude Code 2.1.252. An empty string
        // on screen would read as a nameless agent rather than an unnamed one.
        let request = agent_report(
            &serde_json::json!({
                "hook_event_name": "SubagentStop",
                "agent_id": "a071",
                "agent_type": ""
            }),
            "pj_x",
        )
        .unwrap();

        assert!(matches!(
            request,
            Request::ReportAgent {
                agent_type: None,
                ..
            }
        ));
    }

    #[test]
    fn a_subagent_event_without_an_id_is_not_reported() {
        // A start and a stop pair up by id. One without an id would leave a row
        // on screen that nothing could ever take away.
        assert!(
            agent_report(
                &serde_json::json!({ "hook_event_name": "SubagentStart" }),
                "pj_x"
            )
            .is_none()
        );
    }

    #[test]
    fn everything_else_is_not_an_agent_report() {
        for name in ["Stop", "PreToolUse", "SessionStart", "Notification"] {
            assert!(
                agent_report(&serde_json::json!({ "hook_event_name": name }), "pj_x").is_none(),
                "{name} was read as an agent"
            );
        }
    }

    #[test]
    fn a_very_long_answer_is_cut_rather_than_wrapped_across_the_header() {
        let long = "x".repeat(400);
        let request = agent_report(
            &serde_json::json!({
                "hook_event_name": "SubagentStop",
                "agent_id": "a071",
                "last_assistant_message": long
            }),
            "pj_x",
        )
        .unwrap();

        let Request::ReportAgent { summary, .. } = request else {
            panic!("expected an agent report")
        };
        let summary = summary.unwrap();
        assert_eq!(summary.chars().count(), 101);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn a_subagent_event_is_not_also_a_session_state() {
        // Both hooks run through the same binary. A subagent finishing must not
        // make the tab say the session is done.
        for name in ["SubagentStart", "SubagentStop"] {
            assert!(
                interpret(&serde_json::json!({ "hook_event_name": name })).is_none(),
                "{name} was read as a session state"
            );
        }
    }

    #[test]
    fn events_that_would_not_change_what_you_do_are_ignored() {
        for name in ["PostToolUse", "PreCompact", "FileChanged", "ConfigChange"] {
            assert!(
                event(serde_json::json!({ "hook_event_name": name })).is_none(),
                "{name} should be ignored"
            );
        }
    }

    #[test]
    fn anything_unrecognisable_is_ignored_rather_than_guessed_at() {
        assert!(event(serde_json::json!({})).is_none());
        assert!(event(serde_json::json!({ "hook_event_name": "SomethingNew" })).is_none());
    }
}
