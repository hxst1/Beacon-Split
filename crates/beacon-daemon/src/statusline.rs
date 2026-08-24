use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use beacon_core::domain::ProjectId;
use beacon_core::protocol::{Envelope, Request, UsageReport};

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
        request: Request::ReportUsage { usage },
    })
    .ok()?;

    let mut stream = UnixStream::connect(socket).ok()?;
    stream.write_all(line.as_bytes()).ok()?;
    stream.write_all(b"\n").ok()?;
    stream.flush().ok()?;
    Some(())
}

/// Pulls what Beacon shows out of the status line payload.
pub fn interpret(event: &serde_json::Value, project: ProjectId) -> UsageReport {
    let number = |path: [&str; 2]| -> Option<f32> {
        event.get(path[0])?.get(path[1])?.as_f64().map(|v| v as f32)
    };
    let integer = |path: [&str; 2]| -> Option<u64> { event.get(path[0])?.get(path[1])?.as_u64() };
    let seconds = |window: &str| -> Option<i64> {
        event
            .get("rate_limits")?
            .get(window)?
            .get("resets_at")?
            .as_i64()
    };
    let window_used = |window: &str| -> Option<f32> {
        event
            .get("rate_limits")?
            .get(window)?
            .get("used_percentage")?
            .as_f64()
            .map(|v| v as f32)
    };

    UsageReport {
        project,
        model: event
            .get("model")
            .and_then(|model| model.get("display_name").or_else(|| model.get("id")))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        context_used_percentage: number(["context_window", "used_percentage"]),
        context_used_tokens: integer(["context_window", "current_usage"])
            .or_else(|| integer(["context_window", "total_input_tokens"])),
        context_size: integer(["context_window", "context_window_size"]),
        five_hour_used_percentage: window_used("five_hour"),
        five_hour_resets_at: seconds("five_hour"),
        seven_day_used_percentage: window_used("seven_day"),
        seven_day_resets_at: seconds("seven_day"),
    }
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

    fn payload() -> serde_json::Value {
        // The shape Claude Code documents for a status line.
        serde_json::json!({
            "model": { "id": "claude-opus-5", "display_name": "Opus 5" },
            "workspace": { "current_dir": "/Users/x/projects/app" },
            "context_window": {
                "used_percentage": 37.4,
                "current_usage": 74_800,
                "context_window_size": 200_000
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
    fn prefers_the_models_display_name() {
        let usage = interpret(&payload(), ProjectId("pj_x".into()));
        assert_eq!(usage.model.as_deref(), Some("Opus 5"));
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
