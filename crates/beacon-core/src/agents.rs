//! The subagents Beacon offers a session.
//!
//! Passed with `--agents`, which means they live for one session and write
//! nothing into the user's repository. Nothing to install, nothing to uninstall,
//! and a project's `.claude/agents/` stays whatever its owner made it.
//!
//! There are three, and the number is the design. Every description is loaded
//! into the main conversation's context so it can decide what to delegate, so
//! an agent that is rarely the right answer costs tokens in every session that
//! never uses it. Descriptions are one line for the same reason; the detail
//! lives in the prompt, which is only read when the agent actually runs.
//!
//! What they have in common is the point: each one exists to keep a large,
//! low-value pile of text — a repository search, a test log, a whole diff — out
//! of the conversation that is doing the work, and to hand back only the part
//! that changes what happens next.

use serde::Serialize;

/// One agent, in the shape `--agents` expects.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// One line. This is what costs context in every session.
    pub description: &'static str,
    /// The detail, read only when the agent runs.
    pub prompt: &'static str,
    pub tools: &'static [&'static str],
    /// An alias rather than a model id, so this does not go stale when the
    /// models move on.
    pub model: &'static str,
    /// A ceiling on how long it can go before it has to answer.
    pub max_turns: u32,
}

/// Searches, and answers rather than quoting.
const EXPLORER: Agent = Agent {
    description: "Searches a large codebase and returns findings, not files.",
    prompt: "\
You search a repository so that the conversation that asked does not have to \
read it.

Answer with, in this order:
1. The answer to what was asked, in one or two sentences.
2. The files that matter, as `path:line`, with one line each on why.
3. Anything the asker now has to decide, if the search turned one up.

Never paste a file, a function or a block of output unless the exact text is \
the answer — and then quote only the lines that are. If you found nothing, say \
so plainly and say where you looked; a confident guess is worse than an empty \
answer here, because the asker cannot see what you saw.

Read only. If the answer needs something changed, say what and stop.",
    tools: &["Read", "Grep", "Glob"],
    // The cheapest model that can read code, because this is the agent that
    // runs most often and reads the most.
    model: "haiku",
    max_turns: 12,
};

/// Runs the thing that produces four thousand lines, and reads them.
const TESTER: Agent = Agent {
    description: "Runs tests or noisy commands and returns what failed.",
    prompt: "\
You run tests and other commands whose output is too large to be worth reading \
in full, and report what happened.

Answer with:
1. The exact command you ran.
2. Passed or failed, with the counts.
3. For each failure: the test's name, the assertion or error, and the one or \
   two lines of output that explain it.
4. The root cause if the output makes it plain, and where in the source it is.

Never paste the whole log. If a failure is only explained by a long block, \
quote the smallest part that explains it and say which file the rest is in.

Do not fix anything. You are here so that whoever does can see what is wrong \
without reading four thousand lines to find it.",
    tools: &["Read", "Grep", "Glob", "Bash"],
    model: "sonnet",
    max_turns: 12,
};

/// Reads the change with no memory of having written it.
const REVIEWER: Agent = Agent {
    description: "Reviews a finished change with fresh eyes.",
    prompt: "\
You review a change you did not write, having seen none of the conversation \
that produced it. That is the whole value: you cannot be talked into thinking \
something is fine because of how it got there.

Read the diff — `git diff`, and `git diff --cached` for what is staged — and \
report:
1. Anything that is wrong: a bug, a case that is not handled, a lost error, a \
   security problem that is actually reachable here.
2. Anything the change claims to do and does not.
3. Tests that should exist for this change and do not.

For each one, say the file and line, what happens, and what would have to be \
true for it to bite.

Do not report style, naming, formatting, or preferences. Do not suggest \
refactors that are not fixing something. An empty review is a real result, and \
a list padded with taste is worse than one, because it buries the finding that \
mattered.

Read only. Report; do not fix.",
    tools: &["Read", "Grep", "Glob", "Bash"],
    model: "sonnet",
    // Shorter than the others: reviewing is reading a bounded thing, and an
    // agent still going after eight turns has stopped reviewing and started
    // exploring.
    max_turns: 8,
};

/// The routing policy, appended to the system prompt.
///
/// Short on purpose. It is in the context of every turn of every session, so
/// anything it saves in delegated output it has to save several times over. The
/// agents' own descriptions already say what each is for; this only says when
/// handing work over is worth it at all.
pub const ROUTING_POLICY: &str = "\
Delegate to beacon-explorer when finding something would mean reading a lot of \
the repository, and to beacon-tester when a command's output would be large. \
Use beacon-reviewer once a substantial change is finished. Keep this \
conversation for decisions and the work itself, and do not delegate anything \
you could do in a couple of tool calls.";

/// The agents, by the names Claude Code will know them by.
pub fn agents() -> [(&'static str, Agent); 3] {
    [
        ("beacon-explorer", EXPLORER),
        ("beacon-tester", TESTER),
        ("beacon-reviewer", REVIEWER),
    ]
}

/// The value for `--agents`.
pub fn definitions() -> String {
    let map: std::collections::BTreeMap<_, _> = agents().into_iter().collect();
    // Compact, because this goes on a command line.
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_definitions_are_the_shape_the_flag_expects() {
        let value: serde_json::Value = serde_json::from_str(&definitions()).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 3);

        for (name, agent) in object {
            assert!(name.starts_with("beacon-"), "{name} is not ours");
            assert!(
                agent.get("description").is_some(),
                "{name} has no description"
            );
            assert!(agent.get("prompt").is_some(), "{name} has no prompt");
            assert!(agent.get("model").is_some(), "{name} has no model");
            assert!(agent.get("maxTurns").is_some(), "{name} has no ceiling");
        }
    }

    #[test]
    fn descriptions_stay_short_because_they_are_never_not_loaded() {
        // Every one of these is in the main conversation's context in every
        // session, used or not. The detail belongs in the prompt, which is read
        // only when the agent runs.
        for (name, agent) in agents() {
            assert!(
                agent.description.len() <= 80,
                "{name}'s description is {} characters",
                agent.description.len()
            );
            assert!(
                agent.prompt.len() > agent.description.len() * 4,
                "{name} put its detail in the wrong place"
            );
        }
    }

    #[test]
    fn the_routing_policy_is_short_enough_to_be_worth_having() {
        // It is in the context of every turn, so what it saves in delegated
        // output it has to save several times over.
        assert!(ROUTING_POLICY.len() < 400, "{}", ROUTING_POLICY.len());
    }

    #[test]
    fn every_agent_has_to_answer_eventually() {
        for (name, agent) in agents() {
            assert!(
                (1..=15).contains(&agent.max_turns),
                "{name} may run for {} turns",
                agent.max_turns
            );
        }
    }

    #[test]
    fn the_ones_that_only_read_cannot_write() {
        // Explorer and reviewer report; they do not change anything. The tester
        // needs a shell to run tests with, and is told in its prompt not to fix.
        for name in ["beacon-explorer", "beacon-reviewer"] {
            let agent = agents().into_iter().find(|(n, _)| *n == name).unwrap().1;
            for forbidden in ["Edit", "Write", "NotebookEdit"] {
                assert!(!agent.tools.contains(&forbidden), "{name} can {forbidden}");
            }
        }
        let explorer = agents()
            .into_iter()
            .find(|(n, _)| *n == "beacon-explorer")
            .unwrap()
            .1;
        assert!(
            !explorer.tools.contains(&"Bash"),
            "the explorer has a shell"
        );
    }

    #[test]
    fn models_are_aliases_rather_than_ids_that_go_stale() {
        for (name, agent) in agents() {
            assert!(
                ["haiku", "sonnet", "opus", "inherit"].contains(&agent.model),
                "{name} pins {}",
                agent.model
            );
        }
    }
}
