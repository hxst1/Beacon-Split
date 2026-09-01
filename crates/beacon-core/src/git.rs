use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::files::resolve_within;
use crate::tools::resolve_program;

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
    /// Unmerged: the path has conflict stages in the index rather than one
    /// entry. Carried explicitly because the two letters alone do not say so —
    /// `AA` and `DD` are conflicts that never mention `U`.
    pub conflicted: bool,
}

impl GitEntry {
    pub fn is_untracked(&self) -> bool {
        self.unstaged == FileState::Untracked
    }
}

/// Whether a porcelain status pair means "unmerged".
///
/// Git lists these seven combinations itself, and they are the only ones: any
/// other pair with the same letters is an ordinary staged or working-tree
/// change. Getting this wrong in either direction is expensive — a conflict
/// treated as an ordinary change can be resolved by accident, and an ordinary
/// change treated as a conflict cannot be staged at all.
fn is_unmerged_pair(staged: char, unstaged: char) -> bool {
    matches!(
        (staged, unstaged),
        ('D', 'D') | ('A', 'U') | ('U', 'D') | ('U', 'A') | ('D', 'U') | ('A', 'A') | ('U', 'U')
    )
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

/// How long an ordinary git command gets before Beacon stops waiting for it.
///
/// Everything Beacon asks for here is milliseconds on a healthy repository, so
/// half a minute already means something is wrong — an index lock nobody is
/// releasing, a network filesystem that stopped answering. Waiting on it
/// forever leaves the panel with every control disabled and no way back.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);

/// The same rule for the commands that are legitimately slow.
///
/// A commit runs the repository's own hooks and push and pull talk to a
/// network, so minutes are normal. What this catches is the one that never
/// ends: a pinentry dialog, or a credential helper waiting for an answer that
/// cannot be given here.
const SLOW_TIMEOUT: Duration = Duration::from_secs(180);

/// How often a running command is checked on. Short enough not to be felt on a
/// status read, long enough to cost nothing while a push runs.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// The git Beacon runs.
///
/// Resolved through the user's login shell rather than trusted to PATH: a
/// Finder-launched macOS app inherits `/usr/bin:/bin:/usr/sbin:/sbin` and
/// nothing else, so a Homebrew-only git would be reported as present by the
/// requirements check — which does resolve it properly — and then fail on
/// every call. Falling back to the bare name keeps a machine where resolution
/// somehow fails no worse off than before.
fn git_program() -> &'static Path {
    static RESOLVED: OnceLock<PathBuf> = OnceLock::new();
    RESOLVED.get_or_init(|| resolve_program("git").unwrap_or_else(|| PathBuf::from("git")))
}

/// What a finished git command left behind.
struct GitOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl GitOutput {
    /// The command's news, from wherever it chose to write it.
    ///
    /// Push and pull say almost everything on stderr — "Everything up-to-date"
    /// included — so reporting stdout alone tells the user nothing happened
    /// when something did.
    fn report(&self) -> String {
        let mut text = self.stderr.trim().to_string();
        let out = self.stdout.trim();
        if !out.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(out);
        }
        text
    }
}

/// A git command with the environment Beacon insists on.
///
/// The environment is hardened rather than inherited wholesale: git must never
/// stop to ask for a password, because there is no terminal here for it to ask
/// on and it would simply hang.
fn git_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(git_program());
    command
        .current_dir(root)
        // Housekeeping git would otherwise fork off after a commit or a fetch
        // outlives the command that started it and inherits its stdout, so
        // reading that pipe to the end would wait for the repack rather than
        // for git — past the timeout, which only watches the child it spawned.
        .args(["-c", "gc.auto=0"])
        .args(args)
        // Fail instead of blocking on a credential prompt.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS", "")
        .env("GIT_OPTIONAL_LOCKS", "0")
        // A pager would never exit.
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat");
    command
}

/// Runs a command to completion, or gives up on it.
///
/// Output is drained on its own threads because git can write more than a pipe
/// will hold, and a parent that watches for exit while the pipe is full waits
/// forever on a child that is waiting to be read. On the timeout path those
/// threads are abandoned rather than joined: whatever is holding the command
/// up may be holding the pipe open too, and the point of the timeout is to
/// come back.
fn execute(command: &mut Command, what: &str, limit: Duration) -> Result<GitOutput> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| CoreError::session("could not run git", err))?;

    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Err(err) => return Err(CoreError::session("could not run git", err)),
            Ok(None) => {}
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            tracing::warn!(command = what, "a git command would not finish; stopped it");
            return Err(CoreError::invalid(format!(
                "git {what} did not finish within {} seconds and was stopped. It is probably \
                 waiting for something it cannot ask for here — a passphrase, a credential, a \
                 hook of your own. Running it in a terminal will say what it wants.",
                limit.as_secs()
            )));
        }

        std::thread::sleep(POLL_INTERVAL);
    };

    Ok(GitOutput {
        status,
        stdout: collect(stdout),
        stderr: collect(stderr),
    })
}

fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = pipe.read_to_end(&mut buffer);
        buffer
    })
}

fn collect(reader: Option<std::thread::JoinHandle<Vec<u8>>>) -> String {
    let bytes = reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Turns a failed command into the error the user sees, preferring whatever
/// git said over a message of our own invention.
fn failure(output: &GitOutput, args: &[&str]) -> CoreError {
    let message = output.report();
    CoreError::invalid(if message.is_empty() {
        format!("git {} failed", args.first().unwrap_or(&""))
    } else {
        message
    })
}

/// Runs git in a repository and returns its stdout.
fn run(root: &Path, args: &[&str]) -> Result<String> {
    run_within(root, args, LOCAL_TIMEOUT)
}

fn run_within(root: &Path, args: &[&str], limit: Duration) -> Result<String> {
    let what = args.first().copied().unwrap_or("");
    let output = execute(&mut git_command(root, args), what, limit)?;
    if !output.status.success() {
        return Err(failure(&output, args));
    }
    Ok(output.stdout)
}

/// Runs git and reports what it said, for the commands whose whole point is
/// what they say. A command that succeeds silently still gets a sentence, so
/// the panel never has to show nothing at all in answer to a click.
fn run_reporting(root: &Path, args: &[&str], silence_means: &str) -> Result<String> {
    let what = args.first().copied().unwrap_or("");
    let output = execute(&mut git_command(root, args), what, SLOW_TIMEOUT)?;
    if !output.status.success() {
        return Err(failure(&output, args));
    }

    let report = output.report();
    Ok(if report.is_empty() {
        silence_means.to_string()
    } else {
        report
    })
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
            conflicted: is_unmerged_pair(staged, unstaged),
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

/// Every file git knows about or would add, honouring the ignore rules.
///
/// `--others --exclude-standard` includes untracked files that are not ignored,
/// so a file created a moment ago is findable without a commit.
pub fn list_files(root: &Path) -> Result<Vec<String>> {
    let raw = run(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )?;

    let mut files: Vec<String> = raw
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// The diff for one path, staged or not.
///
/// An untracked file has nothing to diff against, so it is compared with
/// nothing — which is how git itself renders a wholly new file.
pub fn diff(root: &Path, path: &str, staged: bool, untracked: bool) -> Result<String> {
    resolve_within(root, path)?;

    if untracked {
        // The path stays relative even though it was just resolved: git prints
        // whatever it was given into the `+++ b/…` header, and an absolute one
        // puts the reader's home directory on screen instead of the file they
        // clicked on.
        let args = [
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-index",
            "--",
            "/dev/null",
            path,
        ];
        let output = execute(&mut git_command(root, &args), "diff", LOCAL_TIMEOUT)?;

        // `--no-index` exits 1 when the files differ, which is the normal case
        // here, so its status alone cannot say whether this worked. A file it
        // could not read is also an exit of 1 — with a reason on stderr, which
        // is silent on success.
        let complaint = output.stderr.trim();
        if !complaint.is_empty() || !matches!(output.status.code(), Some(0 | 1)) {
            return Err(failure(&output, &args));
        }
        return Ok(output.stdout);
    }

    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(path);
    run(root, &args)
}

/// Stages a path.
///
/// Staging an unmerged path is how git is told a conflict is resolved, which
/// is a different act from staging a change and is refused while the markers
/// are still in the file. Beacon does not resolve conflicts — it would be
/// offering a button that commits `<<<<<<<` into the user's history.
pub fn stage(root: &Path, path: &str) -> Result<()> {
    let absolute = resolve_within(root, path)?;

    if is_unmerged(root, path) && file_has_conflict_markers(&absolute) {
        return Err(CoreError::invalid(format!(
            "{path} still has conflict markers in it. Resolve the conflict first — the terminal \
             panel is the place for that — and Beacon will take it from there."
        )));
    }

    run(root, &["add", "--", path]).map(|_| ())
}

/// Takes a path back out of the index, leaving the file alone.
///
/// Before the first commit there is no HEAD to restore from, so `git restore
/// --staged` fails outright. Dropping the path from the index is the equivalent
/// there, and a brand-new repository is exactly when someone is most likely to
/// stage the wrong thing.
///
/// An unmerged path is refused outright. `git restore --staged` on one exits
/// zero and quietly collapses the conflict stages into the HEAD version, which
/// marks the conflict resolved while the file on disk still has the markers in
/// it — a silent loss of the other side of a merge, one click from a commit.
pub fn unstage(root: &Path, path: &str) -> Result<()> {
    resolve_within(root, path)?;

    if is_unmerged(root, path) {
        return Err(CoreError::invalid(format!(
            "{path} is part of an unresolved merge, so there is nothing to unstage. Unstaging it \
             would mark the conflict resolved with both sides still in the file."
        )));
    }

    if has_head(root) {
        run(root, &["restore", "--staged", "--", path]).map(|_| ())
    } else {
        run(root, &["rm", "--cached", "-r", "--quiet", "--", path]).map(|_| ())
    }
}

/// Whether git holds conflict stages for this path rather than one entry.
///
/// Asked of git rather than inferred from the status letters, because this
/// guards an operation that cannot be undone by looking again.
fn is_unmerged(root: &Path, path: &str) -> bool {
    run(root, &["ls-files", "--unmerged", "-z", "--", path])
        .map(|listed| !listed.trim().is_empty())
        .unwrap_or(false)
}

fn file_has_conflict_markers(path: &Path) -> bool {
    std::fs::read(path)
        .map(|bytes| contains_conflict_markers(&String::from_utf8_lossy(&bytes)))
        .unwrap_or(false)
}

/// Whether text still carries the markers git writes into a conflicted file.
///
/// All three are required and each must start its own line, so a file that
/// merely talks about conflicts — a changelog, this project's own tests —
/// does not become impossible to stage.
pub fn contains_conflict_markers(text: &str) -> bool {
    let ours = |line: &str| line == "<<<<<<<" || line.starts_with("<<<<<<< ");
    let theirs = |line: &str| line == ">>>>>>>" || line.starts_with(">>>>>>> ");

    let mut lines = text.lines();
    lines.any(ours) && lines.any(|line| line == "=======") && lines.any(theirs)
}

/// Whether the repository has a commit yet.
fn has_head(root: &Path) -> bool {
    run(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok()
}

/// Stages everything.
///
/// Refused while any conflict still has both sides in it: `git add --all`
/// sweeps unmerged paths up with the rest and marks every one of them
/// resolved, which is not what a button offering to stage your changes said
/// it would do.
pub fn stage_all(root: &Path) -> Result<()> {
    if let Some(path) = first_unresolved_conflict(root) {
        return Err(CoreError::invalid(format!(
            "{path} still has conflict markers in it, and staging everything would mark that \
             conflict resolved. Resolve it first — the terminal panel is the place for that."
        )));
    }

    run(root, &["add", "--all"]).map(|_| ())
}

/// The first conflicted path that has not actually been resolved, if any.
fn first_unresolved_conflict(root: &Path) -> Option<String> {
    let unmerged = run(root, &["diff", "--name-only", "--diff-filter=U", "-z"]).ok()?;
    unmerged
        .split('\0')
        .filter(|path| !path.is_empty())
        .find(|path| {
            resolve_within(root, path)
                .map(|absolute| file_has_conflict_markers(&absolute))
                .unwrap_or(false)
        })
        .map(str::to_string)
}

/// Commits what is staged. Never `-a`: what you staged is what you commit.
///
/// Allowed the longer limit because a commit runs the repository's own hooks,
/// and somebody's pre-commit hook is somebody's whole test suite.
pub fn commit(root: &Path, message: &str) -> Result<String> {
    if message.trim().is_empty() {
        return Err(CoreError::invalid("a commit needs a message"));
    }
    run_within(root, &["commit", "-m", message], SLOW_TIMEOUT)
}

pub fn push(root: &Path) -> Result<String> {
    run_reporting(root, &["push"], "Pushed.")
}

/// Pulls, refusing anything that is not a fast-forward.
///
/// A merge or a rebase started from a side panel with no way to see or resolve
/// a conflict would leave the repository somewhere the user did not ask to be.
pub fn pull(root: &Path) -> Result<String> {
    run_reporting(root, &["pull", "--ff-only"], "Pulled.")
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
        assert!(status.entries[0].conflicted);
    }

    #[test]
    fn a_conflict_that_never_says_u_is_still_a_conflict() {
        // Both sides added the file, or both deleted it. Git calls these
        // unmerged; the letters look exactly like an ordinary staged change.
        let status = parse_status("## main\0AA both-added.ts\0DD both-deleted.ts\0");
        assert!(status.entries[0].conflicted);
        assert!(status.entries[1].conflicted);
    }

    #[test]
    fn an_ordinary_change_is_not_mistaken_for_a_conflict() {
        let status = parse_status("## main\0MM edited.ts\0A  added.ts\0?? new.ts\0 D gone.ts\0");
        assert!(status.entries.iter().all(|entry| !entry.conflicted));
    }

    #[test]
    fn conflict_markers_are_recognised_only_where_git_would_write_them() {
        assert!(contains_conflict_markers(
            "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> other\n"
        ));
        // Prose about conflicts, and a setext heading, are not a conflict.
        assert!(!contains_conflict_markers(
            "Merging\n=======\n\nUse <<<<<<< to find them.\n"
        ));
        assert!(!contains_conflict_markers("nothing here\n"));
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

    #[test]
    fn unstaging_a_conflict_is_refused_instead_of_silently_resolving_it() {
        // `git restore --staged` on an unmerged path exits zero and drops the
        // conflict stages, leaving a file full of markers that git now calls
        // resolved — and the Staged section one click from committing it.
        let repo = repository_with_a_conflict();
        let root = repo.path();

        assert!(unstage(root, "merged.txt").is_err());

        assert!(status(root).unwrap().entries[0].conflicted);
        let text = std::fs::read_to_string(root.join("merged.txt")).unwrap();
        assert!(contains_conflict_markers(&text));
    }

    #[test]
    fn a_conflict_cannot_be_staged_while_both_sides_are_still_in_the_file() {
        let repo = repository_with_a_conflict();
        let root = repo.path();

        assert!(stage(root, "merged.txt").is_err());
        assert!(status(root).unwrap().entries[0].conflicted);
    }

    #[test]
    fn staging_everything_will_not_sweep_up_an_unresolved_conflict() {
        // "Stage all" promises to stage what changed, not to end a merge.
        let repo = repository_with_a_conflict();
        let root = repo.path();
        std::fs::write(root.join("unrelated.txt"), "fine\n").unwrap();

        assert!(stage_all(root).is_err());

        let status = status(root).unwrap();
        let conflict = status
            .entries
            .iter()
            .find(|entry| entry.path == "merged.txt")
            .unwrap();
        assert!(conflict.conflicted);
    }

    #[test]
    fn a_conflict_can_be_staged_once_it_has_actually_been_resolved() {
        let repo = repository_with_a_conflict();
        let root = repo.path();
        std::fs::write(root.join("merged.txt"), "ours and theirs\n").unwrap();

        stage(root, "merged.txt").unwrap();

        let entry = &status(root).unwrap().entries[0];
        assert!(!entry.conflicted);
        assert_eq!(entry.staged, FileState::Modified);
    }

    #[test]
    fn an_untracked_diff_names_the_file_by_its_place_in_the_project() {
        // Given an absolute path, git puts it in the `+++ b/…` header, so the
        // diff of a new file used to open with the reader's home directory in
        // it rather than the path they clicked on.
        let repo = empty_repository();
        let root = repo.path();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/new.txt"), "hello\n").unwrap();

        let text = diff(root, "sub/new.txt", false, true).unwrap();

        assert!(text.contains("+++ b/sub/new.txt"), "{text}");
        assert!(text.contains("+hello"), "{text}");
        assert!(
            !text.contains(&*root.to_string_lossy()),
            "the diff leaked where the project lives"
        );
    }

    #[test]
    fn a_diff_git_could_not_produce_is_an_error_rather_than_an_empty_one() {
        // An empty diff renders as "No changes to show." A file that vanished
        // between the status read and the click must not look like that.
        let repo = empty_repository();
        assert!(diff(repo.path(), "never-existed.txt", false, true).is_err());
    }

    #[test]
    fn a_command_that_will_not_finish_is_stopped_rather_than_waited_on() {
        // A pinentry dialog nobody answers, or a credential helper waiting for
        // a terminal that is not there. Without this the panel keeps every
        // control disabled until the app is restarted.
        let mut sleeper = Command::new("/bin/sh");
        sleeper.args(["-c", "sleep 30"]);

        let started = Instant::now();
        let stopped = execute(&mut sleeper, "push", Duration::from_millis(200));

        assert!(stopped.is_err(), "the command was allowed to run on");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "waited too long"
        );
    }

    /// A repository whose identity and configuration are the test's, not
    /// whatever the machine running it happens to have set.
    fn git_in(root: &Path, args: &[&str]) {
        Command::new(git_program())
            .current_dir(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Beacon")
            .env("GIT_AUTHOR_EMAIL", "beacon@example.invalid")
            .env("GIT_COMMITTER_NAME", "Beacon")
            .env("GIT_COMMITTER_EMAIL", "beacon@example.invalid")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("git should be runnable");
    }

    fn empty_repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "--quiet", "-b", "main"]);
        dir
    }

    fn repository_with_a_conflict() -> tempfile::TempDir {
        let dir = empty_repository();
        let root = dir.path();

        std::fs::write(root.join("merged.txt"), "start\n").unwrap();
        git_in(root, &["add", "."]);
        git_in(root, &["commit", "--quiet", "-m", "start"]);

        git_in(root, &["checkout", "--quiet", "-b", "other"]);
        std::fs::write(root.join("merged.txt"), "theirs\n").unwrap();
        git_in(root, &["commit", "--quiet", "-am", "theirs"]);

        git_in(root, &["checkout", "--quiet", "main"]);
        std::fs::write(root.join("merged.txt"), "ours\n").unwrap();
        git_in(root, &["commit", "--quiet", "-am", "ours"]);

        git_in(root, &["merge", "other"]);
        dir
    }
}
