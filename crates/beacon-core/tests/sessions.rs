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
    fn output(&self, _id: &SessionId, _project: &ProjectId, _offset: u64, bytes: &[u8]) {
        self.output.lock().unwrap().extend_from_slice(bytes);
    }

    fn exited(&self, id: &SessionId, _project: &ProjectId, _code: Option<i32>) {
        self.exits.lock().unwrap().push(id.clone());
    }
}

impl Recorder {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }
}

/// The key a terminal sends for Enter. A unix pty turns a newline into one
/// anyway; a Windows pseudo-console takes a bare newline as the line going on.
const ENTER: &str = if cfg!(windows) { "\r" } else { "\n" };

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
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .expect("session should start");

    // Split by an empty quote, so the shell echoing what was typed is not
    // mistaken for the command having run.
    manager
        .write(&id, format!("echo beacon''-ok{ENTER}").as_bytes())
        .unwrap();

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
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();
    let second = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
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
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
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

/// A shell that ends by itself — `exit` typed into it — is reported gone, and
/// only once. Elsewhere that is the pty closing. On Windows the
/// pseudo-console's output outlives the process, so it comes from waiting on
/// the process instead; the reader reaching the end later must not say it
/// again.
#[test]
fn a_shell_that_exits_by_itself_is_reported_once() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();
    manager
        .write(&id, format!("exit{ENTER}").as_bytes())
        .unwrap();

    let reported = || {
        recorder
            .exits
            .lock()
            .unwrap()
            .iter()
            .filter(|exited| **exited == id)
            .count()
    };
    assert!(
        wait_for(Duration::from_secs(10), || reported() > 0),
        "a shell that exited was never reported gone; saw: {:?}",
        recorder.text()
    );

    drop(manager);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(reported(), 1, "the exit was reported more than once");
}

#[test]
fn restarting_replaces_the_session_for_the_project() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let first = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();
    let second = manager
        .restart_for(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();

    assert_ne!(first, second);
    // The project now points at the replacement, not the dead one.
    let reused = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();
    assert_eq!(reused, second);

    manager.close(&second).unwrap();
}

#[test]
fn restarting_a_project_with_no_session_just_starts_one() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .restart_for(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .expect("restart should start a session rather than fail");
    assert!(manager.info(&id).unwrap().running);

    manager.close(&id).unwrap();
}

#[test]
fn a_project_can_run_a_shell_and_claude_at_the_same_time() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let shell = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();

    // Claude may not be installed wherever this runs; what matters is that it
    // is tracked separately from the shell rather than replacing it.
    if let Ok(claude) = manager.ensure(&project, SessionKind::Claude, 0, dir.path(), (80, 24), None)
    {
        assert_ne!(shell, claude);
        assert_eq!(manager.info(&claude).unwrap().kind, SessionKind::Claude);
        manager.close(&claude).unwrap();
    }

    assert!(manager.info(&shell).unwrap().running);
    manager.close(&shell).unwrap();
}

#[test]
fn resizing_a_live_session_succeeds() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();

    manager.resize(&id, 120, 40).expect("resize should succeed");
    manager.close(&id).unwrap();
}

/// A window rebuilding a panel has to know the grid the output it is about to
/// replay was written for.
#[test]
fn a_session_reports_the_grid_its_process_was_told_about() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(recorder as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (132, 40), None)
        .unwrap();

    let info = manager.info(&id).unwrap();
    assert_eq!((info.cols, info.rows), (132, 40));

    // And it follows the window rather than remembering how it started —
    // replaying at the size it was first opened at would be wrong for every
    // session anybody has resized.
    manager.resize(&id, 90, 30).unwrap();
    let after = manager.info(&id).unwrap();
    assert_eq!((after.cols, after.rows), (90, 30));

    // The listing a reattaching client reads says the same thing.
    let listed = manager.list().into_iter().find(|s| s.id == id).unwrap();
    assert_eq!((listed.cols, listed.rows), (90, 30));

    manager.close(&id).unwrap();
}
