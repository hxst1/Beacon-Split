//! What the installed Claude Code can actually do.
//!
//! Beacon leans on Claude Code's own interfaces, and those interfaces arrive
//! over time. A build from six months ago has no `--session-id` to assign and
//! no `--agents` to fill; a feature built on either has to disappear there
//! rather than fail there.
//!
//! Capabilities are read out of `--help`, not out of a table of version
//! numbers. A table would be a list of guesses about when each flag landed,
//! wrong in a way nobody would notice until someone's Beacon quietly stopped
//! offering something. The help text is the program describing itself, which is
//! the only answer that cannot be out of date.
//!
//! Some things cannot be asked about at all. No help text lists which hook
//! events exist, or whether `CLAUDE_CODE_TASK_LIST_ID` is honoured. Those are
//! deliberately absent here: an unknown hook event is simply never fired and an
//! unknown variable is simply ignored, so the honest test is whether anything
//! ever arrives — which is a fact the daemon holds, not a fact about the
//! binary.

use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::tools::{capture_briefly, resolve_program, strip_terminal_identity};

/// How long Claude Code gets to describe itself before Beacon stops waiting.
///
/// Short on purpose: every one of these runs on the way to something the user
/// asked for, and an answer that arrives after the session started is worth
/// less than no answer at all.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// A parsed `major.minor.patch`, for saying "requires Claude Code 2.1 or later"
/// in a message a person can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ClaudeVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl ClaudeVersion {
    /// Reads the leading version out of what `claude --version` prints, which
    /// is `2.1.252 (Claude Code)`.
    pub fn parse(text: &str) -> Option<Self> {
        let token = text.split_whitespace().next()?;
        let mut parts = token.split('.');
        let mut number = || parts.next()?.parse::<u32>().ok();

        Some(Self {
            major: number()?,
            minor: number()?,
            // A two-part version is still a version.
            patch: number().unwrap_or(0),
        })
    }
}

impl std::fmt::Display for ClaudeVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What this machine's Claude Code offers Beacon.
///
/// Every field is a fact about the installed binary, and every one of them
/// gates a feature that hides itself when the answer is no.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Exactly what `claude --version` said, for showing and for reporting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_version: Option<ClaudeVersion>,
    /// `--session-id <uuid>`: Beacon chooses the conversation's id when it
    /// starts one. This is what makes a workstream something Beacon can name,
    /// find and resume without ever reading a transcript.
    pub assigned_session_id: bool,
    /// `-n, --name <name>`
    pub named_sessions: bool,
    /// `-r, --resume [value]`
    pub resume: bool,
    /// `--fork-session`
    pub fork_session: bool,
    /// `-w, --worktree [name]`
    pub worktree: bool,
    /// `--agents <json>`: agent definitions that live for one session and write
    /// nothing into the user's repository.
    pub session_agents: bool,
    /// `--settings <file-or-json>`
    pub session_settings: bool,
    /// `--append-system-prompt <prompt>`
    pub append_system_prompt: bool,
    /// `--effort <level>`
    pub effort: bool,
    /// `--model <model>`
    pub model: bool,
    /// `claude agents --json`: which sessions exist right now, said by Claude
    /// Code rather than inferred. What "do not open the same session twice" is
    /// checked against.
    pub session_listing: bool,
}

impl Capabilities {
    /// Nothing found. Every feature that needs Claude Code hides itself.
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether a named, resumable workstream is possible at all.
    ///
    /// The three flags travel together: without an id Beacon cannot address a
    /// session, and without resume it could not return to one it addressed.
    pub fn workstreams(&self) -> bool {
        self.assigned_session_id && self.named_sessions && self.resume
    }
}

/// Reads the capabilities out of what Claude Code prints about itself.
///
/// Pure, and given both help texts rather than running anything, so the parsing
/// can be tested without a Claude Code on the machine running the tests.
pub fn interpret(version: Option<&str>, help: &str, agents_help: &str) -> Capabilities {
    // Matched with the leading dashes and, where the flag takes one, the
    // following delimiter. `--model` and `--model-something-else` are different
    // flags, and a bare substring search cannot tell them apart.
    let has = |flag: &str| {
        help.match_indices(flag).any(|(at, _)| {
            let after = help[at + flag.len()..].chars().next();
            matches!(
                after,
                None | Some(' ') | Some('\n') | Some(',') | Some('<') | Some('[')
            )
        })
    };

    Capabilities {
        version: version.map(str::to_string),
        parsed_version: version.and_then(ClaudeVersion::parse),
        assigned_session_id: has("--session-id"),
        named_sessions: has("--name"),
        resume: has("--resume"),
        fork_session: has("--fork-session"),
        worktree: has("--worktree"),
        session_agents: has("--agents"),
        session_settings: has("--settings"),
        append_system_prompt: has("--append-system-prompt"),
        effort: has("--effort"),
        model: has("--model"),
        // Asked of the subcommand, because the top-level help only says the
        // subcommand exists — not that it can print machine-readable output.
        session_listing: agents_help.contains("--json"),
    }
}

/// What this machine's Claude Code can do, worked out once.
///
/// Cached for the life of the process: it costs three short processes, it
/// cannot change under a running binary, and it is read on the way to starting
/// a session — which is not a place to spend a fork.
pub fn capabilities() -> &'static Capabilities {
    static CACHED: OnceLock<Capabilities> = OnceLock::new();
    CACHED.get_or_init(detect)
}

fn detect() -> Capabilities {
    let Some(path) = resolve_program("claude") else {
        return Capabilities::none();
    };

    let version = ask(&path, &["--version"]);
    let help = ask(&path, &["--help"]).unwrap_or_default();
    // Skipped when the subcommand is not there to ask; running it would only
    // print the top-level help again and could match `--json` from elsewhere.
    let agents_help = if help.contains("\n  agents") {
        ask(&path, &["agents", "--help"]).unwrap_or_default()
    } else {
        String::new()
    };

    interpret(version.as_deref(), &help, &agents_help)
}

/// Runs Claude Code for its own description of itself.
///
/// Best effort throughout: a Claude Code that will not answer is treated as one
/// that offers nothing, which hides features rather than breaking them.
fn ask(path: &std::path::Path, args: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(path);
    command.args(args);
    strip_terminal_identity(&mut command);

    let text = capture_briefly(&mut command, PROBE_TIMEOUT)?
        .trim()
        .to_string();
    (!text.is_empty()).then_some(text)
}

/// A Claude session that is running right now, as Claude Code lists it.
///
/// The shape `claude agents --json` prints. Every field but the id is optional
/// here even where Claude Code always fills it in: this is another program's
/// output, and a field that stops being printed should cost a detail rather
/// than the whole listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSession {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// `interactive` or `background`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// `idle`, `busy`, or absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Unix milliseconds, which is what Claude Code prints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
}

/// Reads a listing, keeping the entries it understands.
///
/// Entry by entry rather than all at once: one session Beacon cannot make sense
/// of must not hide the rest, and this listing is what "already open somewhere
/// else" is decided from — a guard that silently sees nothing is worse than no
/// guard, because it would let Beacon open a conversation twice.
pub fn parse_sessions(json: &str) -> Vec<LiveSession> {
    let Ok(serde_json::Value::Array(entries)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };

    entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<LiveSession>(entry).ok())
        .filter(|session| !session.session_id.is_empty())
        .collect()
}

/// Which Claude sessions are running, said by Claude Code rather than inferred.
///
/// Asked when it matters rather than polled: this costs a process, and the only
/// question it answers — is this conversation already open somewhere — is asked
/// at the moment somebody tries to open one.
///
/// An empty list is returned both for "none running" and for "could not ask".
/// Callers must not read it as proof that nothing is running; see
/// [`is_running`], which says so in its own signature.
pub fn live_sessions() -> Vec<LiveSession> {
    let Some(path) = resolve_program("claude") else {
        return Vec::new();
    };
    if !capabilities().session_listing {
        return Vec::new();
    }

    ask(&path, &["agents", "--json"])
        .map(|json| parse_sessions(&json))
        .unwrap_or_default()
}

/// Whether a conversation is already open in some Claude process.
///
/// `None` means Claude Code could not be asked — an older build with no
/// listing, or one that would not answer. The distinction is the point:
/// opening the same conversation twice is a thing to refuse when it is known to
/// be open, and a thing to warn about when it cannot be known.
pub fn is_running(session_id: &str) -> Option<bool> {
    if !capabilities().session_listing {
        return None;
    }
    let path = resolve_program("claude")?;
    let json = ask(&path, &["agents", "--json"])?;

    Some(
        parse_sessions(&json)
            .iter()
            .any(|session| session.session_id == session_id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the real `claude --help` of 2.1.252.
    const HELP: &str = "\
Usage: claude [options] [command] [prompt]

Options:
  --agents <json>                       JSON object defining custom agents
  --append-system-prompt <prompt>       Append a system prompt
  --effort <level>                      Effort level for the current session
  --fork-session                        When resuming, create a new session ID
  --model <model>                       Model for the current session
  -n, --name <name>                     Set a display name for this session
  -r, --resume [value]                  Resume a conversation by session ID
  --session-id <uuid>                   Use a specific session ID
  --settings <file-or-json>             Path to a settings JSON file
  -w, --worktree [name]                 Create a new git worktree

Commands:
  agents [options]                      Manage background agents
";

    const AGENTS_HELP: &str = "\
Usage: claude agents [options]

Options:
  --json                                Print active sessions as a JSON array
";

    fn current() -> Capabilities {
        interpret(Some("2.1.252 (Claude Code)"), HELP, AGENTS_HELP)
    }

    #[test]
    fn reads_the_version_out_of_what_claude_prints() {
        let version = ClaudeVersion::parse("2.1.252 (Claude Code)").unwrap();
        assert_eq!(
            version,
            ClaudeVersion {
                major: 2,
                minor: 1,
                patch: 252
            }
        );
    }

    #[test]
    fn a_two_part_version_is_still_a_version() {
        assert_eq!(ClaudeVersion::parse("3.0").unwrap().patch, 0);
    }

    #[test]
    fn nonsense_is_no_version_rather_than_a_wrong_one() {
        for text in ["", "unknown", "Claude Code", "v.next"] {
            assert!(ClaudeVersion::parse(text).is_none(), "parsed {text:?}");
        }
    }

    #[test]
    fn versions_compare_the_way_a_person_would_read_them() {
        let older = ClaudeVersion::parse("2.1.90").unwrap();
        let newer = ClaudeVersion::parse("2.1.252").unwrap();
        assert!(newer > older);
        assert!(ClaudeVersion::parse("2.2.0").unwrap() > newer);
    }

    #[test]
    fn the_installed_build_offers_everything_workstreams_need() {
        let capabilities = current();
        assert!(capabilities.assigned_session_id);
        assert!(capabilities.named_sessions);
        assert!(capabilities.resume);
        assert!(capabilities.workstreams());
    }

    #[test]
    fn reads_the_rest_of_the_flags_beacon_depends_on() {
        let capabilities = current();
        assert!(capabilities.fork_session);
        assert!(capabilities.worktree);
        assert!(capabilities.session_agents);
        assert!(capabilities.session_settings);
        assert!(capabilities.append_system_prompt);
        assert!(capabilities.effort);
        assert!(capabilities.model);
        assert!(capabilities.session_listing);
    }

    #[test]
    fn a_flag_that_is_not_there_is_not_claimed() {
        // The build that shipped before workstreams were possible.
        let old = "\
Options:
  --model <model>                       Model for the current session
  -r, --resume [value]                  Resume a conversation by session ID
";
        let capabilities = interpret(Some("1.0.60 (Claude Code)"), old, "");

        assert!(capabilities.resume);
        assert!(capabilities.model);
        assert!(!capabilities.assigned_session_id);
        assert!(!capabilities.named_sessions);
        assert!(!capabilities.session_agents);
        assert!(!capabilities.workstreams());
    }

    #[test]
    fn a_longer_flag_does_not_answer_for_a_shorter_one() {
        // `--session-id` must not be read out of `--session-id-file`, and
        // `--agents` must not be read out of `--agents-dir`.
        let misleading = "\
Options:
  --session-id-file <path>              Somewhere to write the id
  --agents-dir <path>                   Where to look for agents
";
        let capabilities = interpret(None, misleading, "");
        assert!(!capabilities.assigned_session_id);
        assert!(!capabilities.session_agents);
    }

    #[test]
    fn the_agents_subcommand_alone_does_not_prove_it_speaks_json() {
        // Present, but from a build whose `agents` could only be interactive.
        let capabilities = interpret(None, HELP, "Usage: claude agents [options]\n");
        assert!(!capabilities.session_listing);
    }

    #[test]
    fn what_is_installed_here_describes_itself() {
        // Machine-dependent on purpose, like the requirement checks: the point
        // is that resolution and the probe agree, so a Claude Code that is
        // found is also one Beacon can ask about.
        if resolve_program("claude").is_none() {
            return;
        }

        let capabilities = capabilities();
        assert!(
            capabilities.version.is_some(),
            "claude was found but would not say its version"
        );
        assert!(
            capabilities.parsed_version.is_some(),
            "claude said {:?}, which is not a version",
            capabilities.version
        );
    }

    /// What `claude agents --json` printed on this machine during the audit.
    const LISTING: &str = r#"[
      {
        "pid": 15990,
        "cwd": "/Users/x/projects/app",
        "kind": "interactive",
        "startedAt": 1788242858772,
        "sessionId": "b57bf9d0-8020-4275-a060-a521d289beae",
        "name": "auth-refactor",
        "status": "busy"
      },
      {
        "pid": 19351,
        "cwd": "/Users/x/other",
        "kind": "interactive",
        "startedAt": 1788243333275,
        "sessionId": "580c0cdb-a978-4e8f-ac57-318dd88e7619",
        "name": "other-11"
      }
    ]"#;

    #[test]
    fn reads_which_sessions_are_running() {
        let sessions = parse_sessions(LISTING);
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            sessions[0].session_id,
            "b57bf9d0-8020-4275-a060-a521d289beae"
        );
        assert_eq!(sessions[0].name.as_deref(), Some("auth-refactor"));
        assert_eq!(sessions[0].status.as_deref(), Some("busy"));
        assert_eq!(sessions[0].pid, Some(15990));
    }

    #[test]
    fn a_session_without_a_status_is_still_a_session() {
        // The second entry has no `status`. Claude Code leaves it out, and a
        // listing that dropped the row would under-report what is running.
        let sessions = parse_sessions(LISTING);
        assert_eq!(sessions[1].status, None);
        assert_eq!(sessions[1].name.as_deref(), Some("other-11"));
    }

    #[test]
    fn one_entry_beacon_cannot_read_does_not_hide_the_rest() {
        // This listing is what "already open somewhere else" is decided from.
        // A guard that silently sees nothing would let Beacon open the same
        // conversation twice, which is the one thing it must not do.
        let mixed = r#"[
          { "nonsense": true },
          { "sessionId": "580c0cdb-a978-4e8f-ac57-318dd88e7619" },
          "not an object"
        ]"#;
        let sessions = parse_sessions(mixed);
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].session_id,
            "580c0cdb-a978-4e8f-ac57-318dd88e7619"
        );
    }

    #[test]
    fn a_listing_that_is_not_a_listing_is_empty_rather_than_a_panic() {
        for text in ["", "null", "{}", "not json", "[", "[[]]"] {
            assert!(parse_sessions(text).is_empty(), "parsed {text:?}");
        }
    }

    #[test]
    fn a_field_claude_code_stops_printing_costs_a_detail_not_the_row() {
        let minimal = r#"[{ "sessionId": "abc", "kind": "background" }]"#;
        let sessions = parse_sessions(minimal);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].kind.as_deref(), Some("background"));
        assert_eq!(sessions[0].cwd, None);
        assert_eq!(sessions[0].started_at, None);
    }

    #[test]
    fn what_is_running_here_can_actually_be_asked() {
        // Machine-dependent, like the requirement checks. This process is
        // itself a Claude session when the tests are run from one, so the
        // listing has at least one entry — but a bare `cargo test` is not, so
        // the assertion is about being able to ask, not about what comes back.
        if resolve_program("claude").is_none() || !capabilities().session_listing {
            return;
        }
        // A conversation id nothing could be using.
        assert_eq!(
            is_running("00000000-0000-4000-8000-000000000000"),
            Some(false)
        );
    }

    #[test]
    fn no_claude_means_no_capabilities_rather_than_a_panic() {
        let capabilities = Capabilities::none();
        assert!(capabilities.version.is_none());
        assert!(!capabilities.workstreams());
    }
}
