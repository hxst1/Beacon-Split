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
    /// When Codex wrote these numbers down.
    ///
    /// Milliseconds since the epoch, from the event's own timestamp. The
    /// report is dated by this and not by when Beacon went and looked, or an
    /// idle conversation from this morning would be presented as current every
    /// time a window opened.
    pub at: Option<i64>,
    /// Tokens in the conversation right now, as Codex counts them.
    ///
    /// The last turn's total, not the conversation's: `total_token_usage` adds
    /// up every request ever made, so on a long conversation it runs to
    /// millions and says nothing about how full the window is.
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

    /// Whether every number has an answer, so there is nothing older to look
    /// for.
    fn is_complete(&self) -> bool {
        self.at.is_some()
            && self.context_used_tokens.is_some()
            && self.context_size.is_some()
            && self.five_hour_used_percentage.is_some()
            && self.five_hour_resets_at.is_some()
            && self.seven_day_used_percentage.is_some()
            && self.seven_day_resets_at.is_some()
    }
}

impl CodexUsage {
    /// The same numbers in the shape the rest of Beacon speaks.
    ///
    /// `session_id` is whatever Codex calls the conversation, when a hook has
    /// said; it is carried so a report can be matched to a workstream, exactly
    /// as the status line's is. Everything Claude Code's status line says and
    /// Codex does not — the model, its effort, the prompt cache — stays
    /// unknown rather than being filled in with a plausible value.
    pub fn into_report(
        self,
        project: crate::domain::ProjectId,
        session_id: Option<String>,
    ) -> crate::protocol::UsageReport {
        let context_used_percentage = self.context_used_percentage();
        crate::protocol::UsageReport {
            agent: crate::agent::AgentKind::Codex,
            session_id,
            context_used_percentage,
            context_remaining_percentage: context_used_percentage.map(|used| 100.0 - used),
            context_used_tokens: self.context_used_tokens,
            context_size: self.context_size,
            five_hour_used_percentage: self.five_hour_used_percentage,
            five_hour_resets_at: self.five_hour_resets_at,
            seven_day_used_percentage: self.seven_day_used_percentage,
            seven_day_resets_at: self.seven_day_resets_at,
            ..crate::protocol::UsageReport::unknown(project)
        }
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

/// The newest rollout started in a directory.
///
/// For a Codex nobody has asked to report. Beacon starts it in the project's
/// own checkout and Codex writes that directory into the first line of the
/// rollout, so the conversation can be found without a hook having said which
/// one it is. The newest is taken, which is the one Beacon just started unless
/// somebody is running two Codexes in the same folder — and then it is the one
/// they started last, which is the better guess of the two.
///
/// Compared with both sides resolved: on macOS a project under `/var` is
/// written by Codex as `/private/var`, and the two never match as text.
pub fn rollout_in(home: &Path, cwd: &Path) -> Option<PathBuf> {
    let wanted = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    days(&home.join("sessions")).into_iter().find_map(|day| {
        read_dir_sorted(&day)
            .into_iter()
            .find(|path| started_in(path, &wanted))
    })
}

/// Whether a rollout says it was started in this directory.
///
/// Only the first line is read. `session_meta` is written before anything
/// else, so a whole transcript never has to be opened to answer this.
fn started_in(rollout: &Path, wanted: &Path) -> bool {
    use std::io::{BufRead, BufReader};

    let Ok(file) = std::fs::File::open(rollout) else {
        return false;
    };
    let mut first = String::new();
    if BufReader::new(file).read_line(&mut first).is_err() {
        return false;
    }
    let Ok(record) = serde_json::from_str::<Value>(&first) else {
        return false;
    };
    let Some(cwd) = record["payload"]["cwd"].as_str() else {
        return false;
    };
    let cwd = PathBuf::from(cwd);
    cwd.canonicalize().as_deref().unwrap_or(&cwd) == wanted
}

/// The last thing a rollout said about what the conversation had spent.
///
/// Field by field, newest first. A `token_count` event is free to carry the
/// token counts without the rate limits or the other way round, and taking the
/// newest event whole would then blank whatever it left out — so each number
/// is filled from the newest event that has it, and the read stops as soon as
/// they are all answered.
pub fn read(path: &Path) -> Option<CodexUsage> {
    let text = tail(path, TAIL_BYTES)?;
    let mut found = CodexUsage::default();

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
        fill(
            &mut found.at,
            record["timestamp"].as_str().and_then(epoch_ms),
        );
        fill(
            &mut found.context_used_tokens,
            info["last_token_usage"]["total_tokens"].as_u64(),
        );
        fill(
            &mut found.context_size,
            info["model_context_window"].as_u64(),
        );
        fill(
            &mut found.five_hour_used_percentage,
            window(limits, 300).and_then(percentage),
        );
        fill(
            &mut found.five_hour_resets_at,
            window(limits, 300).and_then(resets_at),
        );
        fill(
            &mut found.seven_day_used_percentage,
            window(limits, 10_080).and_then(percentage),
        );
        fill(
            &mut found.seven_day_resets_at,
            window(limits, 10_080).and_then(resets_at),
        );

        if found.is_complete() {
            break;
        }
    }

    (!found.is_empty()).then_some(found)
}

/// Keeps the first answer, which scanning backwards makes the newest one.
fn fill<T>(slot: &mut Option<T>, found: Option<T>) {
    if slot.is_none() {
        *slot = found;
    }
}

/// `2026-10-05T08:00:00.123Z` as milliseconds since the epoch.
///
/// Written out rather than taken from a date library, because this is the only
/// date Beacon parses and it is always this shape: Codex writes UTC with a
/// `Z`. Anything else is no answer, and the caller falls back to the time it
/// read the file.
fn epoch_ms(stamp: &str) -> Option<i64> {
    let bytes = stamp.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    if !stamp.ends_with('Z') {
        return None;
    }
    let part = |from: usize, to: usize| -> Option<i64> { stamp.get(from..to)?.parse().ok() };

    let days = days_from_civil(part(0, 4)?, part(5, 7)?, part(8, 10)?);
    let seconds = days * 86_400 + part(11, 13)? * 3_600 + part(14, 16)? * 60 + part(17, 19)?;
    // Milliseconds when they are there; a timestamp without them is not wrong,
    // only less precise than this one happens to be.
    let millis = if bytes.get(19) == Some(&b'.') {
        part(20, 23).unwrap_or(0)
    } else {
        0
    };
    Some(seconds * 1_000 + millis)
}

/// Days from 1970-01-01 to a civil date, by Howard Hinnant's algorithm.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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
    ///
    /// `total` is the conversation's running total and `turn` is the last
    /// request's — which is the one that says how full the window is. They are
    /// equal only on the first turn, and they diverge fast: on a real rollout
    /// here the total reached 12.3 million against a window of 258,400.
    fn event(total: u64, turn: u64, window: u64, five: f64, week: f64) -> String {
        serde_json::json!({
            "timestamp": "2026-10-05T08:00:00.000Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": { "total_tokens": total },
                    "last_token_usage": { "total_tokens": turn },
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
                event(1_000, 1_000, 258_400, 1.0, 2.0),
                // Something in between, as there always is.
                serde_json::json!({ "type": "response_item", "payload": { "type": "message" } })
                    .to_string(),
                event(49_252, 25_787, 258_400, 15.0, 77.0),
            ],
        );

        let usage = read(&path).expect("the event is there");
        assert_eq!(usage.context_used_tokens, Some(25_787));
        assert_eq!(usage.context_size, Some(258_400));
        assert_eq!(usage.five_hour_used_percentage, Some(15.0));
        assert_eq!(usage.seven_day_used_percentage, Some(77.0));
        assert_eq!(usage.five_hour_resets_at, Some(1_790_876_384));
        assert_eq!(usage.seven_day_resets_at, Some(1_791_114_078));
    }

    #[test]
    fn a_long_conversation_is_measured_by_its_last_turn_not_its_running_total() {
        // The bug this exists for: `total_token_usage` adds up every request
        // ever made. On a real rollout here it reached 12.3 million against a
        // window of 258,400 — which read as 4,785% and clamped to a full
        // window, on a conversation that was one sixth of the way through it.
        let dir = tempfile::tempdir().unwrap();
        let path = rollout(
            dir.path(),
            "rollout-2026-10-05T08-00-00-abc.jsonl",
            &[
                event(23_465, 23_465, 258_400, 1.0, 2.0),
                event(12_364_386, 43_212, 258_400, 15.0, 77.0),
            ],
        );

        let usage = read(&path).unwrap();
        assert_eq!(usage.context_used_tokens, Some(43_212));
        assert_eq!(usage.context_used_percentage(), Some(16.72291));
    }

    #[test]
    fn a_number_the_newest_event_left_out_comes_from_the_one_before() {
        // Codex may write a `token_count` carrying the limits and no token
        // counts, or the other way round. Taking the newest event whole would
        // blank whatever it left out, and the meter would lose a figure it had
        // a moment ago for no reason the reader could see.
        let dir = tempfile::tempdir().unwrap();
        let sparse = serde_json::json!({
            "timestamp": "2026-10-05T09:00:00.000Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "rate_limits": {
                    "primary": { "used_percent": 20.0, "window_minutes": 300, "resets_at": 9 },
                    "secondary": { "used_percent": 80.0, "window_minutes": 10080, "resets_at": 9 }
                }
            }
        })
        .to_string();
        let path = rollout(
            dir.path(),
            "rollout-2026-10-05T08-00-00-abc.jsonl",
            &[event(100, 100, 1_000, 1.0, 2.0), sparse],
        );

        let usage = read(&path).unwrap();
        // The newest word on each: the limits from the sparse event, the
        // tokens from the one before it.
        assert_eq!(usage.five_hour_used_percentage, Some(20.0));
        assert_eq!(usage.context_used_tokens, Some(100));
        assert_eq!(usage.context_size, Some(1_000));
        // And the time of the newest, which is when the newest thing was said.
        assert_eq!(usage.at, epoch_ms("2026-10-05T09:00:00.000Z"));
    }

    #[test]
    fn a_timestamp_becomes_a_time() {
        // Checked against Python's `datetime.fromisoformat(...).timestamp()`.
        assert_eq!(epoch_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            epoch_ms("2026-10-05T08:00:00.000Z"),
            Some(1_791_187_200_000)
        );
        assert_eq!(
            epoch_ms("2026-10-05T08:00:00.123Z"),
            Some(1_791_187_200_123)
        );
        // A leap day, which is where a hand-written calendar goes wrong.
        assert_eq!(
            epoch_ms("2024-02-29T00:00:00.000Z"),
            Some(1_709_164_800_000)
        );
        // Seconds only is less precise, not wrong.
        assert_eq!(epoch_ms("2026-10-05T08:00:00Z"), Some(1_791_187_200_000));

        // Anything not this shape is no answer rather than a wrong one.
        assert_eq!(epoch_ms("2026-10-05T08:00:00+02:00"), None);
        assert_eq!(epoch_ms("yesterday"), None);
        assert_eq!(epoch_ms(""), None);
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
        std::fs::write(
            &path,
            format!("{padding}\n{}\n", event(7, 7, 100, 1.0, 2.0)),
        )
        .unwrap();

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
            &[event(1, 1, 100, 1.0, 1.0)],
        );
        let wanted = rollout(
            &sessions.join("2026/10/05"),
            "rollout-2026-10-05T08-00-00-01a0f728-667c.jsonl",
            &[event(2, 2, 100, 1.0, 1.0)],
        );

        assert_eq!(rollout_for(home.path(), "01a0f728-667c"), Some(wanted));
        assert_eq!(rollout_for(home.path(), "never-happened"), None);
    }

    #[test]
    fn finds_the_newest_conversation_started_in_a_directory() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let sessions = home.path().join("sessions");

        let meta = |at: &Path| {
            serde_json::json!({
                "type": "session_meta",
                "payload": { "cwd": at.to_string_lossy() }
            })
            .to_string()
        };

        rollout(
            &sessions.join("2026/10/05"),
            "rollout-2026-10-05T07-00-00-older.jsonl",
            &[meta(project.path()), event(1, 1, 100, 1.0, 1.0)],
        );
        let newest = rollout(
            &sessions.join("2026/10/05"),
            "rollout-2026-10-05T09-00-00-newer.jsonl",
            &[meta(project.path()), event(2, 2, 100, 1.0, 1.0)],
        );
        rollout(
            &sessions.join("2026/10/05"),
            "rollout-2026-10-05T23-00-00-somewhere-else.jsonl",
            &[meta(elsewhere.path()), event(3, 3, 100, 1.0, 1.0)],
        );

        assert_eq!(rollout_in(home.path(), project.path()), Some(newest));

        // A directory Codex has never been started in.
        let unknown = tempfile::tempdir().unwrap();
        assert_eq!(rollout_in(home.path(), unknown.path()), None);
    }

    #[test]
    fn a_report_says_which_agent_it_is_about_and_leaves_the_rest_unknown() {
        let usage = CodexUsage {
            at: Some(1_791_000_000_000),
            context_used_tokens: Some(25_840),
            context_size: Some(258_400),
            five_hour_used_percentage: Some(41.0),
            five_hour_resets_at: Some(1_790_876_384),
            seven_day_used_percentage: Some(94.0),
            seven_day_resets_at: Some(1_791_114_078),
        };

        let report = usage.into_report(crate::domain::ProjectId("pj_x".into()), Some("abc".into()));

        assert_eq!(report.agent, crate::agent::AgentKind::Codex);
        assert_eq!(report.session_id.as_deref(), Some("abc"));
        assert_eq!(report.context_used_percentage, Some(10.0));
        assert_eq!(report.context_remaining_percentage, Some(90.0));
        assert_eq!(report.context_used_tokens, Some(25_840));
        assert_eq!(report.five_hour_used_percentage, Some(41.0));
        assert_eq!(report.seven_day_resets_at, Some(1_791_114_078));

        // What Claude Code's status line says and Codex does not is left
        // unknown rather than filled in with something plausible.
        assert_eq!(report.model, None);
        assert_eq!(report.effort, None);
        assert_eq!(report.prompt_cache, None);
    }

    #[test]
    fn a_codex_that_has_never_run_is_not_an_error() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(rollout_for(home.path(), "anything"), None);
        assert_eq!(read(&home.path().join("nothing.jsonl")), None);
    }
}
