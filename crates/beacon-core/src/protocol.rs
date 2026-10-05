use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::agent::AgentKind;
use crate::clips::{Clip, ClipKind};
use crate::domain::{ClipId, ProjectId};
use crate::session::{SessionId, SessionInfo, SessionKind};
use crate::settings::ShellSpec;
use crate::workstreams::{Workstream, WorkstreamId};

/// The wire contract between Beacon and its session daemon.
///
/// Bumped whenever a message changes shape — including when one is *added*,
/// which is the case that is easy to forget: a daemon from an older build
/// simply rejects the new request, and the version check that exists to replace
/// it never fires because nobody moved the number.
///
/// A client that finds a daemon speaking a different version asks it to quit
/// and starts one it understands, rather than guessing: a half-understood
/// session is worse than a new one.
///
/// Version 2 added `Report`, `ReportUsage` and `Usage`.
/// Version 3 gave sessions a slot, so a project can hold several terminals, and
/// let the client say which shell to run.
/// Version 4 added `Clip`, `Clips` and `ForgetClips` — the drawer of things to
/// copy. Three new requests, so by the rule above the number had to move, and
/// upgrading to it replaces a running daemon and the sessions it holds. Paid
/// once, knowingly: the alternative is an older daemon rejecting every clip
/// while the window shows an empty drawer and no reason for it.
///
/// `ClaudeActivity::Idle` was added without a version, deliberately. The rule
/// is about the set of requests: a daemon that meets one it does not know
/// leaves a client waiting on a session it cannot use. A value it does not know
/// inside a `Report` costs one dropped report — the daemon answers with an
/// error and carries on, and the tab keeps saying what it already said. Paying
/// for that with a forced daemon replacement would kill every running session
/// on upgrade, which is a far worse trade than a report that goes missing until
/// the daemon is next restarted.
///
/// `UsageReport` grew the rest of the status line payload without a version for
/// the same reason. Every field it gained is optional in both directions: an
/// older daemon ignores what it does not recognise, and a newer client reads a
/// missing field as unknown, which is what it already does for a plan that
/// reports no rate limits. Nothing is left waiting on either side.
///
/// Version 5 added the workstream requests — `Workstreams`, `StartWorkstream`,
/// `ResumeWorkstream`, `ForkWorkstream` and `RenameWorkstream` — and
/// `ReportAgent`, which says a subagent started or stopped. Six new requests,
/// so by the rule above the number had to move, and upgrading to it replaces a
/// running daemon and the sessions it holds. Paid once, knowingly: an older
/// daemon would reject every one of them, leaving a window that can list
/// conversations it cannot open.
///
/// `Event::Restarted` was added without a version, for the reason `Idle` was.
/// A client that cannot read an event logs it and carries on, and the only
/// client that could meet one it does not know is an older window on a newer
/// daemon, which a window never starts. The other way round, a newer window on
/// an older daemon simply never hears it. Neither leaves anything waiting, and
/// replacing the daemon for it would end every running session on upgrade.
pub const PROTOCOL_VERSION: u32 = 7;

/// Newline-delimited JSON, one message per line.
///
/// Chosen over anything framed or binary because the traffic is small, the
/// contents are inspectable with `nc` when something goes wrong, and the whole
/// codec is two lines of `serde_json`.
/// What a Claude session is doing, as reported by Claude Code itself.
///
/// These come from hooks rather than from reading the terminal. Milestone 3
/// left `dev server` and `error` unimplemented because inferring them from
/// output is guesswork; this is the difference between guessing and being told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaudeActivity {
    /// Running a tool, or thinking.
    Working,
    /// Stopped and waiting for the user — a permission prompt, a question.
    /// The state worth interrupting someone for.
    Waiting,
    /// Finished its turn.
    Done,
    /// Open, with nothing claimed about it — a session that has just started,
    /// resumed, or been cleared.
    ///
    /// Worth a state of its own rather than reusing `Done`: `waiting` and
    /// `done` never expire, because both can honestly last hours. That makes a
    /// session resumed after a permission prompt keep shouting for attention it
    /// no longer needs, and a fresh session claim it finished something it
    /// never started. This says only that Claude is there.
    Idle,
    /// The session ended.
    Ended,
}

/// What a Claude session is costing, as Claude Code itself reports it.
///
/// Every field is optional because Claude Code fills in what it knows: rate
/// limits are absent on plans without them, and the context window is unknown
/// until the first turn. A missing number is shown as missing rather than as
/// zero, which would read as "you have used none of it".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub project: ProjectId,
    /// The conversation Claude Code is in, as it identifies it.
    ///
    /// The reason this is worth carrying: it is how a running session can be
    /// matched to a workstream Beacon started, without reading a transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The name given with `--name` or `/rename`, when there is one.
    ///
    /// Absent for an automatic display name like `beacon-split-b7`, so this
    /// says "the user named it", not "it has a name".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The model's identifier, kept beside its display name so a routing
    /// decision can be made on something stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Reasoning effort, when the model has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    /// How much of the context window this session is using, 0..100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_used_percentage: Option<f32>,
    /// What is left, as Claude Code says it rather than as `100 - used`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_remaining_percentage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_used_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_size: Option<u64>,
    /// The prompt cache, when there has been an API response to observe.
    ///
    /// Grouped rather than flattened like the rate limits, because it arrives
    /// as a group: nothing is known about the cache until the first response,
    /// and then all of it is. A flat set of options would suggest the fields
    /// can go missing one at a time, which is true of a rate-limit window and
    /// not of this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache: Option<PromptCache>,
    /// How much of the five-hour allowance is gone, 0..100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_used_percentage: Option<f32>,
    /// Unix seconds when that window resets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_resets_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_used_percentage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_resets_at: Option<i64>,
    /// The spend limit, for accounts behind a gateway that sets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spend_limit_used_percentage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spend_limit_resets_at: Option<i64>,
    /// The worktree the session is working in, when it is in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// How long the conversation has spent waiting on the API, in ms.
    ///
    /// Carried for one reason: it is taken to grow only when a response
    /// arrives, which is what brings Claude Code new rate limits. That is an
    /// assumption about Claude Code, not something it promises; see
    /// `UsageReport::stamp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_duration_ms: Option<u64>,
    /// Unix ms when the daemon heard this report. Set by the daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_at: Option<i64>,
    /// Unix ms when the rate limits in this report were last new. Set by the
    /// daemon, and older than `reported_at` whenever the report only repeats
    /// what an earlier one said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits_seen_at: Option<i64>,
}

/// How far apart two reset times can be and still be the same five-hour
/// window. Claude Code reports a fixed time, but nothing promises it to the
/// second.
const SAME_WINDOW_SECS: i64 = 60;

/// What the daemon keeps of a conversation's last report, to tell new rate
/// limits from old ones said again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitsSeen {
    pub api_duration_ms: u64,
    pub at: i64,
}

impl UsageReport {
    /// A report about a project that has said nothing yet.
    ///
    /// Every field unknown rather than zero, which is the distinction the whole
    /// type exists to keep: "not reported" and "none used" look identical on a
    /// gauge and mean opposite things.
    pub fn unknown(project: ProjectId) -> Self {
        Self {
            project,
            session_id: None,
            session_name: None,
            model: None,
            model_id: None,
            effort: None,
            thinking: None,
            context_used_percentage: None,
            context_remaining_percentage: None,
            context_used_tokens: None,
            context_size: None,
            prompt_cache: None,
            five_hour_used_percentage: None,
            five_hour_resets_at: None,
            seven_day_used_percentage: None,
            seven_day_resets_at: None,
            spend_limit_used_percentage: None,
            spend_limit_resets_at: None,
            worktree: None,
            api_duration_ms: None,
            reported_at: None,
            limits_seen_at: None,
        }
    }

    /// Dates a report the daemon has just heard, given what it kept of the
    /// same conversation's last one, and returns what to keep of this one.
    ///
    /// Claude Code runs the status line again for things that bring no news
    /// about the allowance — a session resumed, the permission mode changed, a
    /// warm cache expiring in a session nobody is using — and every time it
    /// repeats the rate limits from its last response. Dated on arrival, those
    /// would pass for current and outrank a session whose numbers really are:
    /// the allowance from hours ago shown over the one that is true. So the
    /// limits keep the time they were first seen until the API time moves.
    ///
    /// That rests on an assumption: that the API time grows with each response
    /// and only then. If Claude Code ever ran the status line with new limits
    /// before adding the response's time, those limits would keep the older
    /// date and lose comparisons they should win — which
    /// `has_newer_limits_than` guards against by asking the window first. The
    /// other way round, time growing without new limits, is harmless.
    ///
    /// The first report heard from a conversation has nothing to compare with
    /// and is dated on arrival. That is right for a new conversation, whose
    /// limits only appear with its first response, and wrong for one the
    /// daemon has not heard since it started that repeats limits from before —
    /// which is why the date is the last thing the comparison looks at.
    pub fn stamp(&mut self, previous: Option<LimitsSeen>, now_ms: i64) -> Option<LimitsSeen> {
        self.reported_at = Some(now_ms);
        let seen = match (previous, self.api_duration_ms) {
            (Some(previous), Some(duration)) if previous.api_duration_ms == duration => previous.at,
            _ => now_ms,
        };
        self.limits_seen_at = Some(seen);
        self.api_duration_ms.map(|api_duration_ms| LimitsSeen {
            api_duration_ms,
            at: seen,
        })
    }

    /// Whether this report's rate limits are newer than `other`'s, or `other`
    /// has none.
    ///
    /// The limits belong to the account, not to the project or the session
    /// that reported them, so they are kept apart from the per-project reports:
    /// two sessions in one project take turns as its last report, and the one
    /// repeating old limits would otherwise replace the one with new ones.
    ///
    /// The numbers are asked before the dates, because they cannot be wrong
    /// about their own order the way a date can (see `stamp`): a five-hour
    /// window that resets later is a later window, and within one window the
    /// share used only goes up, so the lower figure is the older one. Only
    /// when both say the same is it down to when they were new. The window
    /// only going up is an assumption too, about how the allowance is counted,
    /// and the safer one: were it wrong, the higher figure is the one to show.
    ///
    /// `newerLimits` in `src/features/usage/usage.ts` decides the same way.
    pub fn has_newer_limits_than(&self, other: Option<&UsageReport>) -> bool {
        let Some(used) = self.five_hour_used_percentage else {
            return false;
        };
        let Some(other) = other else {
            return true;
        };
        let Some(other_used) = other.five_hour_used_percentage else {
            return true;
        };
        if let (Some(mine), Some(theirs)) = (self.five_hour_resets_at, other.five_hour_resets_at) {
            if (mine - theirs).abs() > SAME_WINDOW_SECS {
                return mine > theirs;
            }
            if used != other_used {
                return used > other_used;
            }
        }
        self.limits_seen_at.unwrap_or_default() >= other.limits_seen_at.unwrap_or_default()
    }
}

/// What the prompt cache is doing, as Claude Code reports it.
///
/// The number that changes a decision is `recache_tokens_if_cold`: a large
/// context whose cache has gone cold will be paid for again on the next turn,
/// and that is the moment a clean workstream is worth more than continuing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCache {
    /// Whether the cache is currently warm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warm: Option<bool>,
    /// 0..1, not a percentage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit_ratio: Option<f32>,
    /// Unix seconds when a warm cache goes cold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// What the next turn would cost to write again if the cache were cold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recache_tokens_if_cold: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub misses: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_rebuilds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "camelCase")]
pub enum Request {
    /// First message on a connection. Establishes that both sides agree.
    Hello { version: u32 },
    /// Returns the project's session of this kind, starting one if needed.
    #[serde(rename_all = "camelCase")]
    Ensure {
        project: ProjectId,
        kind: SessionKind,
        /// Which of the project's sessions of this kind.
        #[serde(default)]
        slot: u32,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        /// What to run. Sent by the client rather than read by the daemon, so a
        /// session starts with the shell configured now — not the one that was
        /// configured when the daemon happened to start.
        #[serde(default)]
        shell: Option<ShellSpec>,
        /// Whether Beacon's own subagents are offered to a Claude session.
        ///
        /// Sent by the client for the same reason the shell is: the preference
        /// belongs to the person, and the daemon reads no settings. Absent
        /// means "the client did not say", which is treated as yes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agents: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    Write { id: SessionId, data: String },
    #[serde(rename_all = "camelCase")]
    Resize { id: SessionId, cols: u16, rows: u16 },
    /// Everything the session has produced, for rebuilding a view.
    #[serde(rename_all = "camelCase")]
    Scrollback { id: SessionId },
    #[serde(rename_all = "camelCase")]
    Close { id: SessionId },
    #[serde(rename_all = "camelCase")]
    Restart {
        project: ProjectId,
        kind: SessionKind,
        #[serde(default)]
        slot: u32,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        #[serde(default)]
        shell: Option<ShellSpec>,
        /// Whether Beacon's own subagents are offered to a Claude session.
        ///
        /// Sent by the client for the same reason the shell is: the preference
        /// belongs to the person, and the daemon reads no settings. Absent
        /// means "the client did not say", which is treated as yes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agents: Option<bool>,
    },
    #[serde(rename_all = "camelCase")]
    CloseProject { project: ProjectId },
    /// Reported by a Claude Code hook running inside a session.
    ///
    /// Sent by a short-lived process, not by the window: the hook connects,
    /// says one thing, and exits.
    #[serde(rename_all = "camelCase")]
    Report {
        project: ProjectId,
        /// Which agent is reporting.
        ///
        /// Defaulted to Claude Code, which is what a hook installed before
        /// Beacon ran a second agent is. It decides whose conversation the
        /// `session` below belongs to — and for an agent that names its own,
        /// that is the only way to know.
        #[serde(default)]
        agent: AgentKind,
        activity: ClaudeActivity,
        /// What it is doing, when there is something worth naming — the tool it
        /// just started, for instance.
        detail: Option<String>,
        /// Which conversation is doing it.
        ///
        /// Carried because an event that can only happen inside a turn is proof
        /// that the conversation exists — which is what decides whether the
        /// next start resumes it or creates it. Optional so a hook payload
        /// without one, or an older daemon, costs the proof and nothing else.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<String>,
        /// What Claude said last, when this is the end of a turn.
        ///
        /// Claude Code hands its `Stop` hook the final text of the turn, and
        /// that is the one message worth setting apart from the tool output
        /// around it. Optional, so a hook or a daemon from before it existed
        /// still speak to each other. Held in memory only, never written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply: Option<String>,
    },
    /// Reported by Claude Code's status line, running inside a session.
    #[serde(rename_all = "camelCase")]
    /// Boxed so one large report does not set the size of every message on
    /// the wire. `Box` is transparent to serde, so the line is unchanged.
    ReportUsage { usage: Box<UsageReport> },
    /// Filed by the MCP server running inside a Claude session: something the
    /// user asked for in order to paste it somewhere else.
    ///
    /// The daemon stamps the id and the time rather than the sender. The sender
    /// is a process that lives for one call and has no way to know what is
    /// already in the book, and two clips filed in the same second by different
    /// sessions must still be distinguishable.
    #[serde(rename_all = "camelCase")]
    Clip {
        project: ProjectId,
        title: String,
        body: String,
        #[serde(default)]
        kind: ClipKind,
    },
    /// Everything in the clip book, newest first.
    ///
    /// Kept by the daemon and written to disk, unlike activity: a clip is worth
    /// something precisely *after* the turn that produced it, which is the case
    /// activity explicitly is not.
    Clips {},
    /// Drops one clip, or the whole book when `id` is absent.
    #[serde(rename_all = "camelCase")]
    ForgetClips {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<ClipId>,
    },
    /// The last usage reported for each project.
    ///
    /// Unlike activity, this is worth keeping: a window that has just attached
    /// should show what it costs now, not wait for the next turn to find out.
    Usage {},
    /// Which sessions are alive, so a reattaching client can find its work.
    ///
    /// Carries a body it does not need: a unit variant serialises without a
    /// `params` field, and `#[serde(flatten)]` cannot read an adjacently tagged
    /// enum back without one.
    List {},
    /// Reported by a Claude Code hook when a subagent starts or stops.
    ///
    /// Separate from `Report` because it is about something inside a session
    /// rather than about the session, and because it is the only thing in the
    /// protocol that is deliberately forgotten: an agent that ran for twelve
    /// seconds is worth seeing while it runs and worth nothing afterwards.
    #[serde(rename_all = "camelCase")]
    ReportAgent {
        project: ProjectId,
        /// Claude Code's id for this subagent, so a start and a stop pair up.
        agent: String,
        /// Which agent it is. Claude Code sometimes reports this empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_type: Option<String>,
        running: bool,
        /// A short line about what it found, on the way out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    /// A project's conversations, and which one it is in.
    #[serde(rename_all = "camelCase")]
    Workstreams {
        project: ProjectId,
        /// Which agent's conversations are being asked about.
        ///
        /// Defaulted rather than required, so a client that predates a second
        /// agent still gets an answer — and gets Claude Code's, which is the
        /// only kind it knew about.
        #[serde(default)]
        agent: AgentKind,
    },
    /// Starts a new conversation and moves the project into it.
    ///
    /// Carries the same session arguments as `Ensure` because that is what it
    /// ends in: the project's Claude is replaced by one started in the new
    /// conversation, and it needs a size and a directory like any other.
    #[serde(rename_all = "camelCase")]
    StartWorkstream {
        project: ProjectId,
        /// Which agent the new conversation belongs to. Defaulted to Claude
        /// Code for a client that predates the choice.
        #[serde(default)]
        agent: AgentKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        #[serde(default)]
        shell: Option<ShellSpec>,
        /// Whether Beacon's own subagents are offered to a Claude session.
        ///
        /// Sent by the client for the same reason the shell is: the preference
        /// belongs to the person, and the daemon reads no settings. Absent
        /// means "the client did not say", which is treated as yes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agents: Option<bool>,
    },
    /// Returns to a conversation the project already has.
    #[serde(rename_all = "camelCase")]
    ResumeWorkstream {
        project: ProjectId,
        id: WorkstreamId,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        #[serde(default)]
        shell: Option<ShellSpec>,
        /// Whether Beacon's own subagents are offered to a Claude session.
        ///
        /// Sent by the client for the same reason the shell is: the preference
        /// belongs to the person, and the daemon reads no settings. Absent
        /// means "the client did not say", which is treated as yes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agents: Option<bool>,
    },
    /// Starts a new conversation carrying another's history.
    #[serde(rename_all = "camelCase")]
    ForkWorkstream {
        project: ProjectId,
        from: WorkstreamId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        #[serde(default)]
        shell: Option<ShellSpec>,
        /// Whether Beacon's own subagents are offered to a Claude session.
        ///
        /// Sent by the client for the same reason the shell is: the preference
        /// belongs to the person, and the daemon reads no settings. Absent
        /// means "the client did not say", which is treated as yes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agents: Option<bool>,
    },
    /// Renames one, or takes its name away when told `None`.
    #[serde(rename_all = "camelCase")]
    RenameWorkstream {
        project: ProjectId,
        id: WorkstreamId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Asks the daemon to stop. Used when a client finds a version it does not
    /// speak.
    Shutdown {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// Correlates a reply with its request.
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Greeting {
    pub version: u32,
    pub pid: u32,
    /// How many sessions were already running when this client arrived.
    pub sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum Reply {
    Greeting(Greeting),
    Session(SessionInfo),
    #[serde(rename_all = "camelCase")]
    Scrollback {
        /// Base64-encoded bytes. PTY output is not guaranteed to be valid UTF-8
        /// at a chunk boundary, so it is never coerced into a string.
        data: String,
        /// Stream offset just past the snapshot.
        end_offset: u64,
    },
    /// A struct variant, not a newtype around the list: an internally tagged
    /// enum cannot carry a bare sequence, and serde only finds out at runtime.
    #[serde(rename_all = "camelCase")]
    Sessions {
        sessions: Vec<SessionInfo>,
    },
    /// A struct variant for the same reason as `Sessions`.
    #[serde(rename_all = "camelCase")]
    Usage {
        reports: Vec<UsageReport>,
    },
    /// A struct variant for the same reason as `Sessions`.
    #[serde(rename_all = "camelCase")]
    Clips {
        clips: Vec<Clip>,
    },
    /// A project's conversations, most recently active first.
    #[serde(rename_all = "camelCase")]
    Workstreams {
        workstreams: Vec<Workstream>,
        /// Which one the project is in, when it is in one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current: Option<WorkstreamId>,
    },
    /// One conversation, and the session now running it.
    #[serde(rename_all = "camelCase")]
    Workstream {
        workstream: Box<Workstream>,
        session: SessionInfo,
    },
    Done,
}

/// What comes back for one request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub id: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Ok(Reply),
    Err(String),
}

/// Sent to every connected client, unprompted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Output {
        id: SessionId,
        project: ProjectId,
        offset: u64,
        /// Base64-encoded bytes.
        data: String,
    },
    #[serde(rename_all = "camelCase")]
    Exit {
        id: SessionId,
        project: ProjectId,
        code: Option<i32>,
    },
    /// A project's Claude session reported what it is costing.
    Usage(Box<UsageReport>),
    /// A clip was filed, by this window's session or by another's.
    ///
    /// Broadcast rather than answered to the sender: the sender is the MCP
    /// server, which is not the thing that shows the drawer.
    Clip(Clip),
    /// Every clip was dropped, or one was. Carries the book rather than a
    /// delta, because it is small and a drawer rebuilt from the truth cannot
    /// drift from one that missed an event.
    #[serde(rename_all = "camelCase")]
    Clips { clips: Vec<Clip> },
    /// A subagent started or finished inside a project's Claude session.
    ///
    /// Broadcast and kept nowhere. It is activity, not history.
    #[serde(rename_all = "camelCase")]
    Agent {
        project: ProjectId,
        agent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_type: Option<String>,
        running: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    /// A project's Claude session said what it is doing.
    #[serde(rename_all = "camelCase")]
    Activity {
        project: ProjectId,
        activity: ClaudeActivity,
        detail: Option<String>,
        /// The turn's final reply, on `done`; see `Request::Report`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply: Option<String>,
    },
    /// A session started, but without something it was meant to have.
    ///
    /// Not an error, and deliberately not a refusal: the session is running and
    /// the user can work. It exists because the alternative to saying so is a
    /// feature that is quietly not there, which is discovered much later and
    /// much more expensively than a line in the status bar.
    ///
    /// `summary` is written to be read by a person, because the daemon is the
    /// only layer that knows what actually went wrong.
    #[serde(rename_all = "camelCase")]
    Degraded { project: ProjectId, summary: String },
    /// The daemon started a session again by itself, so any view of it is
    /// showing a process that has gone.
    ///
    /// Carries where the session lives rather than its new id: a window asks
    /// for a project's session by place, and asking again is how it finds the
    /// new one. See `crate::sign_in` for the one thing that does this.
    #[serde(rename_all = "camelCase")]
    Restarted {
        project: ProjectId,
        kind: SessionKind,
        slot: u32,
    },
}

/// One line from the daemon: either a reply, or something that just happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Response(Response),
    Event(Event),
}

/// Where the daemon listens.
///
/// A socket under the per-user temporary directory rather than the config
/// directory: it is runtime state, it should not be synced, and it should not
/// survive a reboot. Access control is the containing directory's permissions —
/// the socket is only reachable by the user who owns it.
pub fn socket_path() -> PathBuf {
    socket_dir().join(crate::transport::SOCKET_FILE)
}

pub fn socket_dir() -> PathBuf {
    // On Linux this is shared between users, so the name has to distinguish
    // them. On macOS and Windows the temporary directory is already per-user —
    // and Windows spells the variable differently, which is harmless there but
    // worth getting right.
    let user = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" })
        .unwrap_or_else(|_| "beacon".to_string());
    std::env::temp_dir().join(format!("beacon-split-{user}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_survives_a_round_trip() {
        let envelope = Envelope {
            id: 7,
            request: Request::Ensure {
                project: ProjectId::generate(),
                kind: SessionKind::Claude,
                slot: 0,
                cwd: PathBuf::from("/tmp/project"),
                cols: 80,
                rows: 24,
                shell: None,
                agents: Some(true),
            },
        };

        let line = serde_json::to_string(&envelope).unwrap();
        let back: Envelope = serde_json::from_str(&line).unwrap();
        assert_eq!(back.id, 7);
        assert!(matches!(back.request, Request::Ensure { cols: 80, .. }));
    }

    /// Every variant, because the one that broke was the one not covered:
    /// a unit variant serialises without `params`, and flatten cannot read it
    /// back.
    #[test]
    fn every_request_survives_a_round_trip() {
        let project = ProjectId::generate();
        let id = SessionId("sn_x".into());
        let cwd = PathBuf::from("/tmp/project");

        let requests = vec![
            Request::Hello { version: 1 },
            Request::Ensure {
                project: project.clone(),
                kind: SessionKind::Shell,
                slot: 0,
                cwd: cwd.clone(),
                cols: 80,
                rows: 24,
                shell: None,
                agents: None,
            },
            Request::Write {
                id: id.clone(),
                data: "ls\n".into(),
            },
            Request::Resize {
                id: id.clone(),
                cols: 100,
                rows: 40,
            },
            Request::Scrollback { id: id.clone() },
            Request::Close { id: id.clone() },
            Request::Restart {
                project: project.clone(),
                kind: SessionKind::Claude,
                slot: 0,
                cwd: cwd.clone(),
                cols: 80,
                rows: 24,
                shell: None,
                agents: Some(false),
            },
            Request::CloseProject {
                project: project.clone(),
            },
            Request::List {},
            Request::Shutdown {},
            Request::Report {
                project: ProjectId("pj_y".into()),
                agent: AgentKind::Codex,
                activity: ClaudeActivity::Waiting,
                detail: Some("Bash".into()),
                session: Some("cafb8c86-53eb-49c4-a8b8-609e5cbc0f49".into()),
                reply: Some("Done — the tests pass.".into()),
            },
            Request::ReportUsage {
                usage: Box::new(sample_usage()),
            },
            Request::Usage {},
            Request::Clip {
                project: ProjectId("pj_y".into()),
                title: "Staging keys".into(),
                body: "API_KEY=abc".into(),
                kind: ClipKind::Variable,
            },
            Request::Clips {},
            Request::ForgetClips {
                id: Some(ClipId("cl_x".into())),
            },
            Request::Workstreams {
                project: ProjectId("pj_y".into()),
                agent: AgentKind::Claude,
            },
            Request::StartWorkstream {
                project: ProjectId("pj_y".into()),
                agent: AgentKind::Codex,
                name: Some("auth-refactor".into()),
                cwd: cwd.clone(),
                cols: 80,
                rows: 24,
                shell: None,
                agents: Some(true),
            },
            Request::ResumeWorkstream {
                project: ProjectId("pj_y".into()),
                id: WorkstreamId("cafb8c86-53eb-49c4-a8b8-609e5cbc0f49".into()),
                cwd: cwd.clone(),
                cols: 80,
                rows: 24,
                shell: None,
                agents: None,
            },
            Request::ForkWorkstream {
                project: ProjectId("pj_y".into()),
                from: WorkstreamId("cafb8c86-53eb-49c4-a8b8-609e5cbc0f49".into()),
                name: None,
                cwd,
                cols: 80,
                rows: 24,
                shell: None,
                agents: Some(true),
            },
            Request::RenameWorkstream {
                project: ProjectId("pj_y".into()),
                id: WorkstreamId("cafb8c86-53eb-49c4-a8b8-609e5cbc0f49".into()),
                name: Some("payments-bug".into()),
            },
            Request::ReportAgent {
                project: ProjectId("pj_y".into()),
                agent: "a0718b64719533846".into(),
                agent_type: Some("beacon-explorer".into()),
                running: false,
                summary: Some("Found 4 relevant files".into()),
            },
        ];

        // A guard, not a formality. Adding a request without moving
        // PROTOCOL_VERSION leaves older daemons rejecting it instead of being
        // replaced, which is exactly what happened once already.
        assert_eq!(
            requests.len(),
            22,
            "the set of requests changed: PROTOCOL_VERSION must change with it"
        );

        for (index, request) in requests.into_iter().enumerate() {
            let envelope = Envelope {
                id: index as u64,
                request,
            };
            let line = serde_json::to_string(&envelope).unwrap();
            let back: Envelope = serde_json::from_str(&line)
                .unwrap_or_else(|err| panic!("{line} did not round-trip: {err}"));
            assert_eq!(back.id, index as u64);
        }
    }

    /// The same guard the requests and replies have, for the same reason: a
    /// client from before an event existed cannot parse it, so the version has
    /// to move with the set.
    #[test]
    fn every_event_survives_a_round_trip() {
        let events = vec![
            Event::Output {
                id: SessionId("sn_x".into()),
                project: ProjectId("pj_x".into()),
                offset: 0,
                data: "aGk=".into(),
            },
            Event::Exit {
                id: SessionId("sn_x".into()),
                project: ProjectId("pj_x".into()),
                code: Some(0),
            },
            Event::Usage(Box::new(sample_usage())),
            Event::Clip(sample_clip()),
            Event::Clips {
                clips: vec![sample_clip()],
            },
            Event::Agent {
                project: ProjectId("pj_x".into()),
                agent: "a0718b64719533846".into(),
                agent_type: Some("beacon-explorer".into()),
                running: true,
                summary: None,
            },
            Event::Activity {
                project: ProjectId("pj_x".into()),
                activity: ClaudeActivity::Working,
                detail: Some("Edit".into()),
                reply: None,
            },
            Event::Degraded {
                project: ProjectId("pj_x".into()),
                summary: "The clip drawer is unavailable.".into(),
            },
            Event::Restarted {
                project: ProjectId("pj_x".into()),
                kind: SessionKind::Claude,
                slot: 0,
            },
        ];

        // `Restarted` was added without a version; see PROTOCOL_VERSION.
        assert_eq!(
            events.len(),
            9,
            "the set of events changed: PROTOCOL_VERSION must change with it"
        );

        for event in events {
            let line = serde_json::to_string(&event).unwrap();
            let back: Message = serde_json::from_str(&line)
                .unwrap_or_else(|err| panic!("{line} did not round-trip: {err}"));
            assert!(matches!(back, Message::Event(_)));
        }
    }

    #[test]
    fn a_reply_and_an_event_are_told_apart_on_the_same_stream() {
        let response = serde_json::to_string(&Response {
            id: 1,
            outcome: Outcome::Ok(Reply::Done),
        })
        .unwrap();
        let event = serde_json::to_string(&Event::Exit {
            id: SessionId("sn_x".into()),
            project: ProjectId("pj_x".into()),
            code: Some(0),
        })
        .unwrap();

        assert!(matches!(
            serde_json::from_str::<Message>(&response).unwrap(),
            Message::Response(_)
        ));
        assert!(matches!(
            serde_json::from_str::<Message>(&event).unwrap(),
            Message::Event(_)
        ));
    }

    /// The mirror of the request test, and for the same reason: the variant
    /// that broke was a newtype around a `Vec`, which an internally tagged enum
    /// cannot serialise at all.
    #[test]
    fn every_reply_survives_a_round_trip() {
        let info = SessionInfo {
            id: SessionId("sn_x".into()),
            project: ProjectId("pj_x".into()),
            kind: SessionKind::Shell,
            slot: 0,
            cwd: "/tmp/project".into(),
            running: true,
            cols: 132,
            rows: 40,
        };

        let replies = vec![
            Reply::Greeting(Greeting {
                version: PROTOCOL_VERSION,
                pid: 1234,
                sessions: 2,
            }),
            Reply::Session(info.clone()),
            Reply::Scrollback {
                data: "aGk=".into(),
                end_offset: 12,
            },
            Reply::Sessions {
                sessions: vec![info],
            },
            Reply::Usage {
                reports: vec![sample_usage()],
            },
            Reply::Clips {
                clips: vec![sample_clip()],
            },
            Reply::Done,
        ];

        assert_eq!(
            replies.len(),
            7,
            "the set of replies changed: PROTOCOL_VERSION must change with it"
        );

        for (index, reply) in replies.into_iter().enumerate() {
            let response = Response {
                id: index as u64,
                outcome: Outcome::Ok(reply),
            };
            let line = serde_json::to_string(&response)
                .unwrap_or_else(|err| panic!("reply {index} could not be encoded: {err}"));
            let back: Response = serde_json::from_str(&line)
                .unwrap_or_else(|err| panic!("{line} did not round-trip: {err}"));
            assert_eq!(back.id, index as u64);
        }
    }

    #[test]
    fn an_error_outcome_carries_its_message() {
        let line = serde_json::to_string(&Response {
            id: 2,
            outcome: Outcome::Err("no such session".into()),
        })
        .unwrap();

        let back: Response = serde_json::from_str(&line).unwrap();
        match back.outcome {
            Outcome::Err(message) => assert_eq!(message, "no such session"),
            Outcome::Ok(_) => panic!("expected an error"),
        }
    }

    fn sample_clip() -> Clip {
        Clip {
            id: ClipId("cl_x".into()),
            project: ProjectId("pj_x".into()),
            title: "Staging keys".into(),
            body: "API_KEY=abc".into(),
            kind: ClipKind::Variable,
            created_at: 1_800_000_000,
        }
    }

    /// A clip event is a newtype around a struct inside an adjacently tagged
    /// enum — the exact shape that could not be serialised when `Sessions` was
    /// written as one, so it is worth a test rather than an assumption.
    #[test]
    fn a_clip_event_survives_the_same_stream_as_a_reply() {
        let event = serde_json::to_string(&Event::Clip(sample_clip())).unwrap();
        let back = serde_json::from_str::<Message>(&event).unwrap();
        match back {
            Message::Event(Event::Clip(clip)) => {
                assert_eq!(clip.body, "API_KEY=abc");
                // The body is what lands on the clipboard: it must survive the
                // wire byte for byte, newlines and all.
                let multiline = Clip {
                    body: "FOO=1\n  BAR=2".into(),
                    ..sample_clip()
                };
                let line = serde_json::to_string(&Event::Clip(multiline)).unwrap();
                match serde_json::from_str::<Message>(&line).unwrap() {
                    Message::Event(Event::Clip(clip)) => {
                        assert_eq!(clip.body, "FOO=1\n  BAR=2")
                    }
                    other => panic!("expected a clip, got {other:?}"),
                }
            }
            other => panic!("expected a clip event, got {other:?}"),
        }
    }

    #[test]
    fn a_clip_defaults_to_plain_text_when_the_sender_says_nothing() {
        // The MCP server may omit `kind`; a missing one must not be a parse
        // failure that drops the clip silently.
        let line = r#"{"id":1,"method":"clip","params":{"project":"pj_x","title":"t","body":"b"}}"#;
        let back: Envelope = serde_json::from_str(line).unwrap();
        match back.request {
            Request::Clip { kind, .. } => assert_eq!(kind, ClipKind::Text),
            other => panic!("expected a clip, got {other:?}"),
        }
    }

    fn sample_usage() -> UsageReport {
        UsageReport {
            model: Some("claude-sonnet-4-6".into()),
            context_used_percentage: Some(37.5),
            context_used_tokens: Some(75_000),
            context_size: Some(200_000),
            five_hour_used_percentage: Some(12.0),
            five_hour_resets_at: Some(1_800_000_000),
            ..UsageReport::unknown(ProjectId("pj_x".into()))
        }
    }

    #[test]
    fn a_usage_report_keeps_the_difference_between_none_and_zero() {
        // Absent means "not known", which is not the same as "none used" — and
        // showing the second when you mean the first is a lie about how much
        // room is left.
        let report = sample_usage();
        let line = serde_json::to_string(&report).unwrap();
        assert!(!line.contains("sevenDay"), "got {line}");

        let back: UsageReport = serde_json::from_str(&line).unwrap();
        assert_eq!(back.seven_day_used_percentage, None);
        assert_eq!(back.five_hour_used_percentage, Some(12.0));
    }

    fn in_session(session: &str, api_duration_ms: Option<u64>) -> UsageReport {
        UsageReport {
            session_id: Some(session.into()),
            api_duration_ms,
            ..sample_usage()
        }
    }

    #[test]
    fn rate_limits_said_again_keep_the_time_they_were_new() {
        let mut first = in_session("s1", Some(2_300));
        let kept = first.stamp(None, 1_000);
        assert_eq!(first.reported_at, Some(1_000));
        assert_eq!(first.limits_seen_at, Some(1_000));

        // Hours later the cache expires in that idle session, and Claude Code
        // runs the status line again with the numbers from its last response.
        let mut repeated = in_session("s1", Some(2_300));
        let kept = repeated.stamp(kept, 9_000_000);
        assert_eq!(repeated.reported_at, Some(9_000_000));
        assert_eq!(repeated.limits_seen_at, Some(1_000));

        // A response moves the API time, and only then are the limits new.
        let mut answered = in_session("s1", Some(4_100));
        answered.stamp(kept, 9_500_000);
        assert_eq!(answered.limits_seen_at, Some(9_500_000));
    }

    #[test]
    fn the_account_keeps_the_newest_limits_whichever_session_spoke_last() {
        // Two sessions in one project: one working, one idle and repeating.
        let mut working = in_session("s1", Some(9_000));
        working.five_hour_used_percentage = Some(92.0);
        working.stamp(None, 9_000_000);

        let mut idle = in_session("s2", Some(2_300));
        idle.five_hour_used_percentage = Some(67.0);
        idle.stamp(
            Some(LimitsSeen {
                api_duration_ms: 2_300,
                at: 1_000,
            }),
            9_100_000,
        );

        // The idle one spoke last, and is the project's last report — but not
        // the account's limits.
        assert!(working.has_newer_limits_than(None));
        assert!(!idle.has_newer_limits_than(Some(&working)));
        assert!(working.has_newer_limits_than(Some(&idle)));

        // A report without limits never takes their place.
        let mut quiet = in_session("s3", Some(1));
        quiet.five_hour_used_percentage = None;
        quiet.stamp(None, 9_200_000);
        assert!(!quiet.has_newer_limits_than(Some(&working)));
    }

    #[test]
    fn after_a_restart_the_window_decides_and_not_who_spoke_last() {
        // The daemon has just started and knows neither conversation, so both
        // are dated on arrival — and the idle one, repeating 67% from hours
        // ago, arrives after the one that has just answered with 92%.
        let mut working = in_session("s1", Some(9_000));
        working.five_hour_used_percentage = Some(92.0);
        working.stamp(None, 9_000_000);

        let mut idle = in_session("s2", Some(2_300));
        idle.five_hour_used_percentage = Some(67.0);
        idle.stamp(None, 9_100_000);

        // Same window, and a window's share only goes up: 67% is the older.
        assert!(!idle.has_newer_limits_than(Some(&working)));
        assert!(working.has_newer_limits_than(Some(&idle)));
    }

    #[test]
    fn a_later_window_is_newer_whatever_it_has_used() {
        let mut old_window = in_session("s1", Some(9_000));
        old_window.five_hour_used_percentage = Some(92.0);
        old_window.stamp(None, 9_000_000);

        // The window came round: 5% of a window that resets five hours later.
        let mut new_window = in_session("s2", Some(100));
        new_window.five_hour_used_percentage = Some(5.0);
        new_window.five_hour_resets_at = Some(1_800_000_000 + 5 * 3_600);
        new_window.stamp(None, 1_000);

        assert!(new_window.has_newer_limits_than(Some(&old_window)));
        assert!(!old_window.has_newer_limits_than(Some(&new_window)));
    }

    #[test]
    fn a_reset_time_a_few_seconds_off_is_the_same_window() {
        let mut first = in_session("s1", Some(9_000));
        first.five_hour_used_percentage = Some(40.0);
        first.stamp(None, 2_000);

        let mut second = in_session("s2", Some(2_300));
        second.five_hour_used_percentage = Some(30.0);
        second.five_hour_resets_at = Some(1_800_000_000 + 20);
        second.stamp(None, 3_000);

        // Twenty seconds later is not a later window: 30% is older than 40%.
        assert!(!second.has_newer_limits_than(Some(&first)));
    }

    #[test]
    fn the_same_figures_are_down_to_when_they_were_new() {
        let mut earlier = in_session("s1", Some(9_000));
        earlier.stamp(None, 1_000);
        let mut later = in_session("s2", Some(9_000));
        later.stamp(None, 2_000);

        assert!(later.has_newer_limits_than(Some(&earlier)));
        assert!(!earlier.has_newer_limits_than(Some(&later)));

        // And without reset times there is only the date to go on.
        earlier.five_hour_resets_at = None;
        earlier.five_hour_used_percentage = Some(99.0);
        assert!(later.has_newer_limits_than(Some(&earlier)));
    }

    #[test]
    fn a_new_api_time_is_new_whichever_way_it_moved() {
        // Resumed, the total can start again from somewhere else; it is the
        // change that means a response, not the direction.
        let mut resumed = in_session("s1", Some(100));
        resumed.stamp(
            Some(LimitsSeen {
                api_duration_ms: 2_300,
                at: 1_000,
            }),
            5_000,
        );
        assert_eq!(resumed.limits_seen_at, Some(5_000));
    }

    #[test]
    fn without_the_api_time_a_report_is_taken_as_new() {
        // An older Claude Code that does not send it: dated on arrival, which
        // is what every report was before.
        let mut report = in_session("s1", None);
        let kept = report.stamp(
            Some(LimitsSeen {
                api_duration_ms: 2_300,
                at: 1_000,
            }),
            5_000,
        );
        assert_eq!(report.limits_seen_at, Some(5_000));
        assert_eq!(kept, None);
    }

    #[test]
    fn the_times_survive_the_trip_to_the_window_and_an_older_daemon_sends_none() {
        let mut report = in_session("s1", Some(2_300));
        report.stamp(None, 1_000);
        let line = serde_json::to_string(&report).unwrap();
        assert!(line.contains(r#""reportedAt":1000"#), "got {line}");
        assert!(line.contains(r#""limitsSeenAt":1000"#), "got {line}");

        let old: UsageReport = serde_json::from_str(r#"{ "project": "pj_x" }"#).unwrap();
        assert_eq!(old.reported_at, None);
        assert_eq!(old.limits_seen_at, None);
    }

    #[test]
    fn an_activity_event_is_told_apart_from_a_reply() {
        let event = serde_json::to_string(&Event::Activity {
            project: ProjectId("pj_x".into()),
            activity: ClaudeActivity::Waiting,
            detail: None,
            reply: None,
        })
        .unwrap();

        assert!(matches!(
            serde_json::from_str::<Message>(&event).unwrap(),
            Message::Event(Event::Activity { .. })
        ));
    }

    #[test]
    fn the_socket_lives_outside_the_config_directory() {
        let path = socket_path();
        assert!(path.ends_with(crate::transport::SOCKET_FILE));
        assert!(
            !path.to_string_lossy().contains("Application Support"),
            "runtime state does not belong with synced configuration"
        );
    }
}
