use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::files::resolve_within;

/// How a path stands in the index or the working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileState {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Ignored,
    Conflicted,
}

impl FileState {
    fn from_code(code: char) -> Self {
        match code {
            'M' => Self::Modified,
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            'U' => Self::Conflicted,
            '?' => Self::Untracked,
            '!' => Self::Ignored,
            _ => Self::Unmodified,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitEntry {
    /// Relative to the repository root, as git reports it.
    pub path: String,
    /// Where a rename or copy came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    /// What is staged for the next commit.
    pub staged: FileState,
    /// What has changed since, in the working tree.
    pub unstaged: FileState,
}

impl GitEntry {
    pub fn is_untracked(&self) -> bool {
        self.unstaged == FileState::Untracked
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// `None` when the head is detached.
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// True before the first commit, when there is no HEAD to diff against.
    pub unborn: bool,
    pub entries: Vec<GitEntry>,
}

/// Whether a folder is a git repository at all.
pub fn is_repository(root: &Path) -> bool {
    root.join(".git").exists()
}

/// Runs git in a repository and returns its stdout.
///
/// The environment is hardened rather than inherited wholesale: git must never
/// stop to ask for a password, because there is no terminal here for it to ask
/// on and it would simply hang.
fn run(root: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(args)
        // Fail instead of blocking on a credential prompt.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .env("GIT_OPTIONAL_LOCKS", "0")
        // A pager would never exit.
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat");

    let output = command
        .output()
        .map_err(|err| CoreError::session("could not run git", err))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if message.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            message
        };
        return Err(CoreError::invalid(if message.is_empty() {
            format!("git {} failed", args.first().unwrap_or(&""))
        } else {
            message
        }));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The repository's current state: branch, tracking position, and what changed.
pub fn status(root: &Path) -> Result<GitStatus> {
    let raw = run(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--branch",
            "--untracked-files=all",
        ],
    )?;
    Ok(parse_status(&raw))
}

/// Parses `git status --porcelain=v1 -z --branch`.
///
/// Split out from running git so the format — which has more corners than it
/// looks — can be tested without a repository for each one.
pub fn parse_status(raw: &str) -> GitStatus {
    let mut status = GitStatus {
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        unborn: false,
        entries: Vec::new(),
    };

    // Records are NUL-separated, which is what makes paths containing spaces,
    // quotes or newlines safe to read.
    let mut records = raw.split('\0').filter(|record| !record.is_empty());

    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("## ") {
            parse_branch_header(header, &mut status);
            continue;
        }

        let mut chars = record.chars();
        let (Some(staged), Some(unstaged)) = (chars.next(), chars.next()) else {
            continue;
        };
        let path = record.get(3..).unwrap_or_default().to_string();
        if path.is_empty() {
            continue;
        }

        // A rename or copy is followed by a second record: where it came from.
        let original_path = if staged == 'R' || staged == 'C' || unstaged == 'R' || unstaged == 'C'
        {
            records.next().map(str::to_string)
        } else {
            None
        };

        status.entries.push(GitEntry {
            path,
            original_path,
            staged: FileState::from_code(staged),
            unstaged: FileState::from_code(unstaged),
        });
    }

    status
}

/// Reads `main...origin/main [ahead 1, behind 2]` and its several variants.
fn parse_branch_header(header: &str, status: &mut GitStatus) {
    // Before the first commit git says "No commits yet on main".
    let header = match header.strip_prefix("No commits yet on ") {
        Some(rest) => {
            status.unborn = true;
            rest
        }
        None => header,
    };

    if header.starts_with("HEAD (no branch)") {
        return;
    }

    let (names, tracking) = match header.split_once(" [") {
        Some((names, tracking)) => (names, Some(tracking.trim_end_matches(']'))),
        None => (header, None),
    };

    match names.split_once("...") {
        Some((branch, upstream)) => {
            status.branch = Some(branch.trim().to_string());
            status.upstream = Some(upstream.trim().to_string());
        }
        None => status.branch = Some(names.trim().to_string()),
    }

    let Some(tracking) = tracking else { return };
    for part in tracking.split(", ") {
        if let Some(count) = part.strip_prefix("ahead ") {
            status.ahead = count.trim().parse().unwrap_or(0);
        } else if let Some(count) = part.strip_prefix("behind ") {
            status.behind = count.trim().parse().unwrap_or(0);
        }
    }
}

/// The diff for one path, staged or not.
///
/// An untracked file has nothing to diff against, so it is compared with
/// nothing — which is how git itself renders a wholly new file.
pub fn diff(root: &Path, path: &str, staged: bool, untracked: bool) -> Result<String> {
    let absolute = resolve_within(root, path)?;

    if untracked {
        let target = absolute.to_string_lossy().into_owned();
        // `--no-index` exits 1 when the files differ, which is the normal case
        // here, so its status is not a failure to report.
        let output = Command::new("git")
            .current_dir(root)
            .args([
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-index",
                "--",
                "/dev/null",
                &target,
            ])
            .env("GIT_PAGER", "cat")
            .output()
            .map_err(|err| CoreError::session("could not run git", err))?;
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);
    run(root, &args)
}

pub fn stage(root: &Path, path: &str) -> Result<()> {
    resolve_within(root, path)?;
    run(root, &["add", "--", path]).map(|_| ())
}

/// Takes a path back out of the index, leaving the file alone.
///
/// Before the first commit there is no HEAD to restore from, so `git restore
/// --staged` fails outright. Dropping the path from the index is the equivalent
/// there, and a brand-new repository is exactly when someone is most likely to
/// stage the wrong thing.
pub fn unstage(root: &Path, path: &str) -> Result<()> {
    resolve_within(root, path)?;
    if has_head(root) {
        run(root, &["restore", "--staged", "--", path]).map(|_| ())
    } else {
        run(root, &["rm", "--cached", "-r", "--quiet", "--", path]).map(|_| ())
    }
}

/// Whether the repository has a commit yet.
fn has_head(root: &Path) -> bool {
    run(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok()
}

pub fn stage_all(root: &Path) -> Result<()> {
    run(root, &["add", "--all"]).map(|_| ())
}

/// Commits what is staged. Never `-a`: what you staged is what you commit.
pub fn commit(root: &Path, message: &str) -> Result<String> {
    if message.trim().is_empty() {
        return Err(CoreError::invalid("a commit needs a message"));
    }
    run(root, &["commit", "-m", message])
}

pub fn push(root: &Path) -> Result<String> {
    run(root, &["push"])
}

/// Pulls, refusing anything that is not a fast-forward.
///
/// A merge or a rebase started from a side panel with no way to see or resolve
/// a conflict would leave the repository somewhere the user did not ask to be.
pub fn pull(root: &Path) -> Result<String> {
    run(root, &["pull", "--ff-only"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_branch_with_an_upstream_and_its_position() {
        let status = parse_status("## main...origin/main [ahead 1, behind 2]\0");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, 1);
        assert_eq!(status.behind, 2);
    }

    #[test]
    fn reads_a_branch_with_no_upstream() {
        let status = parse_status("## feature/thing\0");
        assert_eq!(status.branch.as_deref(), Some("feature/thing"));
        assert_eq!(status.upstream, None);
        assert_eq!(status.ahead, 0);
    }

    #[test]
    fn a_detached_head_has_no_branch() {
        let status = parse_status("## HEAD (no branch)\0");
        assert_eq!(status.branch, None);
    }

    #[test]
    fn a_repository_before_its_first_commit_is_marked_unborn() {
        let status = parse_status("## No commits yet on main\0?? README.md\0");
        assert!(status.unborn);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.entries.len(), 1);
    }

    #[test]
    fn separates_what_is_staged_from_what_is_not() {
        let status = parse_status("## main\0MM src/app.ts\0 M src/other.ts\0A  new.ts\0");
        assert_eq!(status.entries.len(), 3);

        assert_eq!(status.entries[0].staged, FileState::Modified);
        assert_eq!(status.entries[0].unstaged, FileState::Modified);

        assert_eq!(status.entries[1].staged, FileState::Unmodified);
        assert_eq!(status.entries[1].unstaged, FileState::Modified);

        assert_eq!(status.entries[2].staged, FileState::Added);
        assert_eq!(status.entries[2].unstaged, FileState::Unmodified);
    }

    #[test]
    fn a_rename_carries_where_it_came_from() {
        // In -z mode the original path is its own record, not quoted inline.
        let status = parse_status("## main\0R  new/name.ts\0old/name.ts\0 M other.ts\0");
        assert_eq!(status.entries.len(), 2);
        assert_eq!(status.entries[0].path, "new/name.ts");
        assert_eq!(
            status.entries[0].original_path.as_deref(),
            Some("old/name.ts")
        );
        // The record after the original path is a normal entry again.
        assert_eq!(status.entries[1].path, "other.ts");
    }

    #[test]
    fn paths_with_spaces_survive_because_records_are_nul_separated() {
        let status = parse_status("## main\0?? some file with spaces.txt\0");
        assert_eq!(status.entries[0].path, "some file with spaces.txt");
        assert!(status.entries[0].is_untracked());
    }

    #[test]
    fn a_conflict_is_reported_as_one() {
        let status = parse_status("## main\0UU merged.ts\0");
        assert_eq!(status.entries[0].staged, FileState::Conflicted);
        assert_eq!(status.entries[0].unstaged, FileState::Conflicted);
    }

    #[test]
    fn a_clean_repository_has_no_entries() {
        let status = parse_status("## main...origin/main\0");
        assert!(status.entries.is_empty());
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
    }

    #[test]
    fn an_empty_message_is_not_a_commit() {
        let dir = tempfile::tempdir().unwrap();
        assert!(commit(dir.path(), "   ").is_err());
    }
}
