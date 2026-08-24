//! Exercises git against real repositories.
//!
//! The parser has its own unit tests; these check that what git actually prints
//! matches what those tests assume, which is the part that rots silently.

use std::path::Path;
use std::process::Command;

use beacon_core::git::{self, FileState};

/// A repository with a committed file, an identity, and no global config
/// leaking in.
fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git_raw(dir.path(), &["init", "-b", "main"]);
    git_raw(
        dir.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    git_raw(dir.path(), &["config", "user.name", "Beacon Test"]);
    dir
}

fn git_raw(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("git should be installed");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn an_untracked_file_shows_up_as_untracked() {
    let dir = repository();
    std::fs::write(dir.path().join("README.md"), "# hello").unwrap();

    let status = git::status(dir.path()).unwrap();
    assert!(status.unborn, "a repository with no commits is unborn");
    assert_eq!(status.entries.len(), 1);
    assert!(status.entries[0].is_untracked());
    assert_eq!(status.entries[0].path, "README.md");
}

#[test]
fn staging_moves_a_change_from_the_working_tree_to_the_index() {
    let dir = repository();
    std::fs::write(dir.path().join("app.ts"), "export const a = 1\n").unwrap();

    git::stage(dir.path(), "app.ts").unwrap();
    let status = git::status(dir.path()).unwrap();
    assert_eq!(status.entries[0].staged, FileState::Added);
    assert_eq!(status.entries[0].unstaged, FileState::Unmodified);

    git::unstage(dir.path(), "app.ts").unwrap();
    let status = git::status(dir.path()).unwrap();
    assert!(status.entries[0].is_untracked());
}

#[test]
fn committing_clears_the_status_and_names_the_branch() {
    let dir = repository();
    std::fs::write(dir.path().join("app.ts"), "export const a = 1\n").unwrap();
    git::stage(dir.path(), "app.ts").unwrap();

    git::commit(dir.path(), "add app").unwrap();

    let status = git::status(dir.path()).unwrap();
    assert!(!status.unborn);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status.entries.is_empty(), "everything was committed");
}

#[test]
fn a_modified_file_produces_a_diff_and_a_staged_one_produces_another() {
    let dir = repository();
    std::fs::write(dir.path().join("app.ts"), "one\n").unwrap();
    git::stage(dir.path(), "app.ts").unwrap();
    git::commit(dir.path(), "first").unwrap();

    std::fs::write(dir.path().join("app.ts"), "two\n").unwrap();

    let unstaged = git::diff(dir.path(), "app.ts", false, false).unwrap();
    assert!(unstaged.contains("-one"), "got: {unstaged}");
    assert!(unstaged.contains("+two"), "got: {unstaged}");

    // Nothing is staged yet, so the staged diff is empty rather than an error.
    assert!(
        git::diff(dir.path(), "app.ts", true, false)
            .unwrap()
            .is_empty()
    );

    git::stage(dir.path(), "app.ts").unwrap();
    let staged = git::diff(dir.path(), "app.ts", true, false).unwrap();
    assert!(staged.contains("+two"), "got: {staged}");
}

#[test]
fn an_untracked_file_still_has_a_diff_to_show() {
    let dir = repository();
    std::fs::write(dir.path().join("new.ts"), "fresh\n").unwrap();

    let diff = git::diff(dir.path(), "new.ts", false, true).unwrap();
    assert!(diff.contains("+fresh"), "got: {diff}");
}

#[test]
fn a_rename_is_reported_with_where_it_came_from() {
    let dir = repository();
    std::fs::write(dir.path().join("old.ts"), "contents that stay the same\n").unwrap();
    git::stage(dir.path(), "old.ts").unwrap();
    git::commit(dir.path(), "first").unwrap();

    std::fs::rename(dir.path().join("old.ts"), dir.path().join("new.ts")).unwrap();
    git::stage_all(dir.path()).unwrap();

    let status = git::status(dir.path()).unwrap();
    let entry = &status.entries[0];
    assert_eq!(entry.staged, FileState::Renamed);
    assert_eq!(entry.path, "new.ts");
    assert_eq!(entry.original_path.as_deref(), Some("old.ts"));
}

#[test]
fn a_path_with_spaces_round_trips() {
    let dir = repository();
    std::fs::write(dir.path().join("a file with spaces.txt"), "hi\n").unwrap();

    git::stage(dir.path(), "a file with spaces.txt").unwrap();
    let status = git::status(dir.path()).unwrap();
    assert_eq!(status.entries[0].path, "a file with spaces.txt");
    assert_eq!(status.entries[0].staged, FileState::Added);
}

#[test]
fn a_commit_with_nothing_staged_fails_with_gits_own_message() {
    let dir = repository();
    std::fs::write(dir.path().join("app.ts"), "one\n").unwrap();
    git::stage(dir.path(), "app.ts").unwrap();
    git::commit(dir.path(), "first").unwrap();

    let error = git::commit(dir.path(), "nothing to say").unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("nothing") || message.contains("clean"),
        "the user should see git's explanation; got: {message}"
    );
}

#[test]
fn a_path_outside_the_repository_is_refused() {
    let dir = repository();
    assert!(git::stage(dir.path(), "../outside.txt").is_err());
    assert!(git::diff(dir.path(), "/etc/passwd", false, false).is_err());
}
