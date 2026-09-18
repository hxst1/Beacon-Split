//! Whether the marketplace Beacon generates is one the installed Codex will
//! actually take.
//!
//! A unit test can only say the JSON is the shape this build meant to write.
//! The thing worth knowing is whether Codex agrees, and the only way to learn
//! that is to hand it to Codex — the same reason the daemon tests start a real
//! daemon.
//!
//! Skipped, not failed, where Codex is not installed. It is recommended rather
//! than required, so a machine without it is a normal machine and CI is one of
//! them.

use std::path::Path;

use beacon_core::codex_plugin::{self, PluginStatus};
use beacon_core::tools::{path_with_program_dir, resolve_program};

/// Runs a Codex command against a home of its own.
///
/// `CODEX_HOME` is the isolation: without it this would register a marketplace
/// in the configuration somebody is actually using, and `codex plugin remove`
/// in a failing test would leave it there.
fn codex(home: &Path, args: &[&str]) -> std::process::Output {
    let program = resolve_program("codex").expect("checked by the caller");
    let mut command = std::process::Command::new(&program);
    command.args(args).env("CODEX_HOME", home);
    if let Some(path) = path_with_program_dir(&program) {
        command.env("PATH", path);
    }
    command.output().expect("codex should run")
}

#[test]
fn codex_accepts_the_marketplace_beacon_writes() {
    if resolve_program("codex").is_none() {
        eprintln!("skipped: no codex on this machine");
        return;
    }

    let marketplace = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let daemon = Path::new("/Applications/Beacon Split.app/Contents/MacOS/beacon-daemon");

    codex_plugin::write_at(marketplace.path(), daemon).expect("should write the marketplace");
    assert_eq!(
        codex_plugin::status_at(marketplace.path(), daemon).unwrap(),
        PluginStatus::Installed
    );

    let added = codex(
        home.path(),
        &[
            "plugin",
            "marketplace",
            "add",
            &marketplace.path().to_string_lossy(),
        ],
    );
    assert!(
        added.status.success(),
        "codex refused the marketplace: {}",
        String::from_utf8_lossy(&added.stderr)
    );

    // The selector depends on the marketplace and the plugin sharing a name,
    // which is the kind of thing that is right until somebody renames one.
    let installed = codex(home.path(), &["plugin", "add", "beacon@beacon"]);
    assert!(
        installed.status.success(),
        "codex refused the plugin: {}",
        String::from_utf8_lossy(&installed.stderr)
    );

    // And it landed somewhere Codex will look for it, rather than merely being
    // reported as installed.
    let cached = home.path().join("plugins/cache/beacon/beacon");
    assert!(
        cached.is_dir(),
        "nothing was cached at {}",
        cached.display()
    );
}

#[test]
fn a_marketplace_missing_its_manifest_is_refused_and_says_so() {
    if resolve_program("codex").is_none() {
        eprintln!("skipped: no codex on this machine");
        return;
    }

    // The mistake this guards against is mine: the manifest lives at
    // `.agents/plugins/marketplace.json`, and a marketplace without it is
    // refused with a message that does not name the file. Worth pinning, so
    // that moving the manifest fails here rather than in somebody's Settings.
    let bare = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    let refused = codex(
        home.path(),
        &[
            "plugin",
            "marketplace",
            "add",
            &bare.path().to_string_lossy(),
        ],
    );
    assert!(!refused.status.success());
}
