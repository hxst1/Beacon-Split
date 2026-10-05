//! What a Codex conversation is spending, read from the transcript it already
//! writes.
//!
//! Claude Code has a status line, which is a program Beacon can ask to report.
//! Codex has nothing of the kind: its hooks say what is happening, never what
//! it costs. But it writes a rollout of every conversation as it goes, and
//! after each turn that rollout gets a `token_count` event carrying the whole
//! account of the turn — tokens used, the size of the context window, and the
//! two rate limits with the time each one comes round again.
//!
//! So the numbers exist and are already on disk. This reads them. Nothing here
//! estimates anything: a figure Beacon cannot read is a figure Beacon does not
//! show.
//!
//! Only the tail of the file is read. A rollout is a full transcript and grows
//! without limit, while the event wanted here is written after every turn and
//! is therefore near the end — so reading the last few pages finds it, and a
//! long conversation costs no more to ask about than a short one.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// How much of the end of a rollout to read.
///
/// A `token_count` event is a few hundred bytes and one is written per turn,
/// so this holds many of them even where the turns in between were large. Too
/// small risks a window with none in it; too large is reading a transcript to
/// find its last line.
const TAIL_BYTES: u64 = 256 * 1024;

/// What a rollout last said a conversation had spent.
///
/// Every field is optional because every one of them is something Codex may
/// not have said yet: a conversation that has not had a reply has no token
/// count, and an account without limits reports none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexUsage {
    /// Tokens in the conversation so far, as Codex counts them.
    pub context_used_tokens: Option<u64>,
    /// The model's context window, which is what makes the count a fraction.
    pub context_size: Option<u64>,
    pub five_hour_used_percentage: Option<f32>,
    pub five_hour_resets_at: Option<i64>,
    pub seven_day_used_percentage: Option<f32>,
    pub seven_day_resets_at: Option<i64>,
}

impl CodexUsage {
    /// How much of the context window is gone, 0..100.
    ///
    /// Computed rather than read because Codex reports the two numbers and not
    /// the fraction. A window of zero would be a division by zero and is
    /// treated as no answer, which is what it is.
    pub fn context_used_percentage(&self) -> Option<f32> {
        let used = self.context_used_tokens?;
        let size = self.context_size.filter(|size| *size > 0)?;
        Some(((used as f64 / size as f64) * 100.0).clamp(0.0, 100.0) as f32)
    }

    /// Whether anything was found worth showing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Where Codex keeps its configuration and its sessions.
///
/// `CODEX_HOME` first, because that is what Beacon sets when it wants Codex
/// pointed somewhere else, and what somebody with more than one Codex setup
/// will have set for themselves.
pub fn home() -> PathBuf {
    if let Some(set) = std::env::var_os("CODEX_HOME") {
        return PathBuf::from(set);
    }
    crate::paths::home_dir().join(".codex")
}

/// The rollout of one conversation, by the id Codex calls it.
///
/// Rollouts are filed by the day they were started — `sessions/2026/10/05` —
/// and named with the id at the end, so the id alone does not say where to
/// look. Walking the tree is what is left, and the days are walked newest
/// first because the conversation somebody is in was almost certainly started
/// today or yesterday.
pub fn rollout_for(home: &Path, session: &str) -> Option<PathBuf> {
    let wanted = format!("-{session}.jsonl");
    days(&home.join("sessions")).into_iter().find_map(|day| {
        read_dir_sorted(&day)
            .into_iter()
            .find(|path| path.to_string_lossy().ends_with(&wanted))
    })
}

/// The last thing a rollout said about what the conversation had spent.
pub fn read(path: &Path) -> Option<CodexUsage> {
    let text = tail(path, TAIL_BYTES)?;

    // Backwards: the answer wanted is the newest one, and a rollout holds one
    // of these per turn.
    for line in text.lines().rev() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            // The first line is usually cut in half by where the read began,
            // and a rollout being written to can end in a partial line.
            continue;
        };
        let payload = &record["payload"];
        if payload["type"] != "token_count" {
            continue;
        }

        let info = &payload["info"];
        let limits = &payload["rate_limits"];
        let usage = CodexUsage {
            context_used_tokens: info["total_token_usage"]["total_tokens"].as_u64(),
            context_size: info["model_context_window"].as_u64(),
            five_hour_used_percentage: window(limits, 300).and_then(percentage),
            five_hour_resets_at: window(limits, 300).and_then(resets_at),
            seven_day_used_percentage: window(limits, 10_080).and_then(percentage),
            seven_day_resets_at: window(limits, 10_080).and_then(resets_at),
        };
        if !usage.is_empty() {
            return Some(usage);
        }
    }
    None
}

/// The limit covering a window of so many minutes.
///
/// Matched on the length of the window rather than on `primary` and
/// `secondary`, which are only an order. Beacon shows a five-hour allowance
/// and a weekly one because those are the two Anthropic has; if Codex ever
/// reports them the other way round, or adds a third, this still picks the one
/// that answers the question being asked.
fn window(limits: &Value, minutes: u64) -> Option<&Value> {
    ["primary", "secondary"]
        .into_iter()
        .map(|name| &limits[name])
        .find(|limit| limit["window_minutes"].as_u64() == Some(minutes))
}

fn percentage(limit: &Value) -> Option<f32> {
    limit["used_percent"].as_f64().map(|used| used as f32)
}

fn resets_at(limit: &Value) -> Option<i64> {
    limit["resets_at"].as_i64()
}

/// Every day directory under `sessions`, newest first.
///
/// Three levels — year, month, day — and all of them are numbers, so sorting
/// the names in reverse is sorting by date. Anything that is not a directory,
/// or a tree that is not there at all, is simply nothing to look through.
fn days(sessions: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for year in read_dir_sorted(sessions) {
        for month in read_dir_sorted(&year) {
            out.extend(read_dir_sorted(&month));
        }
    }
    out
}

/// The entries of a directory, newest-looking first.
///
/// Reversed by name, which for these is by date: `2026` before `2025`, `10`
/// before `09`, and a rollout file carries the time it was started.
fn read_dir_sorted(at: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(at) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort_unstable();
    paths.reverse();
    paths
}

/// The last `bytes` of a file, as text.
///
/// Lossy on purpose: a cut at a byte boundary can land in the middle of a
/// character, and one replacement character in a line that is being skipped
/// anyway is not a reason to give up on the file.
fn tail(path: &Path, bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let from = size.saturating_sub(bytes);
    file.seek(SeekFrom::Start(from)).ok()?;

    let mut buffer = Vec::with_capacity((size - from) as usize);
    file.read_to_end(&mut buffer).ok()?;
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One `token_count` event as Codex writes it, trimmed to the fields read.
    fn event(total: u64, window: u64, five: f64, week: f64) -> String {
        serde_json::json!({
            "timestamp": "2026-10-05T08:00:00.000Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": { "total_tokens": total },
                    "last_token_usage": { "total_tokens": 1 },
                    "model_context_window": window
                },
                "rate_limits": {
                    "primary": { "used_percent": five, "window_minutes": 300, "resets_at": 1790876384 },
                    "secondary": { "used_percent": week, "window_minutes": 10080, "resets_at": 1791114078 }
                }
            }
        })
        .to_string()
    }

    fn rollout(dir: &Path, name: &str, lines: &[String]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
        path
    }

    #[test]
    fn reads_the_last_account_of_a_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let path = rollout(
            dir.path(),
            "rollout-2026-10-05T08-00-00-abc.jsonl",
            &[
                event(1_000, 258_400, 1.0, 2.0),
                // Something in between, as there always is.
                serde_json::json!({ "type": "response_item", "payload": { "type": "message" } })
                    .to_string(),
                event(23_465, 258_400, 15.0, 77.0),
            ],
        );

        let usage = read(&path).expect("the event is there");
        assert_eq!(usage.context_used_tokens, Some(23_465));
        assert_eq!(usage.context_size, Some(258_400));
        assert_eq!(usage.five_hour_used_percentage, Some(15.0));
        assert_eq!(usage.seven_day_used_percentage, Some(77.0));
        assert_eq!(usage.five_hour_resets_at, Some(1_790_876_384));
        assert_eq!(usage.seven_day_resets_at, Some(1_791_114_078));
    }

    #[test]
    fn works_out_the_fraction_codex_does_not_report() {
        let usage = CodexUsage {
            context_used_tokens: Some(25_840),
            context_size: Some(258_400),
            ..CodexUsage::default()
        };
        assert_eq!(usage.context_used_percentage(), Some(10.0));
    }

    #[test]
    fn a_window_of_nothing_is_no_answer_rather_than_a_division() {
        let usage = CodexUsage {
            context_used_tokens: Some(1),
            context_size: Some(0),
            ..CodexUsage::default()
        };
        assert_eq!(usage.context_used_percentage(), None);
    }

    #[test]
    fn the_limits_are_matched_by_how_long_they_last() {
        // Written the other way round, which is allowed: `primary` and
        // `secondary` are an order, not a meaning.
        let swapped = serde_json::json!({
            "primary": { "used_percent": 77.0, "window_minutes": 10080, "resets_at": 2 },
            "secondary": { "used_percent": 15.0, "window_minutes": 300, "resets_at": 1 }
        });
        assert_eq!(window(&swapped, 300).and_then(percentage), Some(15.0));
        assert_eq!(window(&swapped, 10_080).and_then(percentage), Some(77.0));
        assert_eq!(window(&swapped, 60), None);
    }

    #[test]
    fn a_rollout_with_nothing_spent_yet_says_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = rollout(
            dir.path(),
            "rollout-2026-10-05T08-00-00-abc.jsonl",
            &[serde_json::json!({ "type": "session_meta", "payload": {} }).to_string()],
        );
        assert_eq!(read(&path), None);
    }

    #[test]
    fn a_line_cut_in_half_by_where_the_read_began_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout-2026-10-05T08-00-00-abc.jsonl");
        // Enough padding to push the start of the read past the first line,
        // which is then half a line and must not stop the rest being read.
        let padding = serde_json::json!({
            "type": "response_item",
            "payload": { "text": "x".repeat(TAIL_BYTES as usize) }
        })
        .to_string();
        std::fs::write(&path, format!("{padding}\n{}\n", event(7, 100, 1.0, 2.0))).unwrap();

        let usage = read(&path).expect("the last line is whole");
        assert_eq!(usage.context_used_tokens, Some(7));
    }

    #[test]
    fn finds_a_conversation_by_its_id_under_the_day_it_was_started() {
        let home = tempfile::tempdir().unwrap();
        let sessions = home.path().join("sessions");
        rollout(
            &sessions.join("2026/10/04"),
            "rollout-2026-10-04T09-00-00-older.jsonl",
            &[event(1, 100, 1.0, 1.0)],
        );
        let wanted = rollout(
            &sessions.join("2026/10/05"),
            "rollout-2026-10-05T08-00-00-01a0f728-667c.jsonl",
            &[event(2, 100, 1.0, 1.0)],
        );

        assert_eq!(rollout_for(home.path(), "01a0f728-667c"), Some(wanted));
        assert_eq!(rollout_for(home.path(), "never-happened"), None);
    }

    #[test]
    fn a_codex_that_has_never_run_is_not_an_error() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(rollout_for(home.path(), "anything"), None);
        assert_eq!(read(&home.path().join("nothing.jsonl")), None);
    }
}
