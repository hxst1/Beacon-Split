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

    let (activity, detail) = interpret(&event)?;

    let line = serde_json::to_string(&Envelope {
        // Nothing is waiting for a reply, so the correlation id is a formality.
        id: 0,
        request: Request::Report {
            project: ProjectId(project),
            activity,
            detail,
        },
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
