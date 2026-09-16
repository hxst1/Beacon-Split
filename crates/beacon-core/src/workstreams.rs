//! Claude conversations, named and kept.
//!
//! A workstream is one piece of work — a feature, a bug, a refactor — and the
//! Claude conversation that belongs to it. It exists because the alternative is
//! what Beacon did until now: one nameless Claude per project, started fresh
//! every time and gone the moment it was restarted, which pushes everything
//! into a single conversation that grows until it is mostly things nobody is
//! working on any more.
//!
//! Beacon chooses the conversation's id rather than discovering it. Claude Code
//! takes `--session-id <uuid>` when starting one and when forking one, so the
//! id is known before the process exists, and nothing here ever has to read a
//! transcript to find out what it is talking to.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentKind;
use crate::clips::now_seconds;
use crate::domain::ProjectId;
use crate::error::Result;
use crate::protocol::UsageReport;
use crate::store::{JsonStore, ensure_schema};

/// How many conversations a project keeps.
///
/// A cap rather than everything, because the list is something a person reads:
/// past a screenful it stops being a way back into work and becomes an archive
/// nobody opens. The oldest go first, and never the current one.
pub const MAX_PER_PROJECT: usize = 30;

/// A Claude conversation's id: the UUID Beacon gave it.
///
/// Hyphenated, because that is the form `--session-id` accepts. Not built with
/// the `id_type!` macro the rest of the domain uses: those carry a `pj_` style
/// prefix, and Claude Code would reject one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkstreamId(pub String);

impl WorkstreamId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first block of the UUID, for showing a conversation that has no name.
    pub fn short(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }
}

impl std::fmt::Display for WorkstreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One piece of work and the conversation that belongs to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workstream {
    pub id: WorkstreamId,
    pub project: ProjectId,
    /// What the user called it, if they called it anything.
    ///
    /// Optional rather than defaulted, because a name Beacon invented would be
    /// indistinguishable from one the user chose — and Claude Code draws the
    /// same line: `session_name` appears in its status line payload only once
    /// somebody has actually named the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub created_at: i64,
    pub last_active_at: i64,
    /// The conversation this was forked from, when it was forked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forked_from: Option<WorkstreamId>,
    /// Whether the conversation exists as far as Claude Code is concerned.
    ///
    /// Which flag the next start uses turns on this: `--session-id` is refused
    /// once a conversation exists, and `--resume` is refused until it does.
    ///
    /// Not the same as "Beacon has started a process with this id", which is
    /// what this used to mean and was wrong. Claude Code writes nothing until
    /// the first exchange, so a session that was opened and never typed into
    /// leaves no conversation behind — and asking to resume it answers *"No
    /// conversation found with session ID"*. What settles it is a report from
    /// inside the session saying a turn has happened: a hook, or a status line
    /// that has seen the window fill.
    ///
    /// Persisted, because a daemon that came back after a restart has to know
    /// which of the two flags to use and cannot ask.
    #[serde(default)]
    pub resumable: bool,
    /// Which agent's conversation this is.
    ///
    /// Defaulted rather than versioned: everything recorded before Beacon ran
    /// more than one agent is a Claude Code conversation, so an old book reads
    /// correctly without a migration.
    #[serde(default)]
    pub agent: AgentKind,
    /// The id the agent itself uses for this conversation, when Beacon could
    /// not choose it.
    ///
    /// Empty for Claude Code, whose id Beacon picks and passes with
    /// `--session-id` — there, this and [`Workstream::id`] would be the same
    /// number twice. Codex generates its own and reports it in a hook, so this
    /// is how Beacon learns what it is talking to. See
    /// [`Workstream::resume_id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    /// The last things Claude Code said about it through the status line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_used_percentage: Option<f32>,
}

impl Workstream {
    fn new(project: ProjectId, name: Option<String>, agent: AgentKind) -> Self {
        let now = now_seconds();
        Self {
            id: WorkstreamId::generate(),
            project,
            name: clean_name(name),
            created_at: now,
            last_active_at: now,
            forked_from: None,
            resumable: false,
            agent,
            agent_session_id: None,
            model: None,
            context_used_percentage: None,
        }
    }

    /// The id the agent knows this conversation by, once it is known.
    ///
    /// Two ids, not one, because the agents differ on who gets to choose.
    /// Claude Code takes `--session-id`, so Beacon's id *is* the conversation's
    /// and is known before it exists. Codex names its own and says so in a
    /// hook, so between creating a conversation and its first turn there is a
    /// window with a name and no id — which is also the window in which there
    /// is nothing to resume, so nothing is lost by admitting it.
    ///
    /// The day Codex accepts an assigned id, this is the one place that has to
    /// learn about it.
    pub fn resume_id(&self) -> Option<&str> {
        match self.agent {
            AgentKind::Claude => Some(self.id.as_str()),
            AgentKind::Codex => self.agent_session_id.as_deref(),
        }
    }
}

/// Trims a name, and treats an empty one as no name at all.
fn clean_name(name: Option<String>) -> Option<String> {
    let name = name?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Every project's conversations, and which one it is in.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstreamBook {
    pub schema_version: u32,
    /// Most recently active first, so a project's list needs no sorting and the
    /// cap is applied by dropping from the tail.
    #[serde(default)]
    pub workstreams: Vec<Workstream>,
    /// Which conversation each project is currently in.
    ///
    /// Held here rather than worked out from the timestamps: "most recent" and
    /// "the one I am in" come apart the moment you look at an older one.
    #[serde(default)]
    pub current: BTreeMap<ProjectId, WorkstreamId>,
}

impl Default for WorkstreamBook {
    fn default() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            workstreams: Vec::new(),
            current: BTreeMap::new(),
        }
    }
}

impl WorkstreamBook {
    pub const SCHEMA_VERSION: u32 = 1;

    /// A project's conversations, most recently active first.
    pub fn for_project(&self, project: &ProjectId) -> Vec<&Workstream> {
        self.workstreams
            .iter()
            .filter(|stream| &stream.project == project)
            .collect()
    }

    pub fn get(&self, id: &WorkstreamId) -> Option<&Workstream> {
        self.workstreams.iter().find(|stream| &stream.id == id)
    }

    /// The conversation a project is in, if it is in one.
    pub fn current(&self, project: &ProjectId) -> Option<&Workstream> {
        let id = self.current.get(project)?;
        self.get(id)
    }

    /// Starts a new conversation and moves the project into it.
    pub fn start(
        &mut self,
        project: ProjectId,
        name: Option<String>,
        agent: AgentKind,
    ) -> Workstream {
        let stream = Workstream::new(project.clone(), name, agent);
        self.insert(stream.clone());
        self.current.insert(project.clone(), stream.id.clone());
        self.enforce_cap(&project);
        stream
    }

    /// Starts a conversation that continues another, and moves into it.
    ///
    /// Returns `None` for an id this book has never seen, rather than forking
    /// from nothing: Claude Code would be asked to resume a conversation that
    /// does not exist and would fail after the session had already been closed.
    pub fn fork(
        &mut self,
        project: &ProjectId,
        from: &WorkstreamId,
        name: Option<String>,
    ) -> Option<Workstream> {
        let parent = self.get(from)?;

        // A fork carries its parent's agent. There is no forking a Claude Code
        // conversation into Codex: what a fork is for is the history, and the
        // history is in the other program.
        let mut stream = Workstream::new(project.clone(), name, parent.agent);
        stream.forked_from = Some(from.clone());
        self.insert(stream.clone());
        self.current.insert(project.clone(), stream.id.clone());
        self.enforce_cap(project);
        Some(stream)
    }

    /// Moves a project into a conversation it already has.
    pub fn resume(&mut self, project: &ProjectId, id: &WorkstreamId) -> Option<Workstream> {
        let stream = self.get(id)?.clone();
        if &stream.project != project {
            return None;
        }

        self.current.insert(project.clone(), id.clone());
        self.touch(id);
        self.get(id).cloned()
    }

    /// Records that a conversation now exists, so the next start resumes it
    /// rather than trying to create one that is already there.
    ///
    /// Called for something that can only have happened inside a turn — a tool
    /// starting, a prompt submitted, a turn finishing, a window with tokens in
    /// it. Never for a session merely opening: that writes nothing.
    pub fn mark_resumable(&mut self, id: &WorkstreamId) -> bool {
        let Some(stream) = self.workstreams.iter_mut().find(|s| &s.id == id) else {
            return false;
        };
        let changed = !stream.resumable;
        stream.resumable = true;
        changed
    }

    /// Records the id an agent gave a conversation Beacon could not name.
    ///
    /// Returns whether anything changed, so a caller knows whether the book is
    /// worth writing. An id that arrives twice is not a change; one that
    /// *differs* from what was recorded replaces it and is worth saying out
    /// loud, because it means the conversation Beacon thought it was watching
    /// has been replaced by another.
    pub fn learn_session_id(&mut self, id: &WorkstreamId, reported: &str) -> bool {
        let Some(stream) = self.workstreams.iter_mut().find(|s| &s.id == id) else {
            return false;
        };
        if stream.agent_session_id.as_deref() == Some(reported) {
            return false;
        }
        if let Some(previous) = &stream.agent_session_id {
            tracing::warn!(
                workstream = %id,
                previous = %previous,
                reported = %reported,
                "a conversation reported a different id than the one it had"
            );
        }
        stream.agent_session_id = Some(reported.to_string());
        true
    }

    pub fn rename(&mut self, id: &WorkstreamId, name: Option<String>) -> bool {
        let Some(stream) = self.workstreams.iter_mut().find(|s| &s.id == id) else {
            return false;
        };
        stream.name = clean_name(name);
        true
    }

    /// Records that a conversation is being worked in.
    pub fn touch(&mut self, id: &WorkstreamId) {
        let now = now_seconds();
        if let Some(at) = self.workstreams.iter().position(|s| &s.id == id) {
            self.workstreams[at].last_active_at = now;
            // Most recently active first is a property of the list, so moving
            // it is how the property is kept.
            let stream = self.workstreams.remove(at);
            self.workstreams.insert(0, stream);
        }
    }

    /// Folds in what Claude Code said about a conversation through its status
    /// line.
    ///
    /// Matched on the session id, so a report from a Claude somebody started
    /// themselves — in a terminal, in another tool — is ignored rather than
    /// written onto whichever workstream happens to be current.
    pub fn observe(&mut self, report: &UsageReport) -> bool {
        let Some(session) = report.session_id.as_deref() else {
            return false;
        };
        let id = WorkstreamId(session.to_string());
        let Some(at) = self.workstreams.iter().position(|s| s.id == id) else {
            return false;
        };

        let stream = &mut self.workstreams[at];
        if let Some(model) = &report.model {
            stream.model = Some(model.clone());
        }
        if let Some(context) = report.context_used_percentage {
            stream.context_used_percentage = Some(context);
        }
        // A name the user set with `/rename`, which Beacon did not do and would
        // otherwise never learn about.
        if let Some(name) = &report.session_name {
            stream.name = clean_name(Some(name.clone()));
        }
        // Tokens in the window mean there has been an API response, which means
        // there is a conversation to resume. Claude Code reports both as zero
        // or absent before the first one.
        if report.context_used_tokens.is_some_and(|tokens| tokens > 0)
            || report.prompt_cache.is_some()
        {
            stream.resumable = true;
        }
        self.touch(&id);
        true
    }

    /// Drops a project's conversations, for when the project is removed.
    pub fn forget_project(&mut self, project: &ProjectId) -> usize {
        let before = self.workstreams.len();
        self.workstreams.retain(|stream| &stream.project != project);
        self.current.remove(project);
        before - self.workstreams.len()
    }

    fn insert(&mut self, stream: Workstream) {
        self.workstreams.insert(0, stream);
    }

    /// Drops a project's least recently active conversations past the cap.
    ///
    /// Never the current one: it may be the oldest in a project somebody has
    /// come back to after a long time, and dropping the row would lose the name
    /// of the conversation they are sitting in. It still costs a place, so the
    /// cap is a cap and not one more than a cap.
    fn enforce_cap(&mut self, project: &ProjectId) {
        let current = self
            .current
            .get(project)
            .filter(|id| {
                self.workstreams
                    .iter()
                    .any(|stream| &stream.project == project && &stream.id == *id)
            })
            .cloned();

        let budget = MAX_PER_PROJECT.saturating_sub(usize::from(current.is_some()));
        let mut kept = 0usize;

        self.workstreams.retain(|stream| {
            if &stream.project != project {
                return true;
            }
            if current.as_ref() == Some(&stream.id) {
                return true;
            }
            kept += 1;
            kept <= budget
        });
    }
}

/// The book on disk. One writer by construction: the daemon.
///
/// The same arrangement as the clip book, and for the same reason — a window
/// that has been open for a day must not be able to overwrite a conversation
/// started a minute ago by another one.
#[derive(Debug, Clone)]
pub struct WorkstreamStore {
    store: JsonStore,
}

impl WorkstreamStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store: JsonStore::new(path),
        }
    }

    pub fn default_path() -> std::path::PathBuf {
        crate::paths::default_config_dir().join("workstreams.json")
    }

    pub fn open_default() -> Self {
        Self::new(Self::default_path())
    }

    /// Reads the book, or an empty one on a fresh install.
    ///
    /// A file that cannot be read is logged and replaced rather than raised:
    /// refusing to start the daemon over a list of conversation names would
    /// trade every live session for it.
    pub fn load(&self) -> WorkstreamBook {
        match self.try_load() {
            Ok(book) => book,
            Err(err) => {
                tracing::warn!(error = %err, "could not read the workstream book; starting a new one");
                WorkstreamBook::default()
            }
        }
    }

    fn try_load(&self) -> Result<WorkstreamBook> {
        let book: WorkstreamBook = self.store.read()?;
        ensure_schema(
            self.store.path(),
            book.schema_version,
            WorkstreamBook::SCHEMA_VERSION,
        )?;
        Ok(book)
    }

    pub fn save(&self, book: &WorkstreamBook) -> Result<()> {
        self.store.write(book)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId("pj_x".into())
    }

    fn report(session: &str) -> UsageReport {
        UsageReport {
            session_id: Some(session.into()),
            ..UsageReport::unknown(project())
        }
    }

    #[test]
    fn a_claude_conversation_is_resumed_by_the_id_beacon_gave_it() {
        let mut book = WorkstreamBook::default();
        let stream = book.start(project(), None, AgentKind::Claude);

        // Beacon chose this id and passed it with `--session-id`, so there is
        // nothing to learn and nothing to wait for.
        assert_eq!(stream.resume_id(), Some(stream.id.as_str()));
        assert_eq!(stream.agent_session_id, None);
    }

    #[test]
    fn a_codex_conversation_has_nothing_to_resume_by_until_it_says_so() {
        let mut book = WorkstreamBook::default();
        let stream = book.start(project(), None, AgentKind::Codex);
        let id = stream.id.clone();

        // The window that only exists for an agent that names itself: Beacon
        // has a conversation and no id for it.
        assert_eq!(stream.resume_id(), None);

        assert!(book.learn_session_id(&id, "01a0a9d6-e991-70f0-8c36-aa613bd90216"));
        assert_eq!(
            book.get(&id).unwrap().resume_id(),
            Some("01a0a9d6-e991-70f0-8c36-aa613bd90216")
        );
    }

    #[test]
    fn an_id_reported_again_is_not_a_change_worth_writing_to_disk() {
        let mut book = WorkstreamBook::default();
        let id = book.start(project(), None, AgentKind::Codex).id;

        assert!(book.learn_session_id(&id, "abc"));
        assert!(
            !book.learn_session_id(&id, "abc"),
            "the same id twice changes nothing"
        );
        // A different one does, because the conversation is not the one Beacon
        // thought it was watching.
        assert!(book.learn_session_id(&id, "def"));
        assert_eq!(book.get(&id).unwrap().resume_id(), Some("def"));
    }

    #[test]
    fn learning_an_id_for_a_conversation_that_is_not_there_changes_nothing() {
        let mut book = WorkstreamBook::default();
        assert!(!book.learn_session_id(&WorkstreamId("nobody".into()), "abc"));
    }

    #[test]
    fn a_fork_belongs_to_the_same_agent_as_its_parent() {
        let mut book = WorkstreamBook::default();
        let parent = book.start(project(), None, AgentKind::Codex);

        let forked = book.fork(&project(), &parent.id, None).unwrap();

        // What a fork is for is the history, and the history lives inside the
        // other program: there is no forking Codex into Claude Code.
        assert_eq!(forked.agent, AgentKind::Codex);
    }

    #[test]
    fn a_book_written_before_agents_existed_reads_as_claude_and_stays_resumable() {
        // The migration, stated as the only thing that actually matters: a
        // conversation recorded by an older Beacon must still be resumable
        // after the upgrade. A default of anything but Claude, or a
        // `resume_id` that waited for a report that will never come, would
        // strand every conversation the user already had.
        let old = r#"{
            "schemaVersion": 1,
            "workstreams": [{
                "id": "0c40d249-ee07-4eac-ab8c-608e71f2c9e1",
                "project": "pj_x",
                "name": "payments-bug",
                "createdAt": 1,
                "lastActiveAt": 2,
                "resumable": true
            }],
            "current": { "pj_x": "0c40d249-ee07-4eac-ab8c-608e71f2c9e1" }
        }"#;

        let book: WorkstreamBook = serde_json::from_str(old).unwrap();
        let stream = book
            .current(&project())
            .expect("the project's conversation");

        assert_eq!(stream.agent, AgentKind::Claude);
        assert!(stream.resumable);
        assert_eq!(
            stream.resume_id(),
            Some("0c40d249-ee07-4eac-ab8c-608e71f2c9e1")
        );
        assert_eq!(stream.name.as_deref(), Some("payments-bug"));
    }

    #[test]
    fn a_new_workstream_gets_an_id_claude_code_would_accept() {
        // Hyphenated and 36 characters: what `--session-id <uuid>` takes. A
        // prefixed id like the rest of the domain uses would be rejected.
        let id = WorkstreamId::generate();
        assert_eq!(id.as_str().len(), 36);
        assert_eq!(id.as_str().matches('-').count(), 4);
        assert_ne!(id, WorkstreamId::generate());
    }

    #[test]
    fn starting_one_moves_the_project_into_it() {
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), Some("auth-refactor".into()), AgentKind::Claude);

        assert_eq!(book.current(&project()).unwrap().id, started.id);
        assert_eq!(
            book.current(&project()).unwrap().name.as_deref(),
            Some("auth-refactor")
        );
    }

    #[test]
    fn a_workstream_nobody_named_has_no_name() {
        // Not "untitled", not the project name. A name Beacon invented would be
        // indistinguishable from one the user chose.
        let mut book = WorkstreamBook::default();
        for blank in [None, Some(String::new()), Some("   ".into())] {
            let started = book.start(project(), blank, AgentKind::Claude);
            assert_eq!(started.name, None);
        }
    }

    #[test]
    fn names_are_trimmed_rather_than_taken_literally() {
        let mut book = WorkstreamBook::default();
        let started = book.start(
            project(),
            Some("  payments-bug \n".into()),
            AgentKind::Claude,
        );
        assert_eq!(started.name.as_deref(), Some("payments-bug"));
    }

    #[test]
    fn a_project_lists_its_own_conversations_most_recent_first() {
        let mut book = WorkstreamBook::default();
        let first = book.start(project(), Some("one".into()), AgentKind::Claude);
        let second = book.start(project(), Some("two".into()), AgentKind::Claude);
        book.start(
            ProjectId("pj_other".into()),
            Some("elsewhere".into()),
            AgentKind::Claude,
        );

        let listed = book.for_project(&project());
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.id);
        assert_eq!(listed[1].id, first.id);
    }

    #[test]
    fn working_in_one_moves_it_to_the_front() {
        let mut book = WorkstreamBook::default();
        let first = book.start(project(), Some("one".into()), AgentKind::Claude);
        book.start(project(), Some("two".into()), AgentKind::Claude);

        book.touch(&first.id);
        assert_eq!(book.for_project(&project())[0].id, first.id);
    }

    #[test]
    fn resuming_only_works_within_the_project_that_owns_it() {
        // The guard that stops a stray id from moving another project into a
        // conversation that was never its own.
        let mut book = WorkstreamBook::default();
        let mine = book.start(project(), Some("mine".into()), AgentKind::Claude);
        let other = ProjectId("pj_other".into());

        assert!(book.resume(&other, &mine.id).is_none());
        assert!(book.current(&other).is_none());
        assert!(book.resume(&project(), &mine.id).is_some());
    }

    #[test]
    fn a_fork_records_where_it_came_from_and_becomes_current() {
        let mut book = WorkstreamBook::default();
        let parent = book.start(project(), Some("dashboard".into()), AgentKind::Claude);
        let forked = book
            .fork(&project(), &parent.id, Some("dashboard-experiment".into()))
            .unwrap();

        assert_eq!(forked.forked_from.as_ref(), Some(&parent.id));
        assert_ne!(forked.id, parent.id);
        assert_eq!(book.current(&project()).unwrap().id, forked.id);
    }

    #[test]
    fn forking_from_nothing_is_refused_rather_than_invented() {
        // Otherwise Claude Code is asked to resume a conversation that does not
        // exist, after the session it was in has already been closed.
        let mut book = WorkstreamBook::default();
        let missing = WorkstreamId::generate();
        assert!(book.fork(&project(), &missing, None).is_none());
        assert!(book.current(&project()).is_none());
    }

    #[test]
    fn a_status_line_report_updates_the_conversation_it_names() {
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), Some("auth".into()), AgentKind::Claude);

        let observed = UsageReport {
            model: Some("Opus 5".into()),
            context_used_percentage: Some(38.0),
            ..report(started.id.as_str())
        };
        assert!(book.observe(&observed));

        let stream = book.get(&started.id).unwrap();
        assert_eq!(stream.model.as_deref(), Some("Opus 5"));
        assert_eq!(stream.context_used_percentage, Some(38.0));
    }

    #[test]
    fn a_report_from_a_claude_beacon_did_not_start_is_ignored() {
        // Someone's own terminal session reports through the same status line.
        // Writing it onto whichever workstream happened to be current would put
        // another conversation's context percentage on this one.
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), Some("auth".into()), AgentKind::Claude);

        assert!(!book.observe(&report("11111111-2222-3333-4444-555555555555")));
        assert_eq!(book.get(&started.id).unwrap().model, None);
    }

    #[test]
    fn a_report_with_no_session_id_changes_nothing() {
        let mut book = WorkstreamBook::default();
        book.start(project(), Some("auth".into()), AgentKind::Claude);
        assert!(!book.observe(&UsageReport::unknown(project())));
    }

    #[test]
    fn a_rename_done_inside_claude_is_learned_from_the_report() {
        // `/rename` is Claude Code's, not Beacon's. The status line is how
        // Beacon finds out it happened.
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), None, AgentKind::Claude);

        book.observe(&UsageReport {
            session_name: Some("payments-bug".into()),
            ..report(started.id.as_str())
        });

        assert_eq!(
            book.get(&started.id).unwrap().name.as_deref(),
            Some("payments-bug")
        );
    }

    #[test]
    fn a_project_keeps_a_readable_number_of_conversations() {
        let mut book = WorkstreamBook::default();
        for n in 0..MAX_PER_PROJECT + 10 {
            book.start(project(), Some(format!("stream-{n}")), AgentKind::Claude);
        }
        assert_eq!(book.for_project(&project()).len(), MAX_PER_PROJECT);
    }

    #[test]
    fn the_conversation_you_are_in_is_never_dropped_to_make_room() {
        let mut book = WorkstreamBook::default();
        let oldest = book.start(project(), Some("the-one-i-am-in".into()), AgentKind::Claude);

        for n in 0..MAX_PER_PROJECT + 5 {
            let started = book.start(project(), Some(format!("stream-{n}")), AgentKind::Claude);
            // Move back into the old one, so every later start has to consider
            // dropping it.
            book.resume(&project(), &oldest.id);
            let _ = started;
        }

        assert!(book.get(&oldest.id).is_some());
        assert_eq!(book.current(&project()).unwrap().id, oldest.id);
    }

    #[test]
    fn the_cap_is_per_project_rather_than_overall() {
        let mut book = WorkstreamBook::default();
        let other = ProjectId("pj_other".into());
        book.start(other.clone(), Some("theirs".into()), AgentKind::Claude);

        for n in 0..MAX_PER_PROJECT + 5 {
            book.start(project(), Some(format!("mine-{n}")), AgentKind::Claude);
        }

        assert_eq!(book.for_project(&other).len(), 1);
    }

    #[test]
    fn removing_a_project_takes_its_conversations_with_it() {
        let mut book = WorkstreamBook::default();
        book.start(project(), Some("one".into()), AgentKind::Claude);
        book.start(project(), Some("two".into()), AgentKind::Claude);
        book.start(
            ProjectId("pj_other".into()),
            Some("theirs".into()),
            AgentKind::Claude,
        );

        assert_eq!(book.forget_project(&project()), 2);
        assert!(book.current(&project()).is_none());
        assert_eq!(book.workstreams.len(), 1);
    }

    #[test]
    fn a_conversation_does_not_exist_until_something_has_been_said_in_it() {
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), Some("auth".into()), AgentKind::Claude);
        assert!(!book.get(&started.id).unwrap().resumable);

        assert!(book.mark_resumable(&started.id));
        assert!(book.get(&started.id).unwrap().resumable);
        // Saying so twice is not news.
        assert!(!book.mark_resumable(&started.id));
    }

    #[test]
    fn opening_a_session_and_typing_nothing_leaves_nothing_to_resume() {
        // Found the hard way: Beacon used to call a conversation resumable the
        // moment it started a process with its id. Claude Code writes nothing
        // until the first exchange, so the next start answered "No conversation
        // found with session ID" where the session should have been.
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), None, AgentKind::Claude);

        // A status line report from a session that has not had a turn: the
        // window is empty and there is no cache to speak of.
        book.observe(&UsageReport {
            context_used_tokens: Some(0),
            ..report(started.id.as_str())
        });

        assert!(!book.get(&started.id).unwrap().resumable);
    }

    #[test]
    fn a_window_with_tokens_in_it_proves_there_is_a_conversation() {
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), None, AgentKind::Claude);

        book.observe(&UsageReport {
            context_used_tokens: Some(12_400),
            ..report(started.id.as_str())
        });

        assert!(book.get(&started.id).unwrap().resumable);
    }

    #[test]
    fn a_cache_report_proves_it_too() {
        // Claude Code only describes the cache after the first API response.
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), None, AgentKind::Claude);

        book.observe(&UsageReport {
            prompt_cache: Some(crate::protocol::PromptCache {
                warm: Some(true),
                hit_ratio: None,
                expires_at: None,
                recache_tokens_if_cold: None,
                misses: None,
                expected_rebuilds: None,
            }),
            ..report(started.id.as_str())
        });

        assert!(book.get(&started.id).unwrap().resumable);
    }

    #[test]
    fn whether_a_conversation_exists_survives_the_daemon() {
        // The flag decides `--session-id` against `--resume`, and a daemon that
        // came back and guessed would meet one refusal or the other.
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), None, AgentKind::Claude);
        book.mark_resumable(&started.id);

        let line = serde_json::to_string(&book).unwrap();
        let back: WorkstreamBook = serde_json::from_str(&line).unwrap();
        assert!(back.get(&started.id).unwrap().resumable);
    }

    #[test]
    fn a_book_survives_a_round_trip_through_disk() {
        let mut book = WorkstreamBook::default();
        let started = book.start(project(), Some("auth-refactor".into()), AgentKind::Claude);
        book.fork(&project(), &started.id, None);

        let line = serde_json::to_string(&book).unwrap();
        let back: WorkstreamBook = serde_json::from_str(&line).unwrap();

        assert_eq!(back.workstreams.len(), 2);
        assert_eq!(
            back.current(&project()).unwrap().forked_from,
            Some(started.id)
        );
        // A conversation nobody named stays nameless across the round trip.
        assert!(!line.contains("\"name\":null"), "got {line}");
    }

    #[test]
    fn an_unnamed_conversation_can_still_be_shown_as_something() {
        let id = WorkstreamId("b57bf9d0-8020-4275-a060-a521d289beae".into());
        assert_eq!(id.short(), "b57bf9d0");
    }
}
