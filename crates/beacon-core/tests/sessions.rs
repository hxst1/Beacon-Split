//! Exercises a real PTY. These tests spawn actual shells, which is the only way
//! to know the plumbing works.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beacon_core::domain::ProjectId;
use beacon_core::session::{SessionEvents, SessionId, SessionKind, SessionManager};

#[derive(Default)]
struct Recorder {
    output: Mutex<Vec<u8>>,
    exits: Mutex<Vec<SessionId>>,
}

impl SessionEvents for Recorder {
    fn output(&self, _id: &SessionId, _offset: u64, bytes: &[u8]) {
        self.output.lock().unwrap().extend_from_slice(bytes);
    }

    fn exited(&self, id: &SessionId, _code: Option<i32>) {
        self.exits.lock().unwrap().push(id.clone());
    }
}

impl Recorder {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }
}

/// Polls until `predicate` holds, so tests do not depend on a fixed sleep.
fn wait_for(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_shell_session_runs_commands_and_reports_output() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .expect("session should start");

    manager.write(&id, b"echo beacon-ok\n").unwrap();

    assert!(
        wait_for(Duration::from_secs(10), || recorder
            .text()
            .contains("beacon-ok")),
        "shell never echoed; saw: {:?}",
        recorder.text()
    );

    // Whatever the shell printed is replayable without asking it again.
    let (scrollback, end) = manager.scrollback(&id).unwrap();
    assert!(String::from_utf8_lossy(&scrollback).contains("beacon-ok"));
    assert!(end >= scrollback.len() as u64);

    manager.close(&id).unwrap();
}

#[test]
fn the_same_project_reuses_its_session() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let first = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();
    let second = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();

    assert_eq!(first, second, "switching tabs must not respawn the shell");
    manager.close(&first).unwrap();
}

#[test]
fn closing_a_session_reports_it_gone() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();
    assert!(manager.info(&id).unwrap().running);

    manager.close(&id).unwrap();

    assert!(
        manager.info(&id).is_err(),
        "a closed session should be gone"
    );
    assert!(
        wait_for(Duration::from_secs(5), || !recorder
            .exits
            .lock()
            .unwrap()
            .is_empty()),
        "the reader thread should report the exit"
    );
}

#[test]
fn restarting_replaces_the_session_for_the_project() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let first = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();
    let second = manager.restart(&first, (80, 24)).unwrap();

    assert_ne!(first, second);
    // The project now points at the replacement, not the dead one.
    let reused = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();
    assert_eq!(reused, second);

    manager.close(&second).unwrap();
}

#[test]
fn resizing_a_live_session_succeeds() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();

    manager.resize(&id, 120, 40).expect("resize should succeed");
    manager.close(&id).unwrap();
}
