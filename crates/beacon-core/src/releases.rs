use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// What changed in each version, shipped inside the application.
///
/// Local on purpose: what is new in the build you are running is a fact the
/// build already knows, and asking a server about it would mean the notes stop
/// working the moment the network does.
const NOTES: &str = include_str!("../../../release-notes.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// `major.minor.patch`.
    pub version: String,
    /// ISO date, for ordering by eye rather than by parsing.
    pub date: String,
    /// One line saying what this release is about, if it is about one thing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub changes: Vec<String>,
}

/// The version this build is.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Every release, newest first.
pub fn all() -> Result<Vec<Release>> {
    let mut releases: Vec<Release> = serde_json::from_str(NOTES).map_err(|source| {
        CoreError::invalid(format!(
            "the release notes shipped with this build are broken: {source}"
        ))
    })?;
    releases.sort_by(|a, b| compare(&b.version, &a.version));
    Ok(releases)
}

/// What the user has not seen yet, newest first.
///
/// `seen` is the last version they were shown. `None` means they have never
/// been shown anything, which is a first run — and a first run should not be
/// met with a history of releases they were never here for.
pub fn unseen(seen: Option<&str>) -> Result<Vec<Release>> {
    let Some(seen) = seen else {
        return Ok(Vec::new());
    };

    Ok(all()?
        .into_iter()
        .filter(|release| compare(&release.version, seen) == std::cmp::Ordering::Greater)
        .collect())
}

/// Compares two `major.minor.patch` strings.
///
/// Written rather than pulled in: the whole rule is three numbers, and anything
/// unparseable sorts as zero rather than failing — a malformed version in the
/// notes should not stop the application starting.
pub fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    let parts = |value: &str| -> [u64; 3] {
        let mut out = [0u64; 3];
        // Anything after a `-` is a pre-release marker; ignored for ordering,
        // which is wrong for semver in general and right for what ships here.
        let core = value.split('-').next().unwrap_or(value);
        for (index, piece) in core.split('.').take(3).enumerate() {
            out[index] = piece.trim().parse().unwrap_or(0);
        }
        out
    };

    parts(left).cmp(&parts(right))
}

/// Whether `candidate` is newer than what is running.
pub fn is_newer_than_current(candidate: &str) -> bool {
    compare(candidate, current_version()) == std::cmp::Ordering::Greater
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn the_notes_that_ship_with_this_build_parse() {
        // They are compiled in, so a broken file is a broken release rather
        // than something a user could ever be shown.
        let releases = all().unwrap();
        assert!(!releases.is_empty(), "a build should say what it is");
    }

    #[test]
    fn every_release_has_something_to_say() {
        for release in all().unwrap() {
            assert!(
                !release.changes.is_empty(),
                "{} says nothing",
                release.version
            );
            assert_eq!(release.version.split('.').count(), 3, "{}", release.version);
        }
    }

    #[test]
    fn this_build_appears_in_its_own_notes() {
        // Otherwise the version somebody is running is the one nothing explains.
        let releases = all().unwrap();
        assert!(
            releases.iter().any(|r| r.version == current_version()),
            "no notes for {}",
            current_version()
        );
    }

    #[test]
    fn releases_come_back_newest_first() {
        let versions: Vec<_> = all().unwrap().into_iter().map(|r| r.version).collect();
        let mut sorted = versions.clone();
        sorted.sort_by(|a, b| compare(b, a));
        assert_eq!(versions, sorted);
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        // The case string comparison gets wrong, and gets wrong silently.
        assert_eq!(compare("0.10.0", "0.9.0"), Ordering::Greater);
        assert_eq!(compare("1.0.0", "0.99.99"), Ordering::Greater);
        assert_eq!(compare("0.1.2", "0.1.2"), Ordering::Equal);
    }

    #[test]
    fn a_version_that_makes_no_sense_sorts_low_rather_than_failing() {
        assert_eq!(compare("nonsense", "0.0.1"), Ordering::Less);
    }

    #[test]
    fn a_first_run_is_not_shown_a_history_it_was_not_here_for() {
        assert!(unseen(None).unwrap().is_empty());
    }

    #[test]
    fn only_what_came_after_the_last_seen_version_is_unseen() {
        let unseen = unseen(Some("0.0.0")).unwrap();
        assert!(!unseen.is_empty());
        assert!(
            unseen
                .iter()
                .all(|r| compare(&r.version, "0.0.0") == Ordering::Greater)
        );

        // Nothing is new when you have seen the newest.
        let newest = all().unwrap().first().unwrap().version.clone();
        assert!(super::unseen(Some(&newest)).unwrap().is_empty());
    }
}
