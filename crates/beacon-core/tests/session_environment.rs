//! Checks that a spawned session gets a clean environment rather than
//! inheriting the state of whatever launched Beacon.
//!
//! This lives in its own test binary, and is deliberately a single test: it
//! sets process-wide environment variables, and doing that alongside anything
//! else running concurrently would be a data race.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beacon_core::domain::ProjectId;
use beacon_core::session::{SessionEvents, SessionId, SessionKind, SessionManager};

#[derive(Default)]
struct Recorder {
    output: Mutex<Vec<u8>>,
}

impl SessionEvents for Recorder {
    fn output(&self, _id: &SessionId, _project: &ProjectId, _offset: u64, bytes: &[u8]) {
        self.output.lock().unwrap().extend_from_slice(bytes);
    }

    fn exited(&self, _id: &SessionId, _project: &ProjectId, _code: Option<i32>) {}
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
fn a_session_does_not_inherit_the_launchers_state() {
    // Two launchers Beacon is realistically started from, both of which leave
    // per-process state in the environment:
    //
    // Terminal.app — with TERM_PROGRAM still set, macOS's /etc/zshrc engages
    // that terminal's session restore and sources
    // ~/.zsh_sessions/$TERM_SESSION_ID.session, another terminal's file.
    //
    // Claude Code — the child-session marker makes the `claude` Beacon starts
    // believe it is nested, which turns transcript saving off. The messaging
    // token is the parent's private channel.
    //
    // ANTHROPIC_BASE_URL stands in for user configuration, which is a different
    // thing entirely and must survive.
    //
    // Safe here: this binary runs one test and sets these before spawning
    // anything.
    unsafe {
        std::env::set_var("TERM_PROGRAM", "Apple_Terminal");
        std::env::set_var("TERM_SESSION_ID", "some-other-terminals-session");
        std::env::set_var("CLAUDE_CODE_CHILD_SESSION", "1");
        std::env::set_var("CLAUDE_CODE_MESSAGING_TOKEN", "a-private-token");
        std::env::set_var("ANTHROPIC_BASE_URL", "https://example.invalid");
    }

    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let id = manager
        .ensure(&project, SessionKind::Shell, dir.path(), (80, 24))
        .unwrap();

    // The markers are split across printf arguments so the shell echoing the
    // command back does not itself look like the answer.
    let probe = concat!(
        "printf 'pr%s=[%s] si%s=[%s] chi%s=[%s] tok%s=[%s] ba%s=[%s]\\n' ",
        "og \"$TERM_PROGRAM\" ",
        "d \"$TERM_SESSION_ID\" ",
        "ld \"$CLAUDE_CODE_CHILD_SESSION\" ",
        "en \"$CLAUDE_CODE_MESSAGING_TOKEN\" ",
        "se \"$ANTHROPIC_BASE_URL\"\n",
    );
    manager.write(&id, probe.as_bytes()).unwrap();

    assert!(
        wait_for(Duration::from_secs(15), || recorder
            .text()
            .contains("prog=")),
        "the shell never reported its environment; saw: {:?}",
        recorder.text()
    );
    let seen = recorder.text();

    assert!(
        seen.contains("prog=[Beacon]"),
        "Beacon should present its own terminal identity; saw: {seen:?}"
    );
    assert!(
        !seen.contains("some-other-terminals-session"),
        "the launching terminal's session id leaked; saw: {seen:?}"
    );
    assert!(
        seen.contains("child=[]"),
        "Claude Code's child-session marker leaked; saw: {seen:?}"
    );
    assert!(
        !seen.contains("a-private-token"),
        "the parent's messaging token leaked; saw: {seen:?}"
    );
    assert!(
        seen.contains("base=[https://example.invalid]"),
        "user configuration should be passed through; saw: {seen:?}"
    );

    manager.close(&id).unwrap();
}
