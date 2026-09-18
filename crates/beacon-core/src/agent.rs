//! Which agent a conversation belongs to.
//!
//! Beacon drives more than one coding agent, and they are not
//! interchangeable: they are told to start in different words, they answer
//! different questions about themselves, and one of them will not be told what
//! to call a conversation. This is the small amount of that difference the rest
//! of the codebase has to know about.
//!
//! What is *not* here is anything about how to build a command line. That
//! belongs next to the spawn, where the launch is assembled; putting it here
//! would make every caller of `AgentKind` depend on the details of both
//! programs.

use serde::{Deserialize, Serialize};

/// A coding agent Beacon can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    Claude,
    Codex,
}

impl Default for AgentKind {
    /// Claude, because that is what every conversation recorded before agents
    /// were a choice actually is.
    ///
    /// This is the whole migration for the stored book: the field arrives with
    /// a default and an old file reads correctly rather than needing a schema
    /// version, a rewrite, or a guess.
    fn default() -> Self {
        Self::Claude
    }
}

impl AgentKind {
    /// Every agent, in the order a settings screen should offer them.
    pub const ALL: [AgentKind; 2] = [AgentKind::Claude, AgentKind::Codex];

    /// The program to run, as it is called on the PATH.
    pub fn program(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
    }

    /// How it is spelled in an environment variable or on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
    }

    /// Read back from that spelling. Anything unrecognised is nothing, so a
    /// hook from a future agent is ignored rather than filed as Claude's.
    pub fn parse(text: &str) -> Option<Self> {
        AgentKind::ALL.into_iter().find(|a| a.as_str() == text)
    }

    /// What to call it where a person will read it.
    ///
    /// Their own names for themselves, not Beacon's: somebody who installed
    /// Claude Code should be told that Claude Code is missing.
    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude Code",
            AgentKind::Codex => "Codex",
        }
    }

    /// Whether this agent accepts an id chosen by its caller.
    ///
    /// The difference that shapes everything else. Claude Code takes
    /// `--session-id` and is therefore addressable before it exists; Codex
    /// generates its own and reports it afterwards, so a conversation with it
    /// has a period where Beacon has a name for it and no id.
    ///
    /// Asked of the installed program rather than stated here, because it is a
    /// fact about the build on this machine and not about the product: Codex
    /// has open requests for the flag, and the day one lands this should start
    /// answering yes without anybody editing this file.
    pub fn takes_assigned_id(self) -> bool {
        match self {
            AgentKind::Claude => crate::claude::capabilities().assigned_session_id,
            AgentKind::Codex => crate::codex::capabilities().assigned_session_id,
        }
    }

    /// Whether this agent can hold named, resumable conversations at all.
    ///
    /// Each program is asked in its own terms — the two need different things
    /// to be true — and a no here hides the feature rather than breaking it.
    pub fn workstreams(self) -> bool {
        match self {
            AgentKind::Claude => crate::claude::capabilities().workstreams(),
            AgentKind::Codex => crate::codex::capabilities().workstreams(),
        }
    }
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_book_reads_as_claude() {
        // Every conversation stored before this field existed belongs to Claude
        // Code, so the default has to be Claude and not merely the first
        // variant that happened to be written down.
        assert_eq!(AgentKind::default(), AgentKind::Claude);

        #[derive(Deserialize)]
        struct Row {
            #[serde(default)]
            agent: AgentKind,
        }
        let old: Row = serde_json::from_str("{}").unwrap();
        assert_eq!(old.agent, AgentKind::Claude);
    }

    #[test]
    fn each_agent_survives_a_round_trip_as_the_book_stores_it() {
        for agent in AgentKind::ALL {
            let line = serde_json::to_string(&agent).unwrap();
            assert_eq!(serde_json::from_str::<AgentKind>(&line).unwrap(), agent);
        }
        // Spelled out, because these strings are in files on disk now.
        assert_eq!(
            serde_json::to_string(&AgentKind::Claude).unwrap(),
            "\"claude\""
        );
        assert_eq!(
            serde_json::to_string(&AgentKind::Codex).unwrap(),
            "\"codex\""
        );
    }

    #[test]
    fn the_programs_and_labels_are_distinct() {
        let programs: Vec<&str> = AgentKind::ALL.iter().map(|a| a.program()).collect();
        assert_eq!(programs, ["claude", "codex"]);
        // Their own names, so a missing-tool message names what to install.
        assert_eq!(AgentKind::Claude.label(), "Claude Code");
        assert_eq!(AgentKind::Codex.to_string(), "Codex");
    }
}
