//! That choosing an account actually changes what a session is started with.
//!
//! Everything else about accounts is bookkeeping; this is the one fact the
//! feature rests on. Beacon's whole part in switching accounts is setting an
//! environment variable on the process it starts (ADR-099), so a session that
//! does not receive it is a chip that lies about which subscription the work
//! is going to.
//!
//! A real shell, asked what it got, rather than an assertion about the
//! `CommandBuilder` Beacon filled in: the question is what arrives on the
//! other side of the pty.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use beacon_core::account::AccountHomes;
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

/// Asks a shell to print both homes, with the markers split across arguments
/// so the shell echoing the command back cannot be mistaken for the answer.
fn probe() -> &'static str {
    if cfg!(windows) {
        concat!(
            "'cla{0}=[{1}] cod{2}=[{3}]' -f ",
            "'ude',$env:CLAUDE_CONFIG_DIR,",
            "'ex',$env:CODEX_HOME\r",
        )
    } else {
        concat!(
            "printf 'cla%s=[%s] cod%s=[%s]\\n' ",
            "ude \"$CLAUDE_CONFIG_DIR\" ",
            "ex \"$CODEX_HOME\"\n",
        )
    }
}

fn ask(
    manager: &SessionManager,
    recorder: &Recorder,
    project: &ProjectId,
    slot: u32,
    dir: &std::path::Path,
) -> String {
    let seen = recorder.text().matches("claude=[").count();
    let id = manager
        .ensure(project, SessionKind::Shell, slot, dir, (80, 24), None)
        .unwrap();
    manager.write(&id, probe().as_bytes()).unwrap();

    assert!(
        wait_for(Duration::from_secs(15), || {
            recorder.text().matches("claude=[").count() > seen
        }),
        "the shell never reported its environment; saw: {:?}",
        recorder.text()
    );
    recorder.text()
}

#[test]
fn a_session_is_started_with_the_account_that_was_chosen() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    // Nobody has chosen an account, which is almost everybody. Beacon must
    // leave the environment exactly as it found it: a variable set here would
    // change which account somebody's agent uses without being asked.
    let before = ask(&manager, &recorder, &project, 0, dir.path());
    assert!(
        before.contains("claude=[]") && before.contains("codex=[]"),
        "Beacon pointed a session somewhere without being asked to; saw: {before:?}"
    );

    // Now two accounts are chosen.
    let claude = dir.path().join("a-claude-home");
    let codex = dir.path().join("a-codex-home");
    manager.set_account_homes(
        project.clone(),
        AccountHomes {
            claude: Some(claude.clone()),
            codex: Some(codex.clone()),
        },
    );

    // A shell hears about both, because a `claude` typed in Beacon's terminal
    // should be the same account as the panel beside it.
    let after = ask(&manager, &recorder, &project, 1, dir.path());
    assert!(
        after.contains(&format!("claude=[{}]", claude.display())),
        "the chosen Claude home never reached the session; saw: {after:?}"
    );
    assert!(
        after.contains(&format!("codex=[{}]", codex.display())),
        "the chosen Codex home never reached the session; saw: {after:?}"
    );
}

/// The homes belong to the project whose client sent them.
///
/// One value for the whole daemon would be whichever account was chosen most
/// recently, and this is the case that goes wrong: a project whose client
/// never said anything about accounts would inherit another project's, and
/// everything that reads back what the agent wrote — the transcript that
/// decides whether a conversation can be resumed, the rollout the context
/// percentage comes from — would be looking in the wrong directory.
#[test]
fn one_projects_account_is_not_another_projects() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();

    let chose = ProjectId::generate();
    let did_not = ProjectId::generate();
    manager.set_account_homes(
        chose.clone(),
        AccountHomes {
            claude: Some(dir.path().join("chosen")),
            codex: None,
        },
    );

    let seen = ask(&manager, &recorder, &did_not, 0, dir.path());
    assert!(
        seen.contains("claude=[]"),
        "another project's account leaked into this one; saw: {seen:?}"
    );

    // And the project that did choose still gets it.
    let theirs = ask(&manager, &recorder, &chose, 0, dir.path());
    assert!(
        theirs.contains(&format!("claude=[{}]", dir.path().join("chosen").display())),
        "the project that chose an account did not get it; saw: {theirs:?}"
    );
}

/// Asking for a session that is already running must not change what Beacon
/// believes that session is using.
///
/// `Ensure` is an ensure: a panel that re-mounts asks for the session it
/// already has and gets it back, while still saying which account it would
/// like — and after a switch, that is the new one. Reading the wanted account
/// rather than the started one would mean a running session being looked up
/// under an account it is not on: its transcript reported as never written,
/// which Claude Code answers with `Session ID … is already in use`.
#[test]
fn switching_account_does_not_move_a_session_that_is_already_running() {
    let recorder = Arc::new(Recorder::default());
    let manager = SessionManager::new(Arc::clone(&recorder) as Arc<dyn SessionEvents>);
    let dir = tempfile::tempdir().unwrap();
    let project = ProjectId::generate();

    let first = dir.path().join("first");
    manager.set_account_homes(
        project.clone(),
        AccountHomes {
            claude: Some(first.clone()),
            codex: None,
        },
    );
    let started = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();
    assert_eq!(
        manager.started_with(&project, SessionKind::Shell).claude,
        Some(first.clone())
    );

    // The account is switched, and the panel asks for its session again.
    manager.set_account_homes(
        project.clone(),
        AccountHomes {
            claude: Some(dir.path().join("second")),
            codex: None,
        },
    );
    let same = manager
        .ensure(&project, SessionKind::Shell, 0, dir.path(), (80, 24), None)
        .unwrap();

    assert_eq!(
        same, started,
        "ensure should have handed back the same session"
    );
    assert_eq!(
        manager.started_with(&project, SessionKind::Shell).claude,
        Some(first),
        "the running session is still on the account it started with"
    );
}
