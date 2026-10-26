//! Which account an agent is signed in to.
//!
//! Beacon never touches a credential (ADR-052) and this does not change that:
//! every field here is something the agent itself printed when asked the
//! question its user could have typed. Nothing reads a token file, a keychain
//! entry or a refresh token, and nothing here is stored.
//!
//! The two agents answer very differently. `claude auth status --json` gives
//! the account's email, how it signed in and which plan it is on. Codex has
//! `codex login status`, which prints one line — "Logged in using ChatGPT" —
//! and no machine-readable form of it, so for Codex this carries the method
//! and nothing else. That asymmetry is the tools', not Beacon's, and the shape
//! here keeps it visible rather than inventing a Codex email by decoding the
//! token Beacon has no business opening.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::AgentKind;
use crate::domain::AccountId;

/// What an agent says about the account it is using, as far as it says it.
///
/// Every field optional, and that is the design: this is another program's
/// output, and a field that stops being printed should cost a line in a chip
/// rather than the whole answer.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Whether anybody is signed in.
    ///
    /// `None` means the question had no answer — a version too old to have the
    /// subcommand, one that would not finish, or output this does not
    /// recognise. Callers say nothing rather than guess, which is why this is
    /// not a `bool`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_in: Option<bool>,
    /// The account, where the agent names it. Claude Code does; Codex does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// How the sign-in was done, in the agent's own words: `claude.ai`,
    /// `ChatGPT`, an API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// The plan, where the agent says: `max`, `pro`, `team`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

impl Account {
    /// Whether this says anything worth putting on screen.
    ///
    /// An account nobody can name and no state to report is not a chip, it is
    /// a blank space with a border. The caller draws nothing instead.
    pub fn worth_showing(&self) -> bool {
        self.email.is_some() || self.method.is_some() || self.signed_in == Some(false)
    }
}

/// One account somebody has added, which is a configuration home.
///
/// Not a credential, not a token, not a login: a directory. The agent's own
/// variable — `CLAUDE_CONFIG_DIR`, `CODEX_HOME` — points it there, the agent's
/// own `login` command fills it, and Beacon's only part is deciding which
/// directory a session is started with (ADR-099).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: AccountId,
    /// What the person calls it. For Codex this is the only name there is.
    pub label: String,
    /// Where the agent keeps everything for this account.
    ///
    /// Stored rather than derived from the id, so a profile can name a
    /// directory somebody already has — `~/.claude-work` made by hand long
    /// before Beacon offered this.
    pub home: PathBuf,
}

/// One agent's accounts, and which of them is in use.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccounts {
    /// The ones that were added. Never includes the machine's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<Profile>,
    /// `None` is the machine's own account — `~/.claude`, `~/.codex`, with no
    /// variable set at all.
    ///
    /// That is the default and it is load-bearing: somebody who never opens
    /// the account menu runs exactly as they did before this existed, and
    /// Beacon sets nothing in their environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<AccountId>,
}

impl AgentAccounts {
    /// The active profile, or `None` for the machine's own.
    ///
    /// An `active` naming a profile that is no longer there reads as the
    /// machine's own rather than as an error: the list is the truth, and a
    /// dangling name is how a hand-edited settings file arrives.
    pub fn current(&self) -> Option<&Profile> {
        self.get(self.active.as_ref()?)
    }

    pub fn get(&self, id: &AccountId) -> Option<&Profile> {
        self.added.iter().find(|profile| &profile.id == id)
    }
}

/// Every account Beacon knows about, per agent.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBook {
    #[serde(default, skip_serializing_if = "is_default")]
    pub claude: AgentAccounts,
    #[serde(default, skip_serializing_if = "is_default")]
    pub codex: AgentAccounts,
}

fn is_default(accounts: &AgentAccounts) -> bool {
    accounts == &AgentAccounts::default()
}

impl AccountBook {
    pub fn of(&self, agent: AgentKind) -> &AgentAccounts {
        match agent {
            AgentKind::Claude => &self.claude,
            AgentKind::Codex => &self.codex,
        }
    }

    pub fn of_mut(&mut self, agent: AgentKind) -> &mut AgentAccounts {
        match agent {
            AgentKind::Claude => &mut self.claude,
            AgentKind::Codex => &mut self.codex,
        }
    }

    /// Every configuration home an agent could be started with on this
    /// machine: its own, and then each account that was added.
    ///
    /// Anything Beacon installs into another application's configuration has
    /// to go into all of them. The hooks are the case that matters: an account
    /// without them has no amber tab, no "waiting for you", and no way to know
    /// a conversation exists to resume — so switching account would quietly
    /// turn off most of what Beacon is for (ADR-099).
    pub fn every_home(&self, agent: AgentKind) -> Vec<PathBuf> {
        let mut homes = vec![home_or_default(agent, None)];
        for profile in &self.of(agent).added {
            // A profile may name any directory, including one another profile
            // already names, and installing into the same place twice is at
            // best wasted work. Not sorted to dedupe: the machine's own comes
            // first on purpose.
            if !homes.contains(&profile.home) {
                homes.push(profile.home.clone());
            }
        }
        homes
    }

    /// The homes a session should be started with.
    pub fn homes(&self) -> AccountHomes {
        AccountHomes {
            claude: self.claude.current().map(|found| found.home.clone()),
            codex: self.codex.current().map(|found| found.home.clone()),
        }
    }
}

/// Which configuration home each agent should use, where it is not the
/// machine's own.
///
/// Sent by the client with every session rather than read by the daemon, like
/// the shell and Beacon's subagents and for the same reason: a session starts
/// with the account chosen now, not the one chosen when the daemon happened to
/// start.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountHomes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<PathBuf>,
}

impl AccountHomes {
    pub fn of(&self, agent: AgentKind) -> Option<&Path> {
        match agent {
            AgentKind::Claude => self.claude.as_deref(),
            AgentKind::Codex => self.codex.as_deref(),
        }
    }
}

/// The file an agent keeps its preferences in, inside its configuration home.
///
/// Its preferences and not its credential: those are separate files in both
/// agents, which is what makes carrying one over without the other a line
/// Beacon can hold.
pub fn settings_file(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "settings.json",
        AgentKind::Codex => "config.toml",
    }
}

/// Beacon's own entries in Claude Code's settings, which a new account should
/// not inherit by copy.
///
/// They are installed rather than copied, by the code that knows what this
/// build's hook command is — a copied one would name the right path today and
/// the wrong one after Beacon moved.
const BEACONS_OWN: [&str; 2] = ["hooks", "statusLine"];

/// What to write into a new account's settings, given the ones the machine's
/// own account has.
///
/// Everything the person had, less the entries Beacon manages itself. Not a
/// chosen list of preferences to keep: that would be a table of guesses about
/// another program's schema, wrong the moment Claude Code adds a key, which is
/// the thing ADR-068 exists to refuse. The file is theirs; it comes across as
/// theirs.
///
/// This is a correction. Beacon used to copy none of it, reasoning that those
/// files name paths and hold keys — true of the MCP servers, which live
/// elsewhere, and false of the preferences, which is what the file mostly is.
/// What that cost was concrete: `remoteControlAtStartup` turned off on
/// purpose, a second account added, and Claude Code starting with remote
/// control on because the new home had never heard of the preference.
///
/// `None` when there is nothing worth writing — no settings of their own, or a
/// file this cannot read. An unreadable one is left alone rather than
/// rewritten: it is still theirs, and the account works without it.
pub fn settings_for_a_new_account(own: &str) -> Option<String> {
    let mut settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(own).ok()?;
    for key in BEACONS_OWN {
        settings.remove(key);
    }
    if settings.is_empty() {
        return None;
    }
    serde_json::to_string_pretty(&settings).ok()
}

/// The variable that tells an agent where its configuration lives.
///
/// The agents' own, documented and supported. Beacon invents nothing here.
pub fn home_variable(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "CLAUDE_CONFIG_DIR",
        AgentKind::Codex => "CODEX_HOME",
    }
}

/// Where the agent keeps everything, for a session using this home or none.
///
/// Without a chosen account this is what the agent itself would use, which
/// means the variable in Beacon's own environment comes first: somebody who
/// launched Beacon with `CODEX_HOME` set already has one account somewhere
/// else, and "the machine's own" is theirs, not `~/.codex`. Installing into
/// `~/.codex` for them would put Beacon's reporting in a directory no session
/// ever reads.
pub fn home_or_default(agent: AgentKind, home: Option<&Path>) -> PathBuf {
    if let Some(home) = home {
        return home.to_path_buf();
    }
    if let Some(set) = std::env::var_os(home_variable(agent)) {
        let set = PathBuf::from(set);
        if !set.as_os_str().is_empty() {
            return set;
        }
    }
    agent_default_home(agent)
}

/// Where the agent keeps everything when nothing says otherwise.
///
/// Separate from [`home_or_default`] because it depends on nothing but the
/// agent: the variable above is process state, and a test that asserted on it
/// would pass or fail by what the shell running it happened to export.
pub fn agent_default_home(agent: AgentKind) -> PathBuf {
    match agent {
        AgentKind::Claude => crate::paths::home_dir().join(".claude"),
        AgentKind::Codex => crate::paths::home_dir().join(".codex"),
    }
}

/// Where Beacon puts a home it creates itself.
///
/// Under Beacon's own configuration directory, because Beacon made it and
/// Beacon should be the thing that can tidy it up. A profile pointing at a
/// directory somebody else made keeps whatever path they gave it.
pub fn home_for(agent: AgentKind, id: &AccountId) -> PathBuf {
    crate::paths::default_config_dir()
        .join("accounts")
        .join(agent.as_str())
        .join(id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window reads these names, so they are as much a contract as a
    /// function signature — and a chip that silently shows nothing is what a
    /// renamed field looks like from the outside.
    #[test]
    fn it_reaches_the_window_under_the_names_the_window_knows() {
        let json = serde_json::to_string(&Account {
            signed_in: Some(true),
            email: Some("someone@example.com".to_string()),
            method: Some("claude.ai".to_string()),
            plan: Some("max".to_string()),
        })
        .unwrap();

        assert_eq!(
            json,
            r#"{"signedIn":true,"email":"someone@example.com","method":"claude.ai","plan":"max"}"#
        );
    }

    /// Absent and not null, because `signedIn?: boolean` in the window means
    /// absent, and "nobody answered" must not read as "nobody is signed in".
    #[test]
    fn what_was_never_said_is_left_out_rather_than_sent_as_nothing() {
        assert_eq!(serde_json::to_string(&Account::default()).unwrap(), "{}");
        assert!(!Account::default().worth_showing());
    }

    fn profile(id: &str) -> Profile {
        Profile {
            id: AccountId(id.to_string()),
            label: id.to_string(),
            home: PathBuf::from("/tmp").join(id),
        }
    }

    #[test]
    fn nobody_who_added_nothing_has_anything_in_their_environment() {
        // The whole reason the default is "no variable at all": an account
        // menu nobody opened must leave the machine exactly as it was.
        let book = AccountBook::default();

        assert_eq!(book.homes(), AccountHomes::default());
        assert_eq!(book.homes().of(AgentKind::Claude), None);
        assert_eq!(serde_json::to_string(&book).unwrap(), "{}");
    }

    #[test]
    fn the_active_account_is_the_one_a_session_is_started_with() {
        let mut book = AccountBook::default();
        book.claude.added.push(profile("work"));
        book.claude.active = Some(AccountId("work".to_string()));

        assert_eq!(
            book.homes().of(AgentKind::Claude),
            Some(Path::new("/tmp/work"))
        );
        // The other agent is untouched by a choice made for this one.
        assert_eq!(book.homes().of(AgentKind::Codex), None);
    }

    #[test]
    fn an_account_that_is_no_longer_there_is_the_machines_own() {
        // How a hand-edited settings file arrives, and how a removed account
        // leaves things if anything ever forgets to clear `active`. Falling
        // back to the machine's own is the only harmless answer: it is where
        // everybody started.
        let mut book = AccountBook::default();
        book.codex.active = Some(AccountId("gone".to_string()));

        assert_eq!(book.codex.current(), None);
        assert_eq!(book.homes().of(AgentKind::Codex), None);
    }

    #[test]
    fn a_home_is_the_agents_own_default_when_nobody_chose_one() {
        assert!(
            agent_default_home(AgentKind::Claude).ends_with(".claude"),
            "{:?}",
            agent_default_home(AgentKind::Claude)
        );
        assert!(
            agent_default_home(AgentKind::Codex).ends_with(".codex"),
            "{:?}",
            agent_default_home(AgentKind::Codex)
        );

        // And the chosen one, untouched, when there is one — which is the
        // whole of what this decides. Whether an unset choice falls back to
        // the variable or to the default above is process state, and a test
        // that set it would race every other test in this binary.
        let chosen = Path::new("/tmp/elsewhere");
        assert_eq!(home_or_default(AgentKind::Claude, Some(chosen)), chosen);
    }

    #[test]
    fn what_beacon_installs_goes_into_every_home_including_the_machines_own() {
        let mut book = AccountBook::default();
        book.claude.added.push(profile("work"));
        book.claude.added.push(profile("spare"));

        let homes = book.every_home(AgentKind::Claude);
        // The machine's own comes first and is never left out: it is where
        // somebody who never opens the menu has their hooks. Whatever that is
        // on this machine — the variable may already be set by hand.
        assert_eq!(homes[0], home_or_default(AgentKind::Claude, None));
        assert!(homes.contains(&PathBuf::from("/tmp/work")));
        assert!(homes.contains(&PathBuf::from("/tmp/spare")));
        assert_eq!(homes.len(), 3);

        // And an agent with no accounts has exactly one place to install into.
        assert_eq!(book.every_home(AgentKind::Codex).len(), 1);
    }

    /// The preference that found this: turned off on purpose in one account,
    /// and on again in the next because the new home had never heard of it.
    #[test]
    fn a_new_account_starts_with_the_preferences_the_machine_already_had() {
        let own = r#"{
            "remoteControlAtStartup": false,
            "theme": "dark",
            "model": "opus",
            "permissions": { "allow": ["Bash(git *)"] },
            "hooks": { "Stop": [] },
            "statusLine": { "type": "command", "command": "beacon statusline" }
        }"#;

        let written = settings_for_a_new_account(own).expect("there is plenty worth writing");
        let back: serde_json::Value = serde_json::from_str(&written).unwrap();

        assert_eq!(back["remoteControlAtStartup"], false);
        assert_eq!(back["theme"], "dark");
        assert_eq!(back["model"], "opus");
        assert!(
            back["permissions"].is_object(),
            "permissions are theirs too"
        );

        // Beacon's own entries are installed, not copied: a copied hook names
        // the path this build has today and the wrong one after Beacon moves.
        assert!(back.get("hooks").is_none());
        assert!(back.get("statusLine").is_none());
    }

    #[test]
    fn nothing_of_their_own_means_nothing_to_write() {
        // Somebody whose settings hold only what Beacon put there gets a new
        // account with no settings file at all, rather than an empty object.
        assert_eq!(
            settings_for_a_new_account(r#"{"hooks": {}, "statusLine": {}}"#),
            None
        );
        assert_eq!(settings_for_a_new_account("{}"), None);

        // And a file this cannot read is left where it is. It is still theirs,
        // and the account works without it.
        assert_eq!(settings_for_a_new_account("not json at all"), None);
        assert_eq!(settings_for_a_new_account("[1, 2, 3]"), None);
    }

    #[test]
    fn each_agent_keeps_its_preferences_where_it_keeps_them() {
        assert_eq!(settings_file(AgentKind::Claude), "settings.json");
        assert_eq!(settings_file(AgentKind::Codex), "config.toml");
    }

    /// These are the agents' own variables. Beacon inventing a name here
    /// would be Beacon inventing a feature neither agent has.
    #[test]
    fn the_variables_are_the_ones_the_agents_document() {
        assert_eq!(home_variable(AgentKind::Claude), "CLAUDE_CONFIG_DIR");
        assert_eq!(home_variable(AgentKind::Codex), "CODEX_HOME");
    }
}
