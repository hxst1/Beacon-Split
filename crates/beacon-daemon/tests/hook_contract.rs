//! What Beacon's hook is allowed to print.
//!
//! Claude Code parses a hook's stdout: anything starting with `{` and ending
//! with `}` is read as JSON, and a hook that gets this wrong shows the user an
//! error at the top of their session. That is not hypothetical — this suite
//! exists because a third-party plugin emitted two JSON objects from one hook
//! and every session started with a parse error.
//!
//! Beacon's answer is to print nothing at all and always exit zero. These tests
//! run the real binary, because the contract is about the process, and a unit
//! test of the function that builds the message would not have caught the case
//! that broke: the hook that prints something *else* as well.

use std::io::Write;
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

/// Every event Beacon registers, with a payload shaped like Claude Code's.
fn payload(event: &str) -> String {
    serde_json::json!({
        "session_id": "cafb8c86-53eb-49c4-a8b8-609e5cbc0f49",
        "transcript_path": "/tmp/transcript.jsonl",
        "cwd": "/tmp/project",
        "permission_mode": "auto",
        "hook_event_name": event,
        "tool_name": "Bash",
        "source": "startup",
        "agent_id": "a0718b64719533846",
        "agent_type": "beacon-explorer",
        "last_assistant_message": "Found 4 relevant files"
    })
    .to_string()
}

struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

fn run_hook(stdin: &str, env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_beacon-daemon"));
    command
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Cleared first: the tests may themselves be running inside a Beacon.
    command.env_remove("BEACON_SOCKET");
    command.env_remove("BEACON_PROJECT");
    for (key, value) in env {
        command.env(key, value);
    }

    let mut child = command.spawn().expect("could not run the hook");
    child
        .stdin
        .as_mut()
        .expect("no stdin")
        .write_all(stdin.as_bytes())
        .expect("could not write the payload");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("the hook did not finish");
    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        code: output.status.code(),
    }
}

/// A socket that accepts one connection and hands back what was written to it.
fn listening(dir: &std::path::Path) -> (String, std::thread::JoinHandle<String>) {
    let path = dir.join("hook.sock");
    let listener = UnixListener::bind(&path).expect("could not listen");

    let handle = std::thread::spawn(move || {
        use std::io::Read;
        let Ok((mut stream, _)) = listener.accept() else {
            return String::new();
        };
        let mut line = String::new();
        let _ = stream.read_to_string(&mut line);
        line
    });

    (path.to_string_lossy().into_owned(), handle)
}

#[test]
fn every_registered_event_prints_nothing_and_succeeds() {
    // Nothing on stdout is the strongest possible answer to the contract: there
    // is no way to get JSON wrong if you never emit any.
    for event in beacon_core::claude_hooks::EVENTS {
        let run = run_hook(&payload(event), &[]);
        assert_eq!(run.stdout, "", "{event} printed to stdout");
        assert_eq!(run.code, Some(0), "{event} exited {:?}", run.code);
    }
}

#[test]
fn it_prints_nothing_even_when_it_has_somewhere_to_report_to() {
    // The path that actually runs inside Beacon. Reporting is a socket write,
    // and a socket write must never become a line on stdout.
    let dir = tempfile::tempdir().expect("no temporary directory");
    let (socket, received) = listening(dir.path());

    let run = run_hook(
        &payload("Stop"),
        &[("BEACON_SOCKET", &socket), ("BEACON_PROJECT", "pj_x")],
    );

    assert_eq!(run.stdout, "");
    assert_eq!(run.code, Some(0));

    let line = received.join().expect("the listener panicked");
    let sent: serde_json::Value =
        serde_json::from_str(line.trim()).expect("the daemon was sent something that is not JSON");
    assert_eq!(sent["method"], "report");
}

#[test]
fn a_daemon_that_is_not_there_is_not_an_error_the_user_sees() {
    // Beacon closed, or never started. The hook is registered once and has to
    // be harmless for as long as it stays registered.
    let run = run_hook(
        &payload("PreToolUse"),
        &[
            (
                "BEACON_SOCKET",
                "/tmp/beacon-does-not-exist-1234/daemon.sock",
            ),
            ("BEACON_PROJECT", "pj_x"),
        ],
    );

    assert_eq!(run.stdout, "");
    assert_eq!(run.code, Some(0));
}

#[test]
fn nothing_on_stdin_is_survived_quietly() {
    let run = run_hook("", &[]);
    assert_eq!(run.stdout, "");
    assert_eq!(run.code, Some(0));
}

#[test]
fn a_payload_beacon_cannot_read_is_survived_quietly() {
    // A future Claude Code, a truncated pipe, something else entirely. None of
    // it is worth putting an error in front of somebody's work.
    for stdin in [
        "not json at all",
        "{",
        "{}",
        "null",
        "[]",
        r#"{"hook_event_name": "SomethingAddedNextYear"}"#,
        r#"{"hook_event_name": 7}"#,
    ] {
        let run = run_hook(stdin, &[("BEACON_PROJECT", "pj_x")]);
        assert_eq!(run.stdout, "", "printed something for {stdin:?}");
        assert_eq!(run.code, Some(0), "exited nonzero for {stdin:?}");
    }
}

#[test]
fn it_says_nothing_on_stderr_either() {
    // stderr is not parsed, but a non-zero exit shows its first line to the
    // user — and a hook that chatters there fills the debug log of every
    // session on the machine.
    for event in beacon_core::claude_hooks::EVENTS {
        let run = run_hook(&payload(event), &[]);
        assert_eq!(run.stderr, "", "{event} wrote to stderr");
    }
}

#[test]
fn every_event_beacon_registers_is_an_event_beacon_reads() {
    // An event registered and then ignored is a process Claude Code starts for
    // nothing, on every turn, forever.
    for event in beacon_core::claude_hooks::EVENTS {
        let dir = tempfile::tempdir().expect("no temporary directory");
        let (socket, received) = listening(dir.path());

        let run = run_hook(
            &payload(event),
            &[("BEACON_SOCKET", &socket), ("BEACON_PROJECT", "pj_x")],
        );
        assert_eq!(run.code, Some(0));

        let line = received.join().expect("the listener panicked");
        assert!(
            !line.trim().is_empty(),
            "{event} is registered but reports nothing"
        );
    }
}

#[test]
fn the_status_line_survives_a_payload_it_cannot_read() {
    // A different contract: the status line is *supposed* to print, and what it
    // prints is shown as-is. What it must not do is fail, which would leave
    // Claude Code showing nothing where the user's own line used to be.
    for stdin in ["", "not json", "{}"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_beacon-daemon"));
        command
            .arg("statusline")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.env_remove("BEACON_SOCKET");

        let mut child = command.spawn().expect("could not run the status line");
        child
            .stdin
            .as_mut()
            .expect("no stdin")
            .write_all(stdin.as_bytes())
            .expect("could not write");
        drop(child.stdin.take());

        let output = child.wait_with_output().expect("it did not finish");
        assert_eq!(
            output.status.code(),
            Some(0),
            "exited nonzero for {stdin:?}"
        );
    }
}

#[test]
fn the_status_line_runs_the_one_it_displaced_and_prints_only_that() {
    // Beacon takes a slot that was somebody else's. What Claude Code shows has
    // to be exactly what it would have shown, with nothing added.
    let mut command = Command::new(env!("CARGO_BIN_EXE_beacon-daemon"));
    command
        .arg("statusline")
        .arg("printf 'my own line'")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env_remove("BEACON_SOCKET");

    let mut child = command.spawn().expect("could not run the status line");
    child
        .stdin
        .as_mut()
        .expect("no stdin")
        .write_all(br#"{"model":{"display_name":"Opus 5"}}"#)
        .expect("could not write");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("it did not finish");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "my own line");
    assert_eq!(output.status.code(), Some(0));
}
