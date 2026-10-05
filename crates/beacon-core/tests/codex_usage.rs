//! Whether what Beacon reads out of a rollout is what Codex puts in one.
//!
//! The unit tests build the file themselves, so they can only say the parser
//! agrees with the author's idea of the format. This reads a rollout the
//! installed Codex actually wrote — the same reason the plugin tests hand the
//! marketplace to Codex rather than asserting its JSON.
//!
//! Skipped, not failed, where Codex has never run on this machine. Codex is
//! recommended rather than required, so that is a normal machine and CI is one
//! of them.

use beacon_core::codex_usage::{self, CodexUsage};

/// The newest rollout on this machine, whichever conversation it belongs to.
fn newest_rollout() -> Option<std::path::PathBuf> {
    let sessions = codex_usage::home().join("sessions");
    let mut found: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![sessions];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                found.push(path);
            }
        }
    }
    found.sort_unstable();
    found.pop()
}

#[test]
fn reads_the_numbers_out_of_a_rollout_codex_wrote() {
    let Some(path) = newest_rollout() else {
        eprintln!("skipped: codex has never run on this machine");
        return;
    };

    let Some(usage) = codex_usage::read(&path) else {
        // A conversation that was opened and never answered has no account to
        // read, which is a true answer rather than a failure.
        eprintln!("skipped: the newest rollout has no token count in its tail");
        return;
    };

    assert_ne!(
        usage,
        CodexUsage::default(),
        "something should have been read"
    );

    if let Some(size) = usage.context_size {
        assert!(
            size > 1000,
            "a context window of {size} is not a context window"
        );
    }
    if let Some(percentage) = usage.context_used_percentage() {
        assert!(
            (0.0..=100.0).contains(&percentage),
            "context used {percentage}% is not a percentage"
        );
    }
    for limit in [
        usage.five_hour_used_percentage,
        usage.seven_day_used_percentage,
    ] {
        if let Some(used) = limit {
            assert!((0.0..=100.0).contains(&used), "{used}% is not a percentage");
        }
    }
    // Reset times are seconds, not milliseconds. A millisecond value here
    // would be a thousand times too far in the future and would show as an
    // allowance that never comes round.
    for resets in [usage.five_hour_resets_at, usage.seven_day_resets_at] {
        if let Some(at) = resets {
            assert!(
                (1_600_000_000..10_000_000_000).contains(&at),
                "{at} does not look like a unix time in seconds"
            );
        }
    }

    eprintln!(
        "read from {}: {:?} of {:?} tokens, five-hour {:?}%, week {:?}%",
        path.display(),
        usage.context_used_tokens,
        usage.context_size,
        usage.five_hour_used_percentage,
        usage.seven_day_used_percentage
    );
}
