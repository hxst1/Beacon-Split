use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::appearance::Appearance;
use crate::error::{CoreError, Result};
use crate::paths::default_projects_home;

fn yes() -> bool {
    true
}

/// A shell and how to start it.
///
/// Arguments are configurable because "login shell" is spelled differently
/// enough to matter: `-l` for zsh, bash and fish, nothing at all for nushell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Application-wide preferences. Deliberately small: anything that belongs to a
/// workspace lives in `workspaces.json`, anything ephemeral lives in
/// `ui-state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    /// Root that portable project paths are stored relative to.
    /// `None` means "use this machine's default", which is what keeps the file
    /// itself portable between macOS and Linux.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects_home: Option<String>,
    /// Keyboard bindings, keyed by action id.
    ///
    /// Only the ones that differ from the default are stored, so changing a
    /// default in a later version reaches everyone who never overrode it, and
    /// no stale entry is left behind pointing at an action that moved on.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, String>,
    /// What a terminal runs, when the user wants something other than the
    /// shell their account is set to.
    ///
    /// Beacon is the terminal emulator, so this is a shell — zsh, fish, nu —
    /// and not another emulator. `None` means `$SHELL`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellSpec>,
    /// Whether Beacon may raise a system notification when a project stops and
    /// waits for an answer.
    ///
    /// On by default: knowing which of several projects needs you is the whole
    /// reason the activity states exist, and it is no use if it stays inside a
    /// window you are not looking at.
    #[serde(default = "yes")]
    pub notifications: bool,
    /// Which moments make a sound.
    ///
    /// The same moments a notification is for — Claude waiting for an answer,
    /// a long turn finished — and, like a notification, only when the project
    /// is not the one being looked at. Separate from notifications because
    /// they reach people differently: a banner waits on a screen, a sound
    /// reaches someone who is not facing one.
    #[serde(default)]
    pub sounds: Sounds,
    /// The last version whose notes were shown.
    ///
    /// Absent means this install has never shown any, which is a first run —
    /// and a first run should not be met with a history of releases the user
    /// was not here for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_version: Option<String>,
    /// Whether a new version announces itself on start.
    ///
    /// Muting silences the announcement, not the information: the bell still
    /// shows there is something to read, and reading it is a click away.
    #[serde(default = "yes")]
    pub release_notices: bool,
    /// Whether the welcome guide has been through, finished or skipped.
    ///
    /// False only for a fresh install. A settings file from before the guide
    /// existed belongs to somebody who found their way without it, and meeting
    /// them with it after an update would be the interruption it is meant to
    /// spare a newcomer.
    #[serde(default = "yes")]
    pub welcomed: bool,
    /// Whether Beacon offers its own subagents to the sessions it starts.
    ///
    /// On by default, and a switch because it is not free: the agents'
    /// descriptions and the routing policy sit in the main conversation's
    /// context in every session, used or not. Somebody whose projects already
    /// define their own agents should be able to say so.
    #[serde(default = "yes")]
    pub claude_agents: bool,
    /// Whether the file tree lists dotfiles.
    ///
    /// Off by default: `.git` and friends are noise most of the time, and the
    /// toggle is right there in the panel for the times they are not.
    #[serde(default)]
    pub show_hidden_files: bool,
    /// How the window looks. Stored whole rather than only when changed: unlike
    /// a shortcut, every field here is always in effect.
    #[serde(default)]
    pub appearance: Appearance,
    /// The typeface every terminal draws in.
    #[serde(default)]
    pub terminal_font: TerminalFont,
}

impl Settings {
    pub const SCHEMA_VERSION: u32 = 1;

    pub fn projects_home_path(&self) -> PathBuf {
        self.projects_home
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(default_projects_home)
    }
}

/// The moments that make a sound. Both off until switched on, unlike
/// notifications: a banner waits quietly on a screen, but a sound reaches the
/// whole room, and nobody updating Beacon should start hearing it unasked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sounds {
    /// Claude stopped and is waiting for an answer.
    #[serde(default)]
    pub waiting: bool,
    /// A turn long enough to have walked away from has finished.
    #[serde(default)]
    pub done: bool,
}

/// The typeface every terminal draws in.
///
/// A family is a name the system resolves, not a file Beacon ships: what
/// somebody wants here is the font their other terminal already uses, and a
/// Nerd Font's icons only line up in the grid when the font that has them is
/// the one being drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalFont {
    /// `None` is Beacon's own list of likely monospace fonts. A family chosen
    /// here goes in front of that list rather than replacing it, so a name
    /// this machine does not have still lands on something monospaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Points, as the CSS pixel the webview draws in.
    #[serde(default = "default_font_size")]
    pub size: u8,
}

fn default_font_size() -> u8 {
    TerminalFont::DEFAULT_SIZE
}

impl Default for TerminalFont {
    fn default() -> Self {
        Self {
            family: None,
            size: Self::DEFAULT_SIZE,
        }
    }
}

impl TerminalFont {
    pub const DEFAULT_SIZE: u8 = 12;
    pub const MIN_SIZE: u8 = 8;
    pub const MAX_SIZE: u8 = 32;
    /// Longer than any family name anybody has installed.
    const MAX_FAMILY: usize = 120;

    /// Refuses a family that would not survive being quoted into a CSS font
    /// list, and pulls the size into a range that still makes a terminal.
    pub fn validated(&self) -> Result<Self> {
        let family = match self.family.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(name) if name.chars().count() > Self::MAX_FAMILY => {
                return Err(CoreError::invalid("that font name is too long"));
            }
            // The name is quoted into the list xterm hands the canvas, so a
            // quote or a separator in it would end it early and start
            // something else.
            Some(name)
                if name
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '"' | '\'' | '\\' | ',' | ';')) =>
            {
                return Err(CoreError::invalid(
                    "a font name cannot contain quotes, commas or semicolons",
                ));
            }
            Some(name) => Some(name.to_string()),
        };
        Ok(Self {
            family,
            size: self.size.clamp(Self::MIN_SIZE, Self::MAX_SIZE),
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            projects_home: None,
            bindings: BTreeMap::new(),
            shell: None,
            notifications: true,
            sounds: Sounds::default(),
            last_seen_version: None,
            release_notices: true,
            welcomed: false,
            show_hidden_files: false,
            claude_agents: true,
            appearance: Appearance::default(),
            terminal_font: TerminalFont::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_projects_home_falls_back_to_the_machine_default() {
        assert_eq!(
            Settings::default().projects_home_path(),
            default_projects_home()
        );
    }

    #[test]
    fn a_fresh_configuration_stores_no_bindings() {
        // Defaults are not written out: they are not the user's choices.
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!json.contains("bindings"), "got {json}");
    }

    #[test]
    fn an_explicit_projects_home_wins() {
        let settings = Settings {
            projects_home: Some("/srv/code".into()),
            ..Settings::default()
        };
        assert_eq!(settings.projects_home_path(), PathBuf::from("/srv/code"));
    }

    #[test]
    fn a_settings_file_from_before_sounds_has_both_off() {
        let old: Settings = serde_json::from_str(r#"{ "schemaVersion": 1 }"#).unwrap();
        assert_eq!(
            old.sounds,
            Sounds {
                waiting: false,
                done: false
            }
        );
    }

    #[test]
    fn only_a_fresh_install_is_welcomed() {
        assert!(!Settings::default().welcomed);

        // Somebody who already had a settings file found their way without it.
        let old: Settings = serde_json::from_str(r#"{ "schemaVersion": 1 }"#).unwrap();
        assert!(old.welcomed);

        // And a fresh install that has not been through it yet stays that way
        // across a save, even though the field is now written out.
        let saved = serde_json::to_string(&Settings::default()).unwrap();
        let back: Settings = serde_json::from_str(&saved).unwrap();
        assert!(!back.welcomed);
    }

    #[test]
    fn sounds_switched_off_stay_off() {
        let settings = Settings {
            sounds: Sounds {
                waiting: true,
                done: false,
            },
            ..Settings::default()
        };
        let back: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(
            back.sounds,
            Sounds {
                waiting: true,
                done: false
            }
        );
    }

    #[test]
    fn a_settings_file_from_before_the_terminal_font_keeps_the_default() {
        let old: Settings = serde_json::from_str(r#"{ "schemaVersion": 1 }"#).unwrap();
        assert_eq!(old.terminal_font, TerminalFont::default());
    }

    #[test]
    fn a_blank_font_name_means_beacons_own_list() {
        let font = TerminalFont {
            family: Some("   ".into()),
            size: 12,
        };
        assert_eq!(font.validated().unwrap().family, None);
    }

    #[test]
    fn a_font_name_is_kept_trimmed() {
        let font = TerminalFont {
            family: Some(" JetBrainsMono Nerd Font Mono ".into()),
            size: 13,
        };
        assert_eq!(
            font.validated().unwrap().family.as_deref(),
            Some("JetBrainsMono Nerd Font Mono")
        );
    }

    /// The name is quoted into a CSS font list; one that closes the quote
    /// would hand the canvas whatever came after it.
    #[test]
    fn a_font_name_that_would_break_out_of_its_quotes_is_refused() {
        for name in ["Mono', serif", "A\"B", "Mono; color: red", "Back\\slash"] {
            let font = TerminalFont {
                family: Some(name.into()),
                size: 12,
            };
            assert!(font.validated().is_err(), "accepted {name:?}");
        }
    }

    #[test]
    fn a_terminal_font_size_stays_readable() {
        let tiny = TerminalFont {
            family: None,
            size: 1,
        };
        let huge = TerminalFont {
            family: None,
            size: 200,
        };
        assert_eq!(tiny.validated().unwrap().size, TerminalFont::MIN_SIZE);
        assert_eq!(huge.validated().unwrap().size, TerminalFont::MAX_SIZE);
    }
}
