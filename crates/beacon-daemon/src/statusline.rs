use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use beacon_core::domain::ProjectId;
use beacon_core::protocol::{Envelope, PromptCache, Request, UsageReport};

/// Runs as Claude Code's status line and reports what a session is costing.
///
/// The status line is the only place Claude Code says how much of the five-hour
/// allowance is gone and how full the context is. Nothing writes those to disk,
/// so this is the honest way to know — the alternative would be reading the
/// terminal and guessing, which is worse than not knowing.
///
/// Unlike a hook, the status line is a single slot: configuring one replaces
/// whatever was there. So this delegates. Whatever the user had still runs and
/// still prints; Beacon only listens in.
pub fn run(delegate: Option<String>) -> ! {
    let mut payload = String::new();
    if std::io::stdin().read_to_string(&mut payload).is_err() {
        std::process::exit(0);
    }

    let _ = report(&payload);
    print_line(&payload, delegate);
    std::process::exit(0)
}

fn report(payload: &str) -> Option<()> {
    let socket = std::env::var("BEACON_SOCKET").ok()?;
    let project = std::env::var("BEACON_PROJECT").ok()?;

    let event: serde_json::Value = serde_json::from_str(payload).ok()?;
    let usage = interpret(&event, ProjectId(project));

    let line = serde_json::to_string(&Envelope {
        id: 0,
        request: Request::ReportUsage {
            usage: Box::new(usage),
        },
    })
    .ok()?;

    let mut stream = UnixStream::connect(socket).ok()?;
    stream.write_all(line.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;
    stream.flush().ok()?;
    Some(())
}

/// Pulls what Beacon shows out of the status line payload.
///
/// Every field is optional in the payload and stays optional here. Claude Code
/// fills in what it knows: rate limits only on a plan that has them and only
/// after the first response, the cache only once there has been a response to
/// observe, effort only on a model that has one. A number Beacon invented to
/// fill a gap would be indistinguishable from one Claude Code reported, and it
/// is the second kind people plan around.
pub fn interpret(event: &serde_json::Value, project: ProjectId) -> UsageReport {
    let number = |path: [&str; 2]| -> Option<f32> {
        event.get(path[0])?.get(path[1])?.as_f64().map(|v| v as f32)
    };
    let integer = |path: [&str; 2]| -> Option<u64> { event.get(path[0])?.get(path[1])?.as_u64() };
    let text = |path: [&str; 2]| -> Option<String> {
        event
            .get(path[0])?
            .get(path[1])?
            .as_str()
            .map(str::to_string)
    };
    let top_text = |key: &str| -> Option<String> { event.get(key)?.as_str().map(str::to_string) };
    let window = |name: &str, field: &str| -> Option<&serde_json::Value> {
        event.get("rate_limits")?.get(name)?.get(field)
    };
    let window_used =
        |name: &str| -> Option<f32> { window(name, "used_percentage")?.as_f64().map(|v| v as f32) };
    let window_reset = |name: &str| -> Option<i64> { window(name, "resets_at")?.as_i64() };

    UsageReport {
        project,
        session_id: top_text("session_id"),
        session_name: top_text("session_name"),
        model: event
            .get("model")
            .and_then(|model| model.get("display_name").or_else(|| model.get("id")))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        model_id: text(["model", "id"]),
        effort: text(["effort", "level"]),
        thinking: event
            .get("thinking")
            .and_then(|thinking| thinking.get("enabled"))
            .and_then(serde_json::Value::as_bool),
        context_used_percentage: number(["context_window", "used_percentage"]),
        context_remaining_percentage: number(["context_window", "remaining_percentage"]),
        // `total_input_tokens`, deliberately: it is the sum of the input,
        // cache-creation and cache-read parts, which is exactly "what is in
        // the window now". Its sibling `current_usage` is an object holding
        // those parts separately — never a number — and is null before the
        // first API call and again after a compact.
        context_used_tokens: integer(["context_window", "total_input_tokens"]),
        context_size: integer(["context_window", "context_window_size"]),
        prompt_cache: read_cache(event),
        five_hour_used_percentage: window_used("five_hour"),
        five_hour_resets_at: window_reset("five_hour"),
        seven_day_used_percentage: window_used("seven_day"),
        seven_day_resets_at: window_reset("seven_day"),
        spend_limit_used_percentage: window_used("spend_limit"),
        spend_limit_resets_at: window_reset("spend_limit"),
        worktree: text(["worktree", "name"]),
    }
}

/// The cache block, or nothing at all.
///
/// All or nothing on purpose: the block only exists once there has been an API
/// response, and half a cache report would be read as a cold one.
fn read_cache(event: &serde_json::Value) -> Option<PromptCache> {
    let cache = event.get("prompt_cache")?.as_object()?;
    let integer = |key: &str| -> Option<u64> { cache.get(key)?.as_u64() };

    Some(PromptCache {
        warm: cache.get("warm").and_then(serde_json::Value::as_bool),
        hit_ratio: cache
            .get("hit_ratio")
            .and_then(serde_json::Value::as_f64)
            .map(|v| v as f32),
        expires_at: cache.get("expires_at").and_then(serde_json::Value::as_i64),
        recache_tokens_if_cold: integer("recache_tokens_if_cold"),
        misses: integer("misses"),
        expected_rebuilds: integer("expected_rebuilds"),
    })
}

/// Prints whatever the user's own status line would have printed.
///
/// With nothing to delegate to, prints a plain line rather than nothing:
/// installing this should not make Claude Code look emptier than before.
fn print_line(payload: &str, delegate: Option<String>) {
    if let Some(command) = delegate
        && let Some(output) = run_delegate(&command, payload)
    {
        print!("{output}");
        return;
    }

    let event: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    let usage = interpret(&event, ProjectId(String::new()));

    let mut parts = Vec::new();
    if let Some(model) = usage.model {
        parts.push(model);
    }
    if let Some(context) = usage.context_used_percentage {
        parts.push(format!("{context:.0}% context"));
    }
    if let Some(five_hour) = usage.five_hour_used_percentage {
        parts.push(format!("{:.0}% session left", 100.0 - five_hour));
    }
    println!("{}", parts.join(" · "));
}

fn run_delegate(command: &str, payload: &str) -> Option<String> {
    use std::process::{Command, Stdio};

    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    child.stdin.as_mut()?.write_all(payload.as_bytes()).ok()?;
    drop(child.stdin.take());

    let output = child.wait_with_output().ok()?;
    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The payload Claude Code documents, trimmed to what Beacon reads.
    ///
    /// Kept faithful to the published shape rather than to what is convenient:
    /// a fixture that differs from the real thing tests nothing but itself.
    fn payload() -> serde_json::Value {
        serde_json::json!({
            "cwd": "/Users/x/projects/app",
            "session_id": "b57bf9d0-8020-4275-a060-a521d289beae",
            "session_name": "auth-refactor",
            "version": "2.1.252",
            "model": { "id": "claude-opus-5", "display_name": "Opus 5" },
            "workspace": { "current_dir": "/Users/x/projects/app" },
            "effort": { "level": "high" },
            "thinking": { "enabled": true },
            "context_window": {
                "used_percentage": 37.4,
                "remaining_percentage": 62.6,
                "total_input_tokens": 74_800,
                "total_output_tokens": 1_200,
                "context_window_size": 200_000,
                "current_usage": {
                    "input_tokens": 8_500,
                    "output_tokens": 1_200,
                    "cache_creation_input_tokens": 5_000,
                    "cache_read_input_tokens": 61_300
                }
            },
            "prompt_cache": {
                "warm": true,
                "ttl": "1h",
                "expires_at": 1_800_003_600_i64,
                "requests": 14,
                "misses": 2,
                "expected_rebuilds": 1,
                "hit_ratio": 0.91,
                "recache_tokens_if_cold": 45_000
            },
            "rate_limits": {
                "five_hour": { "used_percentage": 62.5, "resets_at": 1_800_000_000_i64 },
                "seven_day": { "used_percentage": 18.0, "resets_at": 1_800_400_000_i64 }
            }
        })
    }

    #[test]
    fn reads_the_session_allowance_and_when_it_comes_back() {
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(usage.five_hour_used_percentage, Some(62.5));
        assert_eq!(usage.five_hour_resets_at, Some(1_800_000_000));
        assert_eq!(usage.seven_day_used_percentage, Some(18.0));
    }

    #[test]
    fn reads_how_full_the_context_is() {
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(usage.context_used_percentage, Some(37.4));
        assert_eq!(usage.context_used_tokens, Some(74_800));
        assert_eq!(usage.context_size, Some(200_000));
    }

    #[test]
    fn the_token_count_survives_current_usage_being_an_object() {
        // `current_usage` is an object of parts, and is null before the first
        // API call and again after a compact. Reading it as a number used to
        // work only because it failed and fell through to the right field.
        let mut event = payload();
        event["context_window"]["current_usage"] = serde_json::Value::Null;

        let usage = interpret(&event, ProjectId("pj_x".into()));
        assert_eq!(usage.context_used_tokens, Some(74_800));
    }

    #[test]
    fn a_window_that_has_not_been_used_yet_reports_no_tokens() {
        let usage = interpret(
            &serde_json::json!({ "context_window": { "context_window_size": 200_000 } }),
            ProjectId("pj_x".into()),
        );
        assert_eq!(usage.context_used_tokens, None);
        assert_eq!(usage.context_size, Some(200_000));
    }

    #[test]
    fn carries_the_conversation_claude_code_is_in() {
        // The field that makes a workstream addressable without ever opening a
        // transcript.
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(
            usage.session_id.as_deref(),
            Some("b57bf9d0-8020-4275-a060-a521d289beae")
        );
        assert_eq!(usage.session_name.as_deref(), Some("auth-refactor"));
    }

    #[test]
    fn an_unnamed_session_is_not_given_a_name() {
        // Claude Code leaves `session_name` out for an automatic display name
        // like `beacon-split-b7`. Absent means the user has not named it, and
        // Beacon must not invent one to fill the gap.
        let mut event = payload();
        event.as_object_mut().unwrap().remove("session_name");

        let usage = interpret(&event, ProjectId("pj_x".into()));
        assert_eq!(usage.session_name, None);
        assert!(usage.session_id.is_some());
    }

    #[test]
    fn reads_the_effort_and_whether_it_is_thinking() {
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(usage.effort.as_deref(), Some("high"));
        assert_eq!(usage.thinking, Some(true));
    }

    #[test]
    fn reads_what_the_cache_would_cost_to_rebuild() {
        // The number a recommendation is made from: a large context whose cache
        // has gone cold is paid for again on the next turn.
        let cache = interpret(&payload(), ProjectId("pj_x".into()))
            .prompt_cache
            .unwrap();

        assert_eq!(cache.warm, Some(true));
        assert_eq!(cache.hit_ratio, Some(0.91));
        assert_eq!(cache.recache_tokens_if_cold, Some(45_000));
        assert_eq!(cache.misses, Some(2));
        assert_eq!(cache.expected_rebuilds, Some(1));
        assert_eq!(cache.expires_at, Some(1_800_003_600));
    }

    #[test]
    fn a_session_before_its_first_response_reports_no_cache_at_all() {
        // Not a cold cache — an unknown one. Claude Code leaves the block out
        // until there has been a response to observe.
        let mut event = payload();
        event.as_object_mut().unwrap().remove("prompt_cache");

        assert!(
            interpret(&event, ProjectId("pj_x".into()))
                .prompt_cache
                .is_none()
        );
    }

    #[test]
    fn a_window_claude_code_does_not_report_is_left_out() {
        // Each rate-limit window is independently absent, and Claude Code drops
        // one once its reset time passes.
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(usage.spend_limit_used_percentage, None);
        assert_eq!(usage.spend_limit_resets_at, None);
    }

    #[test]
    fn reads_the_room_left_as_claude_code_states_it() {
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(usage.context_remaining_percentage, Some(62.6));
    }

    #[test]
    fn prefers_the_models_display_name() {
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(usage.model.as_deref(), Some("Opus 5"));
        assert_eq!(usage.model_id.as_deref(), Some("claude-opus-5"));
    }

    #[test]
    fn what_claude_code_does_not_say_stays_unknown() {
        // A plan without rate limits reports none. Showing zero there would
        // read as "you have used nothing", which is the opposite of unknown.
        let usage = interpret(
            &serde_json::json!({ "model": { "display_name": "Opus 5" } }),
            ProjectId("pj_x".into()),
        );
        assert_eq!(usage.five_hour_used_percentage, None);
        assert_eq!(usage.context_used_percentage, None);
        assert_eq!(usage.context_size, None);
    }

    #[test]
    fn an_unrecognisable_payload_does_not_panic() {
        let usage = interpret(&serde_json::json!("nonsense"), ProjectId("pj_x".into()));
        assert_eq!(usage.model, None);
    }

    #[test]
    fn a_delegate_that_fails_does_not_swallow_the_line() {
        assert!(run_delegate("exit 1", "{}").is_some_and(|out| out.is_empty()));
        assert!(run_delegate("printf hello", "{}").as_deref() == Some("hello"));
    }
}
