//! Checks that a session gets a clean environment rather than inheriting the
//! identity of whatever launched Beacon.
//!
//! This lives in its own test binary on purpose: it sets process-wide
//! environment variables, and doing that alongside other tests spawning shells
//! in parallel would be a data race.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beacon_core::domain::ProjectId;
use beacon_core::session::{SessionEvents, SessionId, SessionKind, SessionManager};

#[derive(Default)]
struct Recorder {
    output: Mutex<Vec<u8>>,
}

impl SessionEvents for Recorder {
    fn output(&self, _id: &SessionId, _offset: u64, bytes: &[u8]) {
        self.output.lock().unwrap().extend_from_slice(bytes);
    }

    fn exited(&self, _id: &SessionId, _code: Option<i32>) {}
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
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_session_does_not_inherit_the_launching_terminals_identity() {
    // Pretend Beacon was started from Terminal.app, which is exactly what
    // happens when it is launched from a shell running there. With TERM_PROGRAM
    // still set, macOS's /etc/zshrc engages Terminal.app's session restore and
    // sources ~/.zsh_sessions/$TERM_SESSION_ID.session — another terminal's
    // file, which may well not exist.
    //
    // Safe here: this binary runs one test and sets these before spawning
    // anything.
    unsafe {
        std::env::set_var("TERM_PROGRAM", "Apple_Terminal");
        std::env::set_var("TERM_SESSION_ID", "some-other-terminals-session");
    }

    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();

    // The markers are split across printf arguments so that the shell echoing
    // the command back does not itself look like the answer.
    manager
        .write(
            &id,
            b"printf 'pr%s=%s si%s=%s\\n' og \"$TERM_PROGRAM\" d \"$TERM_SESSION_ID\"\n",
        )
        .unwrap();

    assert!(
        wait_for(Duration::from_secs(10), || recorder
            .text()
            .contains("prog=")),
        "the shell never reported its environment; saw: {:?}",
        recorder.text()
    );

    let seen = recorder.text();
    assert!(
        seen.contains("prog=Beacon"),
        "Beacon should present its own identity; saw: {seen:?}"
    );
    assert!(
        !seen.contains("some-other-terminals-session"),
        "the launching terminal's session id leaked through; saw: {seen:?}"
    );

    manager.close(&id).unwrap();
}
