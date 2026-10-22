//! What the installed Codex can actually do.
//!
//! The sibling of [`crate::claude`], and deliberately a separate module rather
//! than a branch inside it: the two programs answer different questions with
//! different words, and a single probe that tried to serve both would describe
//! neither honestly.
//!
//! Capabilities are read out of what the program prints about itself, for the
//! reason set out in ADR-068: a table of version numbers is a list of guesses
//! about when each flag landed, wrong in a way nobody notices until someone's
//! Beacon quietly stops offering something.
//!
//! Codex is better at being asked than Claude Code is. Besides `--help` it has
//! `codex features list`, which prints every feature with its stage and
//! whether it is on — and some things are gated there rather than by the
//! presence of a flag, so both have to be read. See [`Capabilities::worktree`]
//! for the case that makes this more than a detail.

use std::time::Duration;

use serde::Serialize;

use crate::account::Account;
use crate::tools::{capture_briefly, resolve_program, strip_terminal_identity};

/// How long Codex gets to describe itself before Beacon stops waiting.
///
/// The same budget Claude Code gets, for the same reason: every one of these
/// runs on the way to something the user asked for, and an answer that arrives
/// after the session started is worth less than no answer at all.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// A parsed `major.minor.patch`.
///
/// Parsed by looking for the first token that is a version, rather than by
/// taking the first token as Claude Code's parser does: `codex --version`
/// prints `codex-cli 0.154.0`, so the leading token is the program's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CodexVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl CodexVersion {
    pub fn parse(text: &str) -> Option<Self> {
        text.split_whitespace().find_map(Self::parse_token)
    }

    fn parse_token(token: &str) -> Option<Self> {
        let mut parts = token.split('.');
        let mut number = || parts.next()?.parse::<u32>().ok();

        let parsed = Self {
            major: number()?,
            minor: number()?,
            // A two-part version is still a version.
            patch: number().unwrap_or(0),
        };
        // Anything left over means this was not a version, and taking it would
        // turn a path or a date into one.
        parts.next().is_none().then_some(parsed)
    }
}

impl std::fmt::Display for CodexVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// What this machine's Codex offers Beacon.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_version: Option<CodexVersion>,
    /// `--session-id <uuid>`: the caller choosing the conversation's id.
    ///
    /// Absent from every Codex published so far, and the reason Beacon has to
    /// learn a Codex session's id instead of assigning it. Probed anyway rather
    /// than written off: it is an open request upstream, and the day it lands
    /// Beacon should pick it up without anybody editing this file.
    pub assigned_session_id: bool,
    /// `--name`: naming a session as it starts. Also absent, which is why
    /// Beacon keeps its own name for a conversation rather than asking Codex to
    /// remember one.
    pub named_sessions: bool,
    /// `codex resume [SESSION_ID]`, a subcommand rather than a flag.
    pub resume: bool,
    /// `codex fork [SESSION_ID]`. Codex forks as a verb of its own, which
    /// Claude Code does not.
    pub fork: bool,
    /// `-C, --cd <DIR>`: which directory the session works in. What Beacon puts
    /// a worktree behind.
    pub working_dir: bool,
    /// `-s, --sandbox <MODE>`
    pub sandbox: bool,
    /// `--no-alt-screen`: the TUI without the alternate screen, keeping
    /// scrollback. An escape hatch if the full-screen TUI ever fights the
    /// terminal Beacon draws it in.
    pub inline_tui: bool,
    /// `-c, --config <key=value>`: one invocation's configuration, without
    /// touching the user's own.
    pub config_override: bool,
    /// Whether the hook system is there and switched on, read from
    /// `codex features list` because no flag announces it.
    ///
    /// Everything Beacon shows about a running Codex session hangs off this: a
    /// hook is how a session says it started, what it is doing, and when it is
    /// waiting for an answer. Without it a Codex session is a terminal and
    /// nothing more.
    pub hooks: bool,
    /// Whether Codex can be asked to make its own git worktree.
    ///
    /// Both halves are required, and that is the point. `--worktree` is in the
    /// help of a Codex whose `features list` says `worktrees experimental
    /// false` — the flag is there and the feature behind it is off. Reading
    /// only the help would report a capability that does nothing when used.
    ///
    /// Beacon does not need it either way: it makes worktrees itself and passes
    /// `--cd`, so that both agents get the same directories by the same
    /// mechanism rather than each CLI's own, with its own maturity.
    pub worktree: bool,
}

impl Capabilities {
    /// Nothing found. Every feature that needs Codex hides itself.
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether a Codex session can be a named, resumable conversation at all.
    ///
    /// Not the same three flags Claude Code needs, because the shape of the
    /// problem is different. Beacon cannot hand Codex an id, so it has to be
    /// told one — and in a session drawn in a terminal, a hook is the only
    /// thing that ever says it. No hooks, no id; no id, nothing to resume *by*,
    /// however willing `codex resume` is.
    ///
    /// Naming is missing from the list on purpose: Codex has no `--name`, so
    /// the name is Beacon's own and always available.
    pub fn workstreams(&self) -> bool {
        self.resume && (self.hooks || self.assigned_session_id)
    }
}

/// Reads the capabilities out of what Codex prints about itself.
///
/// Pure, and given the texts rather than running anything, so the parsing can
/// be tested without a Codex on the machine running the tests.
pub fn interpret(version: Option<&str>, help: &str, features: &str) -> Capabilities {
    // Matched with the leading dashes and, where the flag takes one, the
    // following delimiter. `--cd` and `--cdx` are different flags, and a bare
    // substring search cannot tell them apart.
    let has = |flag: &str| {
        help.match_indices(flag).any(|(at, _)| {
            let after = help[at + flag.len()..].chars().next();
            matches!(
                after,
                None | Some(' ') | Some('\n') | Some(',') | Some('<') | Some('[')
            )
        })
    };
    // Subcommands are listed indented under `Commands:`, so they are matched
    // there rather than anywhere the word appears: `resume` turns up in the
    // prose of other entries too.
    let has_command = |name: &str| help.contains(&format!("\n  {name} "));

    Capabilities {
        version: version.map(str::to_string),
        parsed_version: version.and_then(CodexVersion::parse),
        assigned_session_id: has("--session-id"),
        named_sessions: has("--name"),
        resume: has_command("resume"),
        fork: has_command("fork"),
        working_dir: has("--cd"),
        sandbox: has("--sandbox"),
        inline_tui: has("--no-alt-screen"),
        config_override: has("--config"),
        hooks: feature_enabled(features, "hooks"),
        // See the field's own note: the flag alone is not the answer.
        worktree: has("--worktree") && feature_enabled(features, "worktrees"),
    }
}

/// Whether `codex features list` says a feature is on.
///
/// The listing is one feature per line — name, stage, then `true` or `false`.
/// Only the last column is read: a feature can be `experimental` and on, or
/// `stable` and off, and what Beacon needs to know is whether using it would
/// do anything.
pub fn feature_enabled(features: &str, name: &str) -> bool {
    features.lines().any(|line| {
        let mut fields = line.split_whitespace();
        fields.next() == Some(name) && fields.next_back() == Some("true")
    })
}

/// What this machine's Codex can do, worked out once and then remembered.
///
/// It costs two short processes, and it is read on the way to starting a session, which is
/// not a place to spend a fork. What it cannot do is survive the user
/// upgrading or installing Codex underneath a running Beacon — so
/// [`forget_capabilities`] exists, and the one thing that calls it is somebody
/// asking Beacon to look at their machine again.
///
/// The answer is leaked rather than handed out by value, so that every caller
/// can keep taking a `&'static` and none of them had to change. One probe is a
/// few hundred bytes, and a person presses that button a handful of times in
/// the life of a window.
pub fn capabilities() -> &'static Capabilities {
    let mut held = cached().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(known) = *held {
        return known;
    }
    let found: &'static Capabilities = Box::leak(Box::new(detect()));
    *held = Some(found);
    found
}

fn cached() -> &'static std::sync::Mutex<Option<&'static Capabilities>> {
    static CACHED: std::sync::Mutex<Option<&'static Capabilities>> = std::sync::Mutex::new(None);
    &CACHED
}

/// Forgets them, so the next question is put to the program as it is now.
pub fn forget_capabilities() {
    *cached().lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn detect() -> Capabilities {
    let Some(path) = resolve_program("codex") else {
        return Capabilities::none();
    };

    let version = ask(&path, &["--version"]);
    let help = ask(&path, &["--help"]).unwrap_or_default();
    // Skipped when the subcommand is not there to ask. An older Codex without
    // it reports no features, which reads as "off" — the safe direction: a
    // feature Beacon does not use costs a hidden button, and one it uses when
    // it should not costs a broken session.
    let features = if help.contains("\n  features ") {
        ask(&path, &["features", "list"]).unwrap_or_default()
    } else {
        String::new()
    };

    interpret(version.as_deref(), &help, &features)
}

/// Whether somebody has signed in to Codex, as Codex answers it.
///
/// `codex login status`, which is the question the user could type. Beacon
/// never touches the credential — the same rule as Claude Code's (ADR-052) —
/// and this does not either: it asks, and reads what comes back.
///
/// Deliberately one-directional, for the reason in [`parse_login_status`], and
/// `None` is no answer: a Codex too old for the subcommand, or one that would
/// not finish.
pub fn signed_in(codex: &std::path::Path, home: Option<&std::path::Path>) -> Option<bool> {
    account(codex, home)?.signed_in
}

/// Which account Codex is using, as far as Codex will say.
///
/// Which is not far. `codex login status` prints one line — "Logged in using
/// ChatGPT" — and there is no `--json` and no account name anywhere in the
/// CLI: `codex doctor` adds how the credential is stored and no more. So this
/// carries the method and never an email.
///
/// Decoding the identity token in `auth.json` would produce one, and that is
/// exactly the line Beacon does not cross (ADR-052): it is a credential file,
/// and a program that opens it to read a name is a program that opens it.
/// Naming a Codex account is the user's job, and in the account menu it is
/// their own label that does it.
///
/// That one line goes to **standard error**, with nothing on standard output,
/// which is why the whole of what it prints is read and not just stdout.
pub fn account(codex: &std::path::Path, home: Option<&std::path::Path>) -> Option<Account> {
    let mut command = std::process::Command::new(codex);
    command.args(["login", "status"]);
    crate::tools::strip_terminal_identity(&mut command);
    command.env("PATH", crate::tools::session_path(codex));
    // The account a session would be started with, for the reason Claude
    // Code's is: otherwise this answers for whichever one the machine is
    // signed in to, under another account's name.
    if let Some(home) = home {
        command.env(
            crate::account::home_variable(crate::agent::AgentKind::Codex),
            home,
        );
    }

    let (succeeded, text) = crate::tools::outcome_briefly(&mut command, PROBE_TIMEOUT)?;
    Some(parse_login_status(&text, succeeded))
}

/// What Codex's one line of login status says.
///
/// Deliberately one-directional about signing in, which is the rule
/// [`signed_in`] has always followed: a non-zero exit, or a line that says so
/// in words, is read as not signed in; a zero exit is read as nothing at all.
/// If Codex turns out to exit zero either way, Beacon simply never says Codex
/// needs signing in — which is where it was before any of this existed. The
/// other way round would be telling somebody who is signed in that they are
/// not, which is worse than saying nothing.
///
/// The method is read case-insensitively and reported with Codex's own
/// capitals, since "ChatGPT" is a name and not a word.
///
/// Every line is looked at rather than the first one, because the first one is
/// not reliably Codex's. An npm-installed Codex is reached through a shim, and
/// a Node deprecation warning or a `punycode` notice lands on the same stream
/// ahead of the answer — this command puts nothing on standard output at all,
/// so there is nothing to tell them apart by position.
pub fn parse_login_status(text: &str, succeeded: bool) -> Account {
    const USING: &str = "logged in using ";
    const NOT: &str = "not logged in";

    // Byte-for-byte with the line it came from, so an offset found in one
    // indexes the other, and always on a character boundary because the
    // needle is ASCII. `to_ascii_lowercase` is the reason: the Unicode one
    // can change a string's length.
    let said = |needle: &str| {
        text.lines().map(str::trim).find_map(|line| {
            let at = line.to_ascii_lowercase().find(needle)?;
            Some(&line[at + needle.len()..])
        })
    };

    // Cut at " - ", because Codex puts a masked key after it: the strings in
    // the binary are "Logged in using ChatGPT", "Logged in using access
    // token", "Logged in using workload identity", and
    // "Logged in using an API key - " with the key appended. Part of a
    // credential is still a credential, and a header is not where it goes.
    let method = said(USING).map(|rest| {
        rest.split(" - ")
            .next()
            .unwrap_or_default()
            .trim()
            .trim_end_matches('.')
            .to_string()
    });

    Account {
        signed_in: (!succeeded || said(NOT).is_some()).then_some(false),
        email: None,
        // An empty tail — "Logged in using" and nothing after it — names no
        // method, and a chip reading nothing at all is not a chip.
        method: method.filter(|found| !found.is_empty()),
        plan: None,
    }
}

/// Runs Codex for its own description of itself.
///
/// Best effort throughout: a Codex that will not answer is treated as one that
/// offers nothing, which hides features rather than breaking them.
fn ask(path: &std::path::Path, args: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(path);
    command.args(args);
    strip_terminal_identity(&mut command);

    let text = capture_briefly(&mut command, PROBE_TIMEOUT)?
        .trim()
        .to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from the real `codex --help` of codex-cli 0.154.0, keeping the
    /// lines verbatim so the parser is tested against what it will really see.
    const HELP: &str = "\
Usage: codex [OPTIONS] [PROMPT]
       codex [OPTIONS] <COMMAND> [ARGS]

Commands:
  exec              Run Codex non-interactively [aliases: e]
  resume            Resume a previous interactive session (picker by default; use --last to continue
                    the most recent)
  fork              Fork a previous interactive session (picker by default; use --last to fork the
                    most recent)
  features          Inspect feature flags

Options:
  -c, --config <key=value>
          Override a configuration value
  -s, --sandbox <SANDBOX_MODE>
          Select the sandbox policy to use
  -C, --cd <DIR>
          Tell the agent to use the specified directory as its working root
      --worktree
          Run the session in a new managed Git worktree
      --no-alt-screen
          Disable alternate screen mode
";

    /// Three lines from the real `codex features list`, including the one that
    /// matters most: a flag in the help whose feature is switched off.
    const FEATURES: &str = "\
apps                                     stable             true
hooks                                    stable             true
worktrees                                experimental       false
";

    fn real() -> Capabilities {
        interpret(Some("codex-cli 0.154.0"), HELP, FEATURES)
    }

    #[test]
    fn the_version_is_read_past_the_programs_own_name() {
        // `codex --version` prints `codex-cli 0.154.0`, so a parser that takes
        // the first token finds a name where it wants a number.
        assert_eq!(
            real().parsed_version,
            Some(CodexVersion {
                major: 0,
                minor: 154,
                patch: 0
            })
        );
    }

    #[test]
    fn something_that_is_not_a_version_is_not_read_as_one() {
        assert_eq!(CodexVersion::parse("codex-cli"), None);
        assert_eq!(CodexVersion::parse("/Users/eya/.nvm/bin"), None);
        assert_eq!(CodexVersion::parse("2026.09.16.1"), None);
        // Two parts is still a version.
        assert_eq!(
            CodexVersion::parse("1.2"),
            Some(CodexVersion {
                major: 1,
                minor: 2,
                patch: 0
            })
        );
    }

    #[test]
    fn subcommands_are_found_where_subcommands_are_listed() {
        let capabilities = real();
        assert!(capabilities.resume);
        assert!(capabilities.fork);
    }

    #[test]
    fn a_word_in_someone_elses_description_is_not_a_subcommand() {
        // Codex describes its own commands in prose that names others: the
        // entry for `resume` says "use --last to continue the most recent".
        // Matching the word anywhere would invent commands out of sentences.
        let prose = "\
Usage: codex [OPTIONS] [PROMPT]

Commands:
  exec              Run Codex non-interactively. To fork or resume an earlier
                    session, see the other commands.
";
        let capabilities = interpret(None, prose, FEATURES);
        assert!(
            !capabilities.resume,
            "`resume` appears only in a description here"
        );
        assert!(!capabilities.fork, "and so does `fork`");
    }

    #[test]
    fn the_flags_beacon_leans_on_are_recognised() {
        let capabilities = real();
        assert!(capabilities.working_dir, "--cd is how a worktree is used");
        assert!(capabilities.sandbox);
        assert!(capabilities.inline_tui);
        assert!(capabilities.config_override);
    }

    #[test]
    fn what_codex_cannot_do_today_is_reported_as_absent_rather_than_assumed() {
        let capabilities = real();
        // The two that shape the whole integration: Beacon cannot choose a
        // Codex conversation's id, and cannot name it as it starts.
        assert!(!capabilities.assigned_session_id);
        assert!(!capabilities.named_sessions);
    }

    #[test]
    fn a_flag_whose_feature_is_switched_off_is_not_a_capability() {
        // The real 0.154.0: `--worktree` in the help, `worktrees experimental
        // false` in the features. Reading only the help would offer the user
        // something that does nothing.
        assert!(
            HELP.contains("--worktree"),
            "the flag really is in the help"
        );
        assert!(!real().worktree);

        let switched_on = "worktrees                                experimental       true\n";
        assert!(interpret(None, HELP, switched_on).worktree);
    }

    #[test]
    fn hooks_are_read_from_the_features_because_no_flag_announces_them() {
        assert!(real().hooks);
        assert!(!interpret(None, HELP, "hooks    stable    false\n").hooks);
        // An older Codex with no `features` subcommand reports nothing, which
        // has to read as off rather than as on.
        assert!(!interpret(None, HELP, "").hooks);
    }

    #[test]
    fn a_conversation_needs_both_a_way_back_and_a_way_to_learn_its_id() {
        assert!(real().workstreams(), "resume plus hooks is enough");

        // Hooks off: the id is never reported, so there is nothing to resume by.
        let no_hooks = interpret(Some("codex-cli 0.154.0"), HELP, "hooks  stable  false\n");
        assert!(no_hooks.resume);
        assert!(!no_hooks.workstreams());

        // And nothing at all offers nothing.
        assert!(!Capabilities::none().workstreams());
    }

    #[test]
    fn an_empty_help_is_a_codex_that_offers_nothing() {
        let capabilities = interpret(None, "", "");
        assert_eq!(capabilities, Capabilities::none());
        assert!(!capabilities.workstreams());
    }

    /// Every way Codex can answer, taken out of the shipped binary's own
    /// strings rather than guessed at.
    #[test]
    fn a_masked_key_in_the_answer_never_reaches_the_screen() {
        // This is the one that matters: Codex appends the key to this line.
        let keyed = parse_login_status("Logged in using an API key - sk-proj-abc***WXYZ", true);
        assert_eq!(keyed.method.as_deref(), Some("an API key"));

        // And the rest of the list, verbatim, so a new one is noticed by this
        // test failing rather than by a key appearing in a header.
        for (printed, expected) in [
            ("Logged in using ChatGPT", "ChatGPT"),
            ("Logged in using access token", "access token"),
            (
                "Logged in using personal access token",
                "personal access token",
            ),
            ("Logged in using workload identity", "workload identity"),
            (
                "Logged in using Amazon Bedrock API key",
                "Amazon Bedrock API key",
            ),
            (
                "Logged in using Amazon Bedrock AWS access keys",
                "Amazon Bedrock AWS access keys",
            ),
        ] {
            assert_eq!(
                parse_login_status(printed, true).method.as_deref(),
                Some(expected),
                "{printed}"
            );
        }
    }

    #[test]
    fn codex_names_how_it_signed_in_and_never_who() {
        // The whole of what `codex login status` prints, on a machine signed
        // in with a ChatGPT plan. There is no email in it, and none anywhere
        // else in the CLI.
        let account = parse_login_status("Logged in using ChatGPT\n", true);
        assert_eq!(account.method.as_deref(), Some("ChatGPT"));
        assert_eq!(account.email, None);
        assert!(account.worth_showing());

        // A zero exit is not read as signed in, and never has been.
        assert_eq!(account.signed_in, None);
    }

    #[test]
    fn codex_is_only_ever_reported_as_signed_out() {
        // The exit code is one way of saying it.
        assert_eq!(parse_login_status("", false).signed_in, Some(false));
        // And the line is the other, for a Codex that says so and exits zero.
        assert_eq!(
            parse_login_status("Not logged in\n", true).signed_in,
            Some(false)
        );
        // "Not logged in" is not a method, however it is capitalised.
        assert_eq!(parse_login_status("not logged in", true).method, None);
    }

    #[test]
    fn a_warning_in_front_of_the_answer_does_not_become_the_answer() {
        // Codex is reached through an npm shim, its answer is on standard
        // error, and so is everything Node feels like saying first.
        let noisy = "\
(node:51234) [DEP0040] DeprecationWarning: The `punycode` module is deprecated.
(Use `node --trace-deprecation ...` to show where the warning was created)
Logged in using ChatGPT
";
        assert_eq!(
            parse_login_status(noisy, true).method.as_deref(),
            Some("ChatGPT")
        );

        // And the same for being told nobody is signed in.
        assert_eq!(
            parse_login_status("(node:51234) DeprecationWarning\nNot logged in\n", true).signed_in,
            Some(false)
        );
    }

    /// Byte offsets are found in a lowercased copy and used on the original.
    #[test]
    fn an_answer_with_characters_outside_ascii_does_not_panic() {
        // A line whose bytes outnumber its characters, on both sides of the
        // phrase being looked for. Indexing a `str` off a boundary panics,
        // and this is the shape that would do it.
        let account = parse_login_status("Señor — logged in using ChatGPT · año", true);
        assert_eq!(account.method.as_deref(), Some("ChatGPT · año"));
    }

    #[test]
    fn a_login_status_nobody_recognises_asserts_nothing() {
        // A Codex too old for the subcommand, or a wording that changed. The
        // chip draws nothing rather than a blank badge.
        let quiet = parse_login_status("", true);
        assert_eq!(quiet, Account::default());
        assert!(!quiet.worth_showing());
    }
}
