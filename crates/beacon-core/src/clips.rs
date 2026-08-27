//! The clip book: things Claude produced for you to copy somewhere else.
//!
//! A clip is not a note. Nothing here is editable, searchable or organised into
//! folders, because the entire life of a clip is: Claude makes one, you press
//! copy, you paste it into the thing that actually wanted it — a shell, a
//! browser, a mail client. An environment variable, a command, the body of an
//! email. Anything that wants to be kept belongs in a file, and Claude can
//! already write files.
//!
//! That shape is what makes the feature cheap: no editor, no conflict
//! resolution, no sync. One writer (the daemon), an append, and a cap.

use serde::{Deserialize, Serialize};

use crate::domain::{ClipId, ProjectId};
use crate::error::{CoreError, Result};
use crate::store::{JsonStore, ensure_schema};

/// How many clips are kept before the oldest fall off the end.
///
/// A cap rather than "everything forever": this is a scratch surface, and a
/// list nobody can find anything in is the same as no list. Two hundred is far
/// more than the handful anybody has open questions about, and small enough
/// that the whole book is one cheap read at startup.
pub const MAX_CLIPS: usize = 200;

/// The longest body a clip may carry, in bytes.
///
/// Generous for an email and absurd for a command, which is the point: the
/// limit exists so that a Claude which decides to "send you the file" gets a
/// clear refusal it can act on, rather than quietly turning the drawer into a
/// document store. Anything bigger genuinely wants to be a file.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// The longest title, in bytes. A title is a label in a list, not a summary.
pub const MAX_TITLE_BYTES: usize = 200;

/// What a clip is, so the drawer can label it and pick a typeface.
///
/// Deliberately four. The kind exists to answer "is this something I paste into
/// a shell, or something I paste into a person?" — a command and a variable are
/// monospaced and must not be wrapped mid-token; an email is prose. Anything
/// finer would be a taxonomy nobody maintains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipKind {
    /// No claim beyond "you asked for this to copy".
    #[default]
    Text,
    /// Meant to be pasted into a shell.
    Command,
    /// A key and value, or a block of them — a `.env` fragment.
    Variable,
    /// Prose written to be sent to a person.
    Email,
}

impl ClipKind {
    /// Whether the body should be shown in a monospaced face, unwrapped.
    ///
    /// Wrapping a command is not a cosmetic problem: a line break lands in the
    /// clipboard's neighbour, the terminal, as a return.
    pub fn is_literal(self) -> bool {
        matches!(self, Self::Command | Self::Variable)
    }
}

/// One thing to copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: ClipId,
    /// Which project's session produced it. Always known: the only way a clip
    /// is created is through a session Beacon started, and Beacon tells that
    /// session who it is.
    pub project: ProjectId,
    /// A few words naming it, so the list is readable without opening anything.
    pub title: String,
    /// Exactly what the copy button puts on the clipboard.
    ///
    /// Never reformatted, never trimmed beyond its outer whitespace, never
    /// syntax-highlighted into something that copies differently from what it
    /// looks like. A clip that pastes differently than it reads is worse than
    /// no clip at all.
    pub body: String,
    #[serde(default)]
    pub kind: ClipKind,
    /// Unix seconds.
    pub created_at: i64,
}

impl Clip {
    /// Builds a clip, refusing one that could not be useful.
    ///
    /// The refusals travel back to Claude as a tool error, which is the one
    /// audience that can do something about them: it can shorten the body, or
    /// write a file instead.
    pub fn new(
        project: ProjectId,
        title: impl Into<String>,
        body: impl Into<String>,
        kind: ClipKind,
        created_at: i64,
    ) -> Result<Self> {
        let title = title.into().trim().to_string();
        // Only the outer whitespace: the indentation inside a block is part of
        // what gets pasted.
        let body = body.into().trim_matches('\n').to_string();

        if body.trim().is_empty() {
            return Err(CoreError::invalid("a clip needs something to copy"));
        }
        if body.len() > MAX_BODY_BYTES {
            return Err(CoreError::invalid(format!(
                "that is {} bytes, and a clip holds at most {MAX_BODY_BYTES}. Something this \
                 large wants to be a file, not something to paste.",
                body.len()
            )));
        }
        if title.is_empty() {
            return Err(CoreError::invalid("a clip needs a title to be findable"));
        }
        if title.len() > MAX_TITLE_BYTES {
            return Err(CoreError::invalid(format!(
                "a clip title is a label, not a summary: at most {MAX_TITLE_BYTES} bytes"
            )));
        }

        Ok(Self {
            id: ClipId::generate(),
            project,
            title,
            body,
            kind,
            created_at,
        })
    }
}

/// Unix seconds now, or zero if the clock is before the epoch.
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}

/// Every clip, newest first.
///
/// Persisted, because the daemon stops when nothing is running and a history
/// that empties itself every five idle minutes is not a history. Stored in
/// plain JSON alongside the rest of Beacon's configuration: it is protected by
/// the same thing that protects `workspaces.json`, which is the account. Worth
/// saying out loud because clips carry drafted email and environment values —
/// which is exactly why the drawer has a way to empty it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipBook {
    pub schema_version: u32,
    /// Newest first, so the drawer renders in order without sorting and the cap
    /// is applied by truncating the tail.
    #[serde(default)]
    pub clips: Vec<Clip>,
}

impl Default for ClipBook {
    fn default() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            clips: Vec::new(),
        }
    }
}

impl ClipBook {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// Files a clip at the front, dropping the oldest past the cap.
    pub fn add(&mut self, clip: Clip) {
        self.clips.insert(0, clip);
        self.clips.truncate(MAX_CLIPS);
    }

    /// Forgets one clip, or every clip when told `None`.
    ///
    /// Returns how many went, so the caller can tell "already gone" from
    /// "never existed" without a second read.
    pub fn forget(&mut self, id: Option<&ClipId>) -> usize {
        match id {
            Some(id) => {
                let before = self.clips.len();
                self.clips.retain(|clip| &clip.id != id);
                before - self.clips.len()
            }
            None => std::mem::take(&mut self.clips).len(),
        }
    }

    /// Drops every clip belonging to a project, for when it is removed.
    pub fn forget_project(&mut self, project: &ProjectId) -> usize {
        let before = self.clips.len();
        self.clips.retain(|clip| &clip.project != project);
        before - self.clips.len()
    }
}

/// The clip book on disk, owned by whoever is holding it.
///
/// One writer by construction: the daemon. The window never writes this file,
/// it asks the daemon — which means there is no merge, no last-write-wins, and
/// no window that has been open for a day quietly overwriting a clip made a
/// minute ago by another one.
#[derive(Debug, Clone)]
pub struct ClipStore {
    store: JsonStore,
}

impl ClipStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            store: JsonStore::new(path),
        }
    }

    pub fn default_path() -> std::path::PathBuf {
        crate::paths::default_config_dir().join("clips.json")
    }

    pub fn open_default() -> Self {
        Self::new(Self::default_path())
    }

    /// Reads the book, or an empty one on a fresh install.
    ///
    /// A file that cannot be parsed is *not* an error the caller has to handle:
    /// refusing to start the daemon because a scratch list is malformed would
    /// trade every live session for a list of things to paste. It is logged and
    /// replaced.
    pub fn load(&self) -> ClipBook {
        match self.try_load() {
            Ok(book) => book,
            Err(err) => {
                tracing::warn!(error = %err, "could not read the clip book; starting a new one");
                ClipBook::default()
            }
        }
    }

    fn try_load(&self) -> Result<ClipBook> {
        let book: ClipBook = self.store.read()?;
        ensure_schema(
            self.store.path(),
            book.schema_version,
            ClipBook::SCHEMA_VERSION,
        )?;
        Ok(book)
    }

    pub fn save(&self, book: &ClipBook) -> Result<()> {
        self.store.write(book)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId("pj_x".into())
    }

    fn clip(title: &str) -> Clip {
        Clip::new(project(), title, "value", ClipKind::Text, 0).unwrap()
    }

    #[test]
    fn a_clip_keeps_its_body_exactly_as_it_will_be_pasted() {
        // Inner indentation is part of what gets copied; only the blank lines
        // wrapping the block are the model's formatting rather than the user's.
        let clip = Clip::new(
            project(),
            "env",
            "\n\nFOO=1\n  BAR=2\n\n",
            ClipKind::Variable,
            0,
        )
        .unwrap();
        assert_eq!(clip.body, "FOO=1\n  BAR=2");
    }

    #[test]
    fn a_clip_with_nothing_to_copy_is_refused() {
        assert!(Clip::new(project(), "empty", "   \n ", ClipKind::Text, 0).is_err());
    }

    #[test]
    fn a_clip_without_a_label_is_refused() {
        assert!(Clip::new(project(), "  ", "something", ClipKind::Text, 0).is_err());
    }

    #[test]
    fn a_body_too_large_to_be_worth_pasting_is_refused_with_a_reason() {
        let huge = "x".repeat(MAX_BODY_BYTES + 1);
        let err = Clip::new(project(), "dump", huge, ClipKind::Text, 0).unwrap_err();
        // The message is read by Claude, so it has to say what to do instead.
        assert!(err.to_string().contains("file"), "{err}");
    }

    #[test]
    fn the_newest_clip_is_first() {
        let mut book = ClipBook::default();
        book.add(clip("first"));
        book.add(clip("second"));
        assert_eq!(book.clips()[0].title, "second");
    }

    #[test]
    fn the_oldest_clips_fall_off_the_end() {
        let mut book = ClipBook::default();
        for index in 0..MAX_CLIPS + 10 {
            book.add(clip(&format!("clip {index}")));
        }
        assert_eq!(book.clips().len(), MAX_CLIPS);
        assert_eq!(book.clips()[0].title, format!("clip {}", MAX_CLIPS + 9));
    }

    #[test]
    fn forgetting_one_leaves_the_rest() {
        let mut book = ClipBook::default();
        book.add(clip("keep"));
        book.add(clip("drop"));
        let target = book.clips()[0].id.clone();

        assert_eq!(book.forget(Some(&target)), 1);
        assert_eq!(book.clips().len(), 1);
        assert_eq!(book.clips()[0].title, "keep");
        // Forgetting it again is not an error, it is nothing to do.
        assert_eq!(book.forget(Some(&target)), 0);
    }

    #[test]
    fn forgetting_everything_empties_the_book() {
        let mut book = ClipBook::default();
        book.add(clip("a"));
        book.add(clip("b"));
        assert_eq!(book.forget(None), 2);
        assert!(book.clips().is_empty());
    }

    #[test]
    fn a_removed_project_takes_its_clips_with_it() {
        let mut book = ClipBook::default();
        book.add(clip("mine"));
        book.add(
            Clip::new(
                ProjectId("pj_other".into()),
                "theirs",
                "v",
                ClipKind::Text,
                0,
            )
            .unwrap(),
        );

        assert_eq!(book.forget_project(&project()), 1);
        assert_eq!(book.clips().len(), 1);
        assert_eq!(book.clips()[0].title, "theirs");
    }

    #[test]
    fn commands_and_variables_are_the_ones_that_must_not_be_wrapped() {
        assert!(ClipKind::Command.is_literal());
        assert!(ClipKind::Variable.is_literal());
        assert!(!ClipKind::Email.is_literal());
        assert!(!ClipKind::Text.is_literal());
    }

    #[test]
    fn a_book_survives_a_round_trip_through_disk() {
        let dir = std::env::temp_dir().join(format!("beacon-clips-{}", uuid::Uuid::new_v4()));
        let store = ClipStore::new(dir.join("clips.json"));

        // Nothing written yet is an empty book, not a failure.
        assert!(store.load().clips().is_empty());

        let mut book = ClipBook::default();
        book.add(clip("only"));
        store.save(&book).unwrap();

        let back = store.load();
        assert_eq!(back.clips().len(), 1);
        assert_eq!(back.clips()[0].title, "only");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_clip_book_that_cannot_be_read_is_replaced_rather_than_fatal() {
        // Losing the scratch list is a nuisance; refusing to start the daemon
        // over it would lose every running session.
        let dir = std::env::temp_dir().join(format!("beacon-clips-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clips.json");
        std::fs::write(&path, b"{ not json").unwrap();

        assert!(ClipStore::new(&path).load().clips().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
