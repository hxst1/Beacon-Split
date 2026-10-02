//! One sign-in for every open project.
//!
//! Open several projects before signing in to Claude Code and every panel sits
//! on its own sign-in screen. Signing in from one of them stores the credential
//! where every `claude` on the machine reads it — but only when it starts, so
//! the others go on waiting, and the user goes through the browser once per
//! project.
//!
//! Beacon never touches the credential (ADR-052). What it can do is notice the
//! moment there is one, by asking Claude Code itself — `claude auth status` —
//! and start the waiting panels again, at which point each of them finds it on
//! its own. This is the bookkeeping for that, kept free of processes and clocks
//! so it can be tested; the daemon does the asking and the restarting.
//!
//! The panel somebody signed in from is left alone. It is the one where Return
//! was pressed last, because every step of signing in ends with Return and
//! nothing the terminal writes by itself contains one.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::session::SessionId;

/// How long after a Claude session starts before the first question.
///
/// Restoring a window starts every project's session within a moment of each
/// other; waiting a beat means one `claude auth status` answers for all of
/// them instead of one each.
pub const FIRST_LOOK: Duration = Duration::from_secs(1);

/// How often to ask again while something is waiting.
///
/// Signing in means a trip to the browser and back, so a few seconds after it
/// is still before anyone has looked at the other panels.
pub const POLL: Duration = Duration::from_secs(3);

/// How long panels may wait before Beacon stops asking for them.
///
/// Someone who opened their projects and walked away has not signed in, and a
/// question every few seconds for the rest of the day is not worth what it
/// would catch. The panels are left exactly as they are.
pub const GIVE_UP: Duration = Duration::from_secs(30 * 60);

/// What the watcher does after an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Something may still be waiting; ask again after this long.
    AskAgainIn(Duration),
    /// Signed in now. Start these again; the watcher is finished.
    Restart(Vec<SessionId>),
    /// Nothing to do; the watcher is finished.
    Stop,
}

/// Which Claude sessions started signed out and are still waiting.
#[derive(Debug, Default)]
pub struct SignInWatch {
    /// Every session ever shown to the watch, so that reattaching to a live one
    /// — which every window does on opening — is not mistaken for a new start.
    seen: HashSet<SessionId>,
    /// Started, and not yet asked about, with when they started.
    unchecked: HashMap<SessionId, Instant>,
    /// Known to have started while nobody was signed in.
    waiting: HashSet<SessionId>,
    waiting_since: Option<Instant>,
    /// Whether a watcher is running. Owned here, under the same lock as the
    /// sets, so a session started while the watcher is finishing either is in
    /// its last answer or starts a new one — never neither.
    watching: bool,
}

impl SignInWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// A Claude session started, or was asked for.
    ///
    /// Returns whether the caller has to start a watcher, which is the case
    /// when this session is new and none is running.
    pub fn observe(&mut self, id: SessionId, now: Instant) -> bool {
        if !self.seen.insert(id.clone()) {
            return false;
        }
        self.unchecked.insert(id, now);
        !std::mem::replace(&mut self.watching, true)
    }

    /// A session started by the watch's own restart, which is signed in by
    /// construction and must not be asked about again when a window reattaches.
    pub fn signed_in_already(&mut self, id: SessionId) {
        self.seen.insert(id);
    }

    /// Ends the watch with nothing done, for a watcher that could not start.
    ///
    /// Without this `watching` would stay set and no watcher would ever be
    /// started again for as long as the daemon runs.
    pub fn abandon(&mut self) {
        self.unchecked.clear();
        self.waiting.clear();
        self.waiting_since = None;
        self.watching = false;
    }

    /// Takes Claude Code's answer, asked at `asked_at`, and says what next.
    ///
    /// `None` is no answer — an older Claude Code, or one that would not say —
    /// and ends the watch with nothing done: the feature switches itself off
    /// rather than guessing.
    ///
    /// Only sessions that started before the question count as signed out by a
    /// "no": one that started while it was being asked may have started after
    /// the sign-in, and is asked about next time instead.
    pub fn answer(
        &mut self,
        signed_in: Option<bool>,
        asked_at: Instant,
        now: Instant,
        alive: impl Fn(&SessionId) -> bool,
        last_return: impl Fn(&SessionId) -> Option<Instant>,
    ) -> Next {
        // Closed and exited sessions are nobody's concern any more.
        self.seen.retain(&alive);
        self.unchecked.retain(|id, _| alive(id));
        self.waiting.retain(&alive);

        let next = match signed_in {
            None => {
                self.unchecked.clear();
                self.waiting.clear();
                Next::Stop
            }
            Some(true) => {
                self.unchecked.clear();
                let waiting = std::mem::take(&mut self.waiting);
                if waiting.is_empty() {
                    Next::Stop
                } else {
                    Next::Restart(left_waiting(waiting, last_return))
                }
            }
            Some(false) => {
                let asked_about: Vec<SessionId> = self
                    .unchecked
                    .iter()
                    .filter(|(_, started)| **started <= asked_at)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in asked_about {
                    self.unchecked.remove(&id);
                    self.waiting.insert(id);
                }

                if !self.waiting.is_empty() {
                    self.waiting_since.get_or_insert(now);
                }
                let given_up = self
                    .waiting_since
                    .is_some_and(|since| now.duration_since(since) >= GIVE_UP);

                if given_up {
                    self.unchecked.clear();
                    self.waiting.clear();
                    Next::Stop
                } else if self.waiting.is_empty() && self.unchecked.is_empty() {
                    Next::Stop
                } else {
                    Next::AskAgainIn(POLL)
                }
            }
        };

        if !matches!(next, Next::AskAgainIn(_)) {
            self.watching = false;
            self.waiting_since = None;
        }
        next
    }
}

/// Every waiting session but the one somebody signed in from.
///
/// That one is the session where Return was pressed last. When none of them
/// has had Return pressed, the sign-in happened somewhere else — a terminal, or
/// another Claude — and all of them are still waiting.
fn left_waiting(
    waiting: HashSet<SessionId>,
    last_return: impl Fn(&SessionId) -> Option<Instant>,
) -> Vec<SessionId> {
    let signed_in_from = waiting
        .iter()
        .filter_map(|id| Some((last_return(id)?, id)))
        .max_by_key(|(at, _)| *at)
        .map(|(_, id)| id.clone());

    let mut restart: Vec<SessionId> = waiting
        .into_iter()
        .filter(|id| Some(id) != signed_in_from.as_ref())
        .collect();
    restart.sort();
    restart
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> SessionId {
        SessionId(name.to_string())
    }

    fn everyone_alive(_: &SessionId) -> bool {
        true
    }

    fn nobody_typed(_: &SessionId) -> Option<Instant> {
        None
    }

    /// Three sessions started at `start`, all found signed out a moment later.
    fn three_waiting(start: Instant) -> SignInWatch {
        let mut watch = SignInWatch::new();
        assert!(watch.observe(id("a"), start));
        assert!(!watch.observe(id("b"), start));
        assert!(!watch.observe(id("c"), start));

        let asked = start + FIRST_LOOK;
        assert_eq!(
            watch.answer(Some(false), asked, asked, everyone_alive, nobody_typed),
            Next::AskAgainIn(POLL)
        );
        watch
    }

    #[test]
    fn one_watcher_serves_every_session_that_starts_while_it_runs() {
        let now = Instant::now();
        let mut watch = SignInWatch::new();

        assert!(
            watch.observe(id("a"), now),
            "the first start needs a watcher"
        );
        assert!(!watch.observe(id("b"), now), "the second shares it");
    }

    #[test]
    fn reattaching_to_a_session_is_not_a_new_start() {
        let now = Instant::now();
        let mut watch = SignInWatch::new();
        watch.observe(id("a"), now);
        assert_eq!(
            watch.answer(Some(true), now, now, everyone_alive, nobody_typed),
            Next::Stop
        );

        assert!(
            !watch.observe(id("a"), now),
            "already seen, so nothing to ask"
        );
    }

    #[test]
    fn a_session_the_watch_restarted_is_not_asked_about() {
        let now = Instant::now();
        let mut watch = SignInWatch::new();
        watch.signed_in_already(id("a"));

        assert!(!watch.observe(id("a"), now));
    }

    #[test]
    fn an_abandoned_watch_can_be_started_again() {
        let now = Instant::now();
        let mut watch = SignInWatch::new();
        assert!(watch.observe(id("a"), now));
        watch.abandon();

        assert!(watch.observe(id("b"), now));
    }

    #[test]
    fn signed_in_from_the_start_is_left_entirely_alone() {
        let now = Instant::now();
        let mut watch = SignInWatch::new();
        watch.observe(id("a"), now);
        watch.observe(id("b"), now);

        assert_eq!(
            watch.answer(Some(true), now, now, everyone_alive, nobody_typed),
            Next::Stop
        );
        // Finished, so the next start needs a watcher of its own.
        assert!(watch.observe(id("c"), now));
    }

    #[test]
    fn signing_in_starts_again_every_panel_but_the_one_it_happened_in() {
        let start = Instant::now();
        let mut watch = three_waiting(start);

        // Return pressed in "a" early on, then the whole sign-in done in "b".
        let typed = |session: &SessionId| match session.0.as_str() {
            "a" => Some(start + Duration::from_secs(2)),
            "b" => Some(start + Duration::from_secs(9)),
            _ => None,
        };
        let later = start + Duration::from_secs(12);
        assert_eq!(
            watch.answer(Some(true), later, later, everyone_alive, typed),
            Next::Restart(vec![id("a"), id("c")])
        );
    }

    #[test]
    fn signing_in_somewhere_else_starts_every_panel_again() {
        let start = Instant::now();
        let mut watch = three_waiting(start);

        let later = start + Duration::from_secs(12);
        assert_eq!(
            watch.answer(Some(true), later, later, everyone_alive, nobody_typed),
            Next::Restart(vec![id("a"), id("b"), id("c")])
        );
    }

    #[test]
    fn a_session_that_is_gone_is_not_started_again() {
        let start = Instant::now();
        let mut watch = three_waiting(start);

        let later = start + Duration::from_secs(12);
        let not_b = |session: &SessionId| session.0 != "b";
        assert_eq!(
            watch.answer(Some(true), later, later, not_b, nobody_typed),
            Next::Restart(vec![id("a"), id("c")])
        );
    }

    #[test]
    fn the_watch_ends_when_everything_waiting_has_gone() {
        let start = Instant::now();
        let mut watch = three_waiting(start);

        let later = start + Duration::from_secs(12);
        assert_eq!(
            watch.answer(Some(false), later, later, |_| false, nobody_typed),
            Next::Stop
        );
    }

    #[test]
    fn a_session_started_during_the_question_is_asked_about_again() {
        let start = Instant::now();
        let mut watch = SignInWatch::new();
        watch.observe(id("a"), start);

        // "b" started after the question went out; a "no" cannot speak for it.
        let asked = start + FIRST_LOOK;
        watch.observe(id("b"), asked + Duration::from_millis(500));
        assert_eq!(
            watch.answer(
                Some(false),
                asked,
                asked + FIRST_LOOK,
                everyone_alive,
                nobody_typed
            ),
            Next::AskAgainIn(POLL)
        );

        let later = start + Duration::from_secs(10);
        assert_eq!(
            watch.answer(Some(true), later, later, everyone_alive, nobody_typed),
            Next::Restart(vec![id("a")]),
            "only the session known to have started signed out"
        );
    }

    #[test]
    fn no_answer_switches_the_feature_off() {
        let start = Instant::now();
        let mut watch = three_waiting(start);

        let later = start + Duration::from_secs(6);
        assert_eq!(
            watch.answer(None, later, later, everyone_alive, nobody_typed),
            Next::Stop
        );
        // Nothing is left over to restart if an answer arrives later.
        watch.observe(id("d"), later);
        assert_eq!(
            watch.answer(Some(true), later, later, everyone_alive, nobody_typed),
            Next::Stop
        );
    }

    #[test]
    fn panels_left_waiting_long_enough_are_given_up_on() {
        let start = Instant::now();
        let mut watch = three_waiting(start);

        let much_later = start + FIRST_LOOK + GIVE_UP;
        assert_eq!(
            watch.answer(
                Some(false),
                much_later,
                much_later,
                everyone_alive,
                nobody_typed
            ),
            Next::Stop
        );
        // And not restarted by a sign-in that comes after.
        watch.observe(id("d"), much_later);
        assert_eq!(
            watch.answer(
                Some(true),
                much_later,
                much_later,
                everyone_alive,
                nobody_typed
            ),
            Next::Stop
        );
    }
}
