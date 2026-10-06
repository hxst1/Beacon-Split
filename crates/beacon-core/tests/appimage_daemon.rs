//! That a daemon named in somebody's configuration is still there tomorrow.
//!
//! An AppImage is mounted under a name that changes on every launch, and
//! Beacon writes the daemon's path into Claude Code's `settings.json` and
//! Codex's plugin — files on the user's own disk, read long after the mount
//! has gone. The failure is silent: a hook that cannot be run says nothing,
//! and the tabs simply stop reporting.
//!
//! In its own test binary, and a single test: it sets `APPIMAGE` and
//! `XDG_CONFIG_HOME` for the process, and doing that alongside anything else
//! running concurrently would be a data race. The same reason
//! `session_environment` lives alone.
//!
//! Linux only, and not merely because that is where an AppImage is: it is also
//! the only platform where `XDG_CONFIG_HOME` moves Beacon's directory, so it
//! is the only one where this can be asked without writing into the
//! configuration of whoever is running the tests.

#![cfg(target_os = "linux")]

use beacon_core::client::daemon_binary_path;

#[test]
fn under_an_appimage_the_daemon_is_kept_where_it_will_still_be() {
    let config = tempfile::tempdir().unwrap();

    // A daemon beside the running program, which is what there would be inside
    // an AppImage. `current_exe` here is this test binary, so that is where it
    // goes — the rule being checked is "copy it somewhere that lasts", not the
    // geometry of a mount.
    let beside = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("beacon-daemon");
    std::fs::write(&beside, b"#!/bin/sh\nexit 0\n").unwrap();

    // Safe here: this binary runs one test and sets these before asking.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", config.path());
        std::env::remove_var("APPIMAGE");
    }

    // Outside an AppImage nothing is copied: the application is installed
    // somewhere real and the daemon beside it is the right answer.
    let plain = daemon_binary_path();
    assert!(
        !plain.starts_with(config.path()),
        "nothing should be copied when this is an ordinary install, got {}",
        plain.display()
    );

    unsafe { std::env::set_var("APPIMAGE", "/home/somebody/Beacon Split.AppImage") };

    let kept = daemon_binary_path();
    assert!(
        kept.starts_with(config.path()),
        "under an AppImage the daemon has to be named somewhere that lasts, got {}",
        kept.display()
    );
    assert!(kept.is_file(), "and it has to actually be there");

    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(&kept).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o111,
        0o111,
        "a daemon nobody can execute is no daemon"
    );

    // Asked again, the same answer, and without copying over a file the last
    // launch's daemon may still be running out of.
    assert_eq!(daemon_binary_path(), kept);
    assert!(
        !kept.with_extension("incoming").exists(),
        "the half-written copy should have been renamed away"
    );

    let _ = std::fs::remove_file(&beside);
}
