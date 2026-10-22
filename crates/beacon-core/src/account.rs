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

use serde::{Deserialize, Serialize};

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
}
