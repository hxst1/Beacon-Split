//! That Beacon can be told to look at this machine again.
//!
//! The bug this is here for is the one that costs a new user: Beacon says
//! Claude Code is missing, they install it, they come back, and Beacon says it
//! again — because where every program lives was worked out once and kept, and
//! nothing tells them that only a restart clears it.
//!
//! In its own test binary, and deliberately a single test: it sets the
//! process's `PATH`, and doing that alongside anything else running
//! concurrently would be a data race. The same reason `session_environment`
//! lives alone.
//!
//! Unix only. Windows resolves through its own `PATH` and `.cmd` shims rather
//! than through a login shell, which is a different path through the code and
//! a different test.

#![cfg(unix)]

use std::path::Path;

use beacon_core::tools::{forget_programs, resolve_program};

/// A program on disk, named something no machine would already have.
fn plant(dir: &Path, name: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let at = dir.join(name);
    std::fs::write(&at, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o755)).unwrap();
    at
}

#[test]
fn beacon_can_be_told_to_look_at_this_machine_again() {
    let dir = tempfile::tempdir().unwrap();
    let name = "beacon-test-not-a-real-program";

    // Our own `PATH` is where this lands. The login shell is asked first and
    // will not have heard of it, so resolution falls through to here — which
    // is the behaviour being relied on, not a shortcut around it.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let with_dir = std::env::join_paths(
        std::iter::once(dir.path().to_path_buf()).chain(std::env::split_paths(&path)),
    )
    .unwrap();
    // Safe here: this binary runs one test and sets this before resolving.
    unsafe { std::env::set_var("PATH", &with_dir) };

    // Nothing installed yet, and a miss is never remembered — otherwise the
    // next few lines could not happen.
    forget_programs();
    assert_eq!(resolve_program(name), None);

    // Installed while Beacon runs, and found without anybody being asked to
    // restart anything. This is the whole feature.
    let planted = plant(dir.path(), name);
    assert_eq!(
        resolve_program(name).as_deref(),
        Some(planted.as_path()),
        "a program installed while Beacon ran has to be found"
    );

    // Gone from the disk, still answered from memory: asking the shell costs
    // well over a second on an ordinary machine, and a program that was found
    // does not move.
    std::fs::remove_file(&planted).unwrap();
    assert_eq!(
        resolve_program(name).as_deref(),
        Some(planted.as_path()),
        "a program that was found should not be looked for again"
    );

    // And told to forget, it goes and looks.
    forget_programs();
    assert_eq!(
        resolve_program(name),
        None,
        "after forgetting, the answer is whatever the machine says now"
    );
}
