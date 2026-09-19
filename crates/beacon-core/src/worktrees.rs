//! Where an agent works, when it is not to work where you do.
//!
//! Two agents in one project write to the same files, and the second one to
//! save wins. A git worktree is the answer git already has for this: another
//! checkout of the same repository, on its own branch, with its own working
//! tree and the same history.
//!
//! Off unless asked for, and when asked for it applies to *every* agent
//! including the only one. That is the whole rule, and it is the reason it can
//! be stated in a sentence: agents work in their own checkouts, and yours is
//! yours. The alternative — one agent keeping the real directory and the rest
//! being moved aside — needs an answer to which one, and every answer is either
//! arbitrary or depends on the order somebody opened panels in.
//!
//! They are made under Beacon's own directory rather than beside the project,
//! so that nothing Beacon creates is ever scanned by a build, committed by
//! accident, or mistaken for something the user put there.

use std::path::{Path, PathBuf};

use crate::agent::AgentKind;
use crate::domain::ProjectId;
use crate::error::Result;
use crate::git;

/// Everything Beacon has checked out on a project's behalf.
pub fn root() -> PathBuf {
    crate::paths::default_config_dir().join("worktrees")
}

/// Where a project's agent would have its own checkout.
pub fn path_for(project: &ProjectId, agent: AgentKind) -> PathBuf {
    root().join(project.as_str()).join(agent.as_str())
}

/// The branch an agent commits on.
///
/// Namespaced so that it reads as Beacon's in `git branch` and cannot collide
/// with one somebody already had.
pub fn branch_for(agent: AgentKind) -> String {
    format!("beacon/{}", agent.as_str())
}

/// The directory a session for this agent should start in.
///
/// Falls back to the project itself, and the two reasons are different. Asked
/// not to separate them, this is simply where the work happens. Asked to, but
/// the project is not a git repository, there is no such thing as a worktree —
/// and refusing to start a session over that would be choosing a principle over
/// somebody's afternoon, so it says so in the log and carries on.
pub fn root_for(
    project_root: &Path,
    project: &ProjectId,
    agent: AgentKind,
    separate: bool,
) -> Result<PathBuf> {
    if !separate {
        return Ok(project_root.to_path_buf());
    }

    if !git::is_repository(project_root) {
        tracing::info!(
            project = %project,
            "asked for separate worktrees, but this project is not a git repository"
        );
        return Ok(project_root.to_path_buf());
    }

    ensure(project_root, project, agent)
}

/// Makes the worktree if it is not there, and returns where it is.
///
/// Reusing one that already exists is the common case by far: every restart of
/// an agent, and every time the window opens. Git's own listing is what is
/// asked, rather than whether the directory is there, because a directory can
/// survive a worktree git has forgotten — and adding one on top of that fails
/// in a way nobody could read.
fn ensure(project_root: &Path, project: &ProjectId, agent: AgentKind) -> Result<PathBuf> {
    let at = path_for(project, agent);

    // Compared with both sides resolved. Git reports where a worktree really
    // is, and on macOS that is not the path Beacon built: `/var` is a symlink
    // to `/private/var`, and a plain comparison never matches — so every start
    // would decide it had never made this worktree and try to make it again.
    let known = git::worktrees(project_root)?
        .into_iter()
        .any(|worktree| same_place(&worktree.path, &at));
    if known && at.is_dir() {
        return Ok(at);
    }

    // Anything else is git's record and the disk disagreeing, and they can
    // disagree in either direction at once: a record whose directory has gone,
    // a directory whose record has. Pruning clears the first, removing clears
    // the second, and git refuses to add over either.
    if known || at.exists() {
        tracing::warn!(
            at = %at.display(),
            "a worktree git and the disk disagree about; clearing it"
        );
        git::prune_worktrees(project_root)?;
        if at.exists() {
            std::fs::remove_dir_all(&at).map_err(|err| crate::error::CoreError::io(&at, err))?;
        }
    }

    if let Some(parent) = at.parent() {
        std::fs::create_dir_all(parent).map_err(|err| crate::error::CoreError::io(parent, err))?;
    }

    git::add_worktree(project_root, &at, &branch_for(agent))?;
    tracing::info!(at = %at.display(), project = %project, "made a worktree");
    Ok(at)
}

/// Whether two paths name the same directory, following any symlinks.
///
/// Falls back to comparing them as written when either cannot be resolved,
/// which is the case that matters least: a path that does not exist is not one
/// git is holding a worktree at.
fn same_place(one: &Path, other: &Path) -> bool {
    match (one.canonicalize(), other.canonicalize()) {
        (Ok(one), Ok(other)) => one == other,
        _ => one == other,
    }
}

/// Gives back every worktree Beacon made for a project.
///
/// For a project being removed: the checkouts are Beacon's, so they go with it
/// rather than being left in a directory nobody will ever look in again.
pub fn forget_project(project_root: &Path, project: &ProjectId) {
    for agent in AgentKind::ALL {
        let at = path_for(project, agent);
        if !at.exists() {
            continue;
        }
        // Best effort throughout. A project is being removed either way, and a
        // worktree that will not go is not a reason to keep the project.
        if let Err(err) = git::remove_worktree(project_root, &at) {
            tracing::warn!(error = %err, at = %at.display(), "could not remove a worktree");
        }
    }
    let _ = std::fs::remove_dir_all(root().join(project.as_str()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId("pj_x".into())
    }

    #[test]
    fn each_agent_gets_a_place_of_its_own_under_beacons_directory() {
        let claude = path_for(&project(), AgentKind::Claude);
        let codex = path_for(&project(), AgentKind::Codex);

        assert_ne!(claude, codex);
        assert!(claude.starts_with(root()));
        assert!(claude.ends_with("pj_x/claude"));
        // Never beside the project: nothing Beacon makes should be scanned by a
        // build or committed by accident.
        assert!(claude.starts_with(crate::paths::default_config_dir()));
    }

    #[test]
    fn the_branches_are_namespaced_so_they_read_as_beacons() {
        assert_eq!(branch_for(AgentKind::Claude), "beacon/claude");
        assert_eq!(branch_for(AgentKind::Codex), "beacon/codex");
    }

    #[test]
    fn not_asked_to_separate_them_means_the_project_itself() {
        let dir = tempfile::tempdir().unwrap();
        let root = root_for(dir.path(), &project(), AgentKind::Codex, false).unwrap();
        assert_eq!(root, dir.path());
    }

    #[test]
    fn a_project_that_is_not_a_repository_keeps_working() {
        // There is no worktree of a directory that is not a repository, and
        // refusing to start the session over that would be choosing a principle
        // over somebody's afternoon.
        let dir = tempfile::tempdir().unwrap();
        assert!(!git::is_repository(dir.path()));

        let root = root_for(dir.path(), &project(), AgentKind::Codex, true).unwrap();
        assert_eq!(root, dir.path());
    }
}
