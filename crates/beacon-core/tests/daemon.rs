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
use beacon_core::session::SessionKind;

#[derive(Default)]
struct Recorder {
    output: Mutex<Vec<u8>>,
    disconnects: Mutex<usize>,
}

impl DaemonEvents for Recorder {
    fn event(&self, event: Event) {
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
        let client = DaemonClient::connect(&binary, Arc::clone(&first) as Arc<dyn DaemonEvents>)
            .expect("should reach a daemon");

        let session = client
            .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
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
    let client = DaemonClient::connect(&binary, Arc::clone(&second) as Arc<dyn DaemonEvents>)
        .expect("should reach the same daemon");

    let again = client
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
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
    let client = DaemonClient::connect(&binary, recorder as Arc<dyn DaemonEvents>).unwrap();

    let one = ProjectId::generate();
    let other = ProjectId::generate();
    let kept = client
        .ensure(&other, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();
    client
        .ensure(&one, SessionKind::Shell, dir.path(), (80, 24))
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
