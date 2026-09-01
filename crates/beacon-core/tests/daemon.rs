//! Exercises the daemon end to end: a client attaches, starts a session,
//! detaches, and finds its work still running when it comes back.
//!
//! This is the point of the whole arrangement, so it is worth testing against a
//! real daemon process rather than a mock.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beacon_core::client::{DaemonClient, DaemonEvents};
use beacon_core::domain::ProjectId;
use beacon_core::protocol::Event;
use beacon_core::session::{SessionKind, SessionPrefs};

#[derive(Default)]
struct Recorder {
    output: Mutex<Vec<u8>>,
    disconnects: Mutex<usize>,
    reattachments: Mutex<usize>,
    activity: Mutex<Vec<(beacon_core::protocol::ClaudeActivity, Option<String>)>>,
    clips: Mutex<Vec<beacon_core::clips::Clip>>,
}

impl DaemonEvents for Recorder {
    fn event(&self, event: Event) {
        if let Event::Activity {
            activity, detail, ..
        } = &event
        {
            self.activity
                .lock()
                .unwrap()
                .push((*activity, detail.clone()));
        }
        if let Event::Clip(clip) = &event {
            self.clips.lock().unwrap().push(clip.clone());
        }
        if let Event::Output { data, .. } = event {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap_or_default();
            self.output.lock().unwrap().extend_from_slice(&bytes);
        }
    }

    fn disconnected(&self) {
        *self.disconnects.lock().unwrap() += 1;
    }

    fn reattached(&self) {
        *self.reattachments.lock().unwrap() += 1;
    }
}

impl Recorder {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }
}

fn wait_for(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// A socket of this test's own.
///
/// Never the default one: running the tests must not be able to reach — or shut
/// down — a daemon somebody is actually working in.
fn private_socket(dir: &std::path::Path) -> PathBuf {
    dir.join("daemon.sock")
}

/// A daemon of this test's own, with a configuration directory of its own.
///
/// Started here rather than left to the client, so its environment can be set.
/// A daemon that inherits the real one writes to the clip book somebody is
/// actually using — and a test that empties a drawer empties theirs.
fn daemon_with_private_config(
    binary: &std::path::Path,
    socket_dir: &std::path::Path,
    config_dir: &std::path::Path,
) -> std::process::Child {
    let child = std::process::Command::new(binary)
        .arg(socket_dir)
        .env("BEACON_CONFIG_DIR", config_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("the daemon should start");

    // The client connects to whatever is already listening, so it has to be
    // listening before the client is built or it would start a second one —
    // with the real configuration.
    let socket = socket_dir.join("daemon.sock");
    assert!(
        wait_for(Duration::from_secs(10), || {
            std::os::unix::net::UnixStream::connect(&socket).is_ok()
        }),
        "the private daemon never started listening"
    );
    child
}

/// The daemon built alongside this test.
fn daemon_binary() -> PathBuf {
    // Integration tests run from `target/<profile>/deps`, and the binary sits
    // one level up beside the other build outputs.
    let mut dir = std::env::current_exe().expect("test binary path");
    dir.pop();
    if dir.ends_with("deps") {
        dir.pop();
    }
    dir.join("beacon-daemon")
}

#[test]
fn a_session_outlives_the_client_that_started_it() {
    let binary = daemon_binary();
    if !binary.exists() {
        eprintln!("skipping: build beacon-daemon first ({})", binary.display());
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    // First client: start a shell and leave a mark in it.
    let first = Arc::new(Recorder::default());
    let session = {
        let client = DaemonClient::connect_at(
            &binary,
            &private_socket(dir.path()),
            Arc::clone(&first) as Arc<dyn DaemonEvents>,
        )
        .expect("should reach a daemon");

        let session = client
            .ensure(
                &project,
                SessionKind::Shell,
                0,
                dir.path(),
                (80, 24),
                SessionPrefs::default(),
            )
            .expect("should start a session");

        client.write(&session.id, "echo bea''con-lives\n").unwrap();

        assert!(
            wait_for(Duration::from_secs(15), || first
                .text()
                .contains("beacon-lives")),
            "the shell never answered; saw: {:?}",
            first.text()
        );

        session
        // The client goes out of scope here — the window closing.
    };

    // Give the daemon a moment to notice the detach.
    std::thread::sleep(Duration::from_millis(200));

    // Second client: the session should still be there, with its scrollback.
    let second = Arc::new(Recorder::default());
    let client = DaemonClient::connect_at(
        &binary,
        &private_socket(dir.path()),
        Arc::clone(&second) as Arc<dyn DaemonEvents>,
    )
    .expect("should reach the same daemon");

    let again = client
        .ensure(
            &project,
            SessionKind::Shell,
            0,
            dir.path(),
            (80, 24),
            SessionPrefs::default(),
        )
        .expect("should find the running session");
    assert_eq!(
        again.id, session.id,
        "reattaching must find the same session, not start another"
    );
    assert!(again.running);

    let (encoded, end_offset) = client.scrollback(&session.id).unwrap();
    use base64::Engine as _;
    let replayed = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    assert!(
        String::from_utf8_lossy(&replayed).contains("beacon-lives"),
        "the work done before detaching should still be there"
    );
    assert!(end_offset > 0);

    // And it is still a live shell, not a recording.
    client.write(&session.id, "echo still''-here\n").unwrap();
    assert!(
        wait_for(Duration::from_secs(15), || second
            .text()
            .contains("still-here")),
        "the reattached session should still respond; saw: {:?}",
        second.text()
    );

    client.close(&session.id).unwrap();
    assert!(
        client
            .list()
            .unwrap()
            .iter()
            .all(|info| info.id != session.id),
        "a closed session should be gone"
    );
}

#[test]
fn closing_a_project_stops_its_sessions_but_not_the_daemon() {
    let binary = daemon_binary();
    if !binary.exists() {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let recorder = Arc::new(Recorder::default());
    let client = DaemonClient::connect_at(
        &binary,
        &private_socket(dir.path()),
        recorder as Arc<dyn DaemonEvents>,
    )
    .unwrap();

    let one = ProjectId::generate();
    let other = ProjectId::generate();
    let kept = client
        .ensure(
            &other,
            SessionKind::Shell,
            0,
            dir.path(),
            (80, 24),
            SessionPrefs::default(),
        )
        .unwrap();
    client
        .ensure(
            &one,
            SessionKind::Shell,
            0,
            dir.path(),
            (80, 24),
            SessionPrefs::default(),
        )
        .unwrap();

    client.close_project(&one).unwrap();

    let alive = client.list().unwrap();
    assert!(
        alive.iter().any(|info| info.id == kept.id),
        "the other project keeps running"
    );
    assert!(
        alive.iter().all(|info| info.project != one),
        "the closed project has nothing left"
    );

    client.close(&kept.id).unwrap();
}

#[test]
fn the_client_gets_itself_back_after_the_daemon_is_replaced() {
    // The daemon is meant to outlive the window. A window that goes permanently
    // deaf when the daemon is restarted — for an upgrade, or because someone
    // stopped it — gets none of that benefit.
    let binary = daemon_binary();
    if !binary.exists() {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let recorder = Arc::new(Recorder::default());
    let client = DaemonClient::connect_at(
        &binary,
        &private_socket(dir.path()),
        Arc::clone(&recorder) as Arc<dyn DaemonEvents>,
    )
    .unwrap();

    let project = ProjectId::generate();
    client
        .ensure(
            &project,
            SessionKind::Shell,
            0,
            dir.path(),
            (80, 24),
            SessionPrefs::default(),
        )
        .expect("a session to lose");

    // Stopping the daemon is the harshest version of this: everything it held
    // is gone, and the client has to notice and rebuild rather than hang.
    client.shutdown().ok();

    assert!(
        wait_for(Duration::from_secs(10), || *recorder
            .disconnects
            .lock()
            .unwrap()
            > 0),
        "the client should notice it was cut off"
    );

    assert!(
        wait_for(Duration::from_secs(30), || *recorder
            .reattachments
            .lock()
            .unwrap()
            > 0),
        "the client should get itself back without being asked"
    );

    // And it is usable again, not merely connected.
    let fresh = client
        .ensure(
            &project,
            SessionKind::Shell,
            0,
            dir.path(),
            (80, 24),
            SessionPrefs::default(),
        )
        .expect("the reattached client should still work");
    assert!(fresh.running);

    client.close(&fresh.id).unwrap();
}

#[test]
fn a_clip_travels_from_the_mcp_server_to_the_window_and_to_disk() {
    // The whole path, as it runs in the app: Claude calls the tool, the MCP
    // server finds the socket in the environment it inherited, the daemon files
    // the clip, and every attached window is told — with nobody having
    // configured anything.
    use beacon_core::clips::ClipKind;

    let binary = daemon_binary();
    if !binary.exists() {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let socket = private_socket(dir.path());

    // Its own configuration directory, so the clip book this test fills and
    // empties is never the one the user is working in.
    let mut daemon = daemon_with_private_config(&binary, dir.path(), config.path());

    let recorder = Arc::new(Recorder::default());
    let client = DaemonClient::connect_at(
        &binary,
        &socket,
        Arc::clone(&recorder) as Arc<dyn DaemonEvents>,
    )
    .unwrap();

    let project = ProjectId::generate();

    // Exactly the conversation Claude Code has with an MCP server, in order.
    let conversation = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"2"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#.to_string(),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "save_clip",
                "arguments": {
                    "title": "Staging keys",
                    // Two lines, to prove the body survives the whole path
                    // exactly as it will be pasted.
                    "body": "API_KEY=abc\nAPI_URL=https://staging",
                    "kind": "variable",
                },
            },
        })
        .to_string(),
    ]
    .join("\n");

    let mut server = std::process::Command::new(&binary)
        .arg("mcp")
        .env("BEACON_SOCKET", &socket)
        .env("BEACON_PROJECT", project.as_str())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("the mcp server should run");

    use std::io::Write as _;
    server
        .stdin
        .as_mut()
        .unwrap()
        .write_all(format!("{conversation}\n").as_bytes())
        .unwrap();
    drop(server.stdin.take());

    let output = server.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "the mcp server should exit cleanly"
    );

    let replies = String::from_utf8_lossy(&output.stdout);
    assert!(
        replies.contains("save_clip"),
        "the tool should be advertised inside Beacon; saw: {replies}"
    );
    assert!(
        replies.contains("\"isError\":false"),
        "the call should have been accepted; saw: {replies}"
    );

    // The window heard about it without asking.
    assert!(
        wait_for(Duration::from_secs(10), || !recorder
            .clips
            .lock()
            .unwrap()
            .is_empty()),
        "the window should have been told about the clip"
    );

    let announced = recorder.clips.lock().unwrap().clone();
    assert_eq!(announced[0].title, "Staging keys");
    assert_eq!(announced[0].kind, ClipKind::Variable);
    assert_eq!(announced[0].body, "API_KEY=abc\nAPI_URL=https://staging");

    // And asking for the drawer finds the same thing, so a window that opened
    // after the clip arrived is not blank.
    let held = client.clips().unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].id, announced[0].id);

    // Forgetting it empties the drawer for everyone.
    assert!(
        client
            .forget_clips(Some(held[0].id.clone()))
            .unwrap()
            .is_empty()
    );
    assert!(client.clips().unwrap().is_empty());

    // And it was written where it was told to be, not where the user's is.
    let book = config.path().join("clips.json");
    assert!(
        book.exists(),
        "the daemon should have used its own config dir"
    );

    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn an_mcp_server_outside_beacon_offers_nothing() {
    // Registered once and harmless everywhere else, exactly like the hook. An
    // advertised tool costs context in every turn of every session.
    let binary = daemon_binary();
    if !binary.exists() {
        return;
    }

    let output = {
        use std::io::Write as _;
        let mut server = std::process::Command::new(&binary)
            .arg("mcp")
            .env_remove("BEACON_SOCKET")
            .env_remove("BEACON_PROJECT")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the mcp server should run anywhere");

        server
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n")
            .unwrap();
        drop(server.stdin.take());
        server.wait_with_output().unwrap()
    };

    assert!(output.status.success());
    let replies = String::from_utf8_lossy(&output.stdout);
    assert!(
        replies.contains(r#""tools":[]"#),
        "outside Beacon there is nothing to offer; saw: {replies}"
    );
}

#[test]
fn a_claude_hook_reaches_the_window() {
    // The whole path: Claude Code runs the hook, the hook finds the socket in
    // its environment, and the window learns that a project needs attention —
    // without anything reading terminal output and guessing.
    use beacon_core::protocol::ClaudeActivity;

    let binary = daemon_binary();
    if !binary.exists() {
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let socket = private_socket(dir.path());
    let recorder = Arc::new(Recorder::default());
    let client = DaemonClient::connect_at(
        &binary,
        &socket,
        Arc::clone(&recorder) as Arc<dyn DaemonEvents>,
    )
    .unwrap();

    let project = ProjectId::generate();

    // Exactly what Claude Code sends a hook on stdin when a tool wants
    // permission.
    let payload = serde_json::json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "session_id": "whatever",
        "cwd": dir.path(),
    })
    .to_string();

    let mut hook = std::process::Command::new(&binary)
        .arg("hook")
        .env("BEACON_SOCKET", &socket)
        .env("BEACON_PROJECT", project.as_str())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("the hook should run");

    use std::io::Write as _;
    hook.stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    drop(hook.stdin.take());

    let status = hook.wait().unwrap();
    assert!(
        status.success(),
        "a hook must never fail: Claude would notice"
    );

    assert!(
        wait_for(Duration::from_secs(10), || !recorder
            .activity
            .lock()
            .unwrap()
            .is_empty()),
        "the window should have been told"
    );

    let reported = recorder.activity.lock().unwrap().clone();
    assert_eq!(reported[0].0, ClaudeActivity::Waiting);
    assert_eq!(reported[0].1.as_deref(), Some("Bash"));

    drop(client);
}

#[test]
fn a_hook_outside_beacon_does_nothing_at_all() {
    // Registered once in the user's Claude settings, it runs for every session
    // everywhere. Anywhere but here it must be a no-op that costs nothing.
    let binary = daemon_binary();
    if !binary.exists() {
        return;
    }

    let mut hook = std::process::Command::new(&binary)
        .arg("hook")
        .env_remove("BEACON_SOCKET")
        .env_remove("BEACON_PROJECT")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();

    use std::io::Write as _;
    hook.stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"hook_event_name":"Stop"}"#)
        .unwrap();
    drop(hook.stdin.take());

    assert!(hook.wait().unwrap().success());
}

/// A daemon left over from the previous version is swapped out quietly.
///
/// This is what every upgrade that moves the protocol runs into: the window
/// opens, finds the old version's daemon still listening, and has to replace
/// it. The swap is expected, so nothing about it may reach the window as a
/// connection lost — a window told that on its very first paint shows every
/// pane the daemon went away, and there is no daemon going away here.
#[test]
fn a_daemon_from_another_version_is_replaced_without_a_word() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    let dir = tempfile::tempdir().unwrap();
    let socket = private_socket(dir.path());
    let listener = UnixListener::bind(&socket).expect("the stand-in should bind");

    // Two connections on one socket: the first speaks last version's protocol
    // and hangs up when asked to stop, the second is the daemon that replaced
    // it. From the client's side that is exactly an upgrade.
    let versions = [
        beacon_core::protocol::PROTOCOL_VERSION - 1,
        beacon_core::protocol::PROTOCOL_VERSION,
    ];
    let stand_in = std::thread::spawn(move || {
        for version in versions {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let mut writer = stream.try_clone().expect("a writable half");
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                let message: serde_json::Value = match serde_json::from_str(&line) {
                    Ok(message) => message,
                    Err(_) => continue,
                };
                let id = message["id"].as_u64().unwrap_or_default();

                // Asked to stop, it stops — by dropping the connection, which
                // is all a client ever sees of a daemon exiting.
                if message["method"] == "shutdown" {
                    break;
                }

                let reply = serde_json::json!({
                    "id": id,
                    "ok": { "result": "greeting", "version": version, "pid": 1, "sessions": 0 },
                });
                let _ = writeln!(writer, "{reply}");
                let _ = writer.flush();
            }
        }
    });

    let recorder = Arc::new(Recorder::default());
    let client = DaemonClient::connect_at(
        &daemon_binary(),
        &socket,
        Arc::clone(&recorder) as Arc<dyn DaemonEvents>,
    )
    .expect("should end up on the daemon that replaced the old one");

    assert_eq!(
        *recorder.disconnects.lock().unwrap(),
        0,
        "a planned swap was reported to the window as a daemon lost"
    );

    drop(client);
    // Not joined: the stand-in is still holding the connection this client
    // opened, and waiting for it to notice would be waiting for the test's own
    // reader thread to be torn down first.
    drop(stand_in);
}
