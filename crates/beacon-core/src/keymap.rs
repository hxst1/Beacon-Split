use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::{CoreError, Result};

/// Everything that can be given a keyboard shortcut, and what it starts as.
///
/// Only stable actions belong here. The command palette also lists things like
/// "switch to this particular project", whose identity changes with the
/// workspace — those are discoverable but not bindable, because a binding has
/// to mean the same thing next week.
///
/// The table lives in the backend so that conflict checking has one source of
/// truth. What each action *does* lives in the frontend, keyed by these ids.
pub const ACTIONS: &[(&str, &str)] = &[
    ("palette.open", "mod+k"),
    ("quickOpen.open", "mod+p"),
    ("settings.open", "mod+,"),
    ("panel.toggle.files", "mod+e"),
    ("panel.toggle.git", "mod+g"),
    ("panel.toggle.editor", "mod+o"),
    ("panel.toggle.terminal", "mod+j"),
    ("panel.fullscreen", "mod+enter"),
    // Arrows rather than letters, because Option rewrites what a letter key
    // reports on macOS and the binding would arrive as a character nobody
    // pressed.
    ("panel.focusNext", "mod+alt+arrowright"),
    ("panel.focusPrevious", "mod+alt+arrowleft"),
    ("editor.save", "mod+s"),
    ("session.restartClaude", "mod+shift+r"),
    ("project.next", "mod+shift+]"),
    ("project.previous", "mod+shift+["),
];

/// Bindings Beacon refuses to take over.
///
/// These belong to the terminal, and a session is the point of the window.
/// Rebinding copy or paste would break them inside Claude and inside every
/// shell, and the cause would be almost impossible to guess at from the
/// symptom.
const RESERVED: &[&str] = &["mod+c", "mod+v", "mod+x", "mod+a", "mod+z"];

/// One action as the settings screen needs it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionBinding {
    pub action: String,
    /// What it is bound to now.
    pub binding: String,
    /// What it would be with nothing configured.
    pub default_binding: String,
}

pub fn is_known_action(action: &str) -> bool {
    ACTIONS.iter().any(|(id, _)| *id == action)
}

pub fn default_binding(action: &str) -> Option<&'static str> {
    ACTIONS
        .iter()
        .find(|(id, _)| *id == action)
        .map(|(_, binding)| *binding)
}

/// Puts a binding into the one form everything else compares against.
///
/// Accepts what a person would type — `Cmd+Shift+R`, `ctrl+shift+r` — and
/// answers in lower case with modifiers in a fixed order, so two spellings of
/// the same shortcut can never both be stored.
pub fn normalize(binding: &str) -> Result<String> {
    let mut modifier = false;
    let mut shift = false;
    let mut alt = false;
    let mut key: Option<String> = None;

    for part in binding.split('+') {
        let part = part.trim().to_ascii_lowercase();
        if part.is_empty() {
            continue;
        }

        match part.as_str() {
            // Beacon's shortcuts are written against "the primary modifier" so
            // one table is correct on macOS and Linux; all of these mean it.
            "mod" | "cmd" | "command" | "ctrl" | "control" | "meta" | "super" => modifier = true,
            "shift" => shift = true,
            "alt" | "option" | "opt" => alt = true,
            other => {
                if key.is_some() {
                    return Err(CoreError::invalid(
                        "a shortcut can only have one key besides its modifiers",
                    ));
                }
                key = Some(other.to_string());
            }
        }
    }

    let key = key.ok_or_else(|| CoreError::invalid("a shortcut needs a key"))?;
    if !modifier {
        return Err(CoreError::invalid(
            "shortcuts must include the primary modifier, so they cannot collide with typing",
        ));
    }

    let mut parts = vec!["mod"];
    if shift {
        parts.push("shift");
    }
    if alt {
        parts.push("alt");
    }
    let normalized = format!("{}+{key}", parts.join("+"));

    if RESERVED.contains(&normalized.as_str()) {
        return Err(CoreError::invalid(format!(
            "{normalized} belongs to the terminal; rebinding it would break it inside every session"
        )));
    }

    Ok(normalized)
}

/// Every action with the binding it currently has.
pub fn resolve(overrides: &BTreeMap<String, String>) -> Vec<ActionBinding> {
    ACTIONS
        .iter()
        .map(|(action, fallback)| ActionBinding {
            action: (*action).to_string(),
            binding: overrides
                .get(*action)
                .cloned()
                .unwrap_or_else(|| (*fallback).to_string()),
            default_binding: (*fallback).to_string(),
        })
        .collect()
}

/// Which other action already answers to this binding, if any.
pub fn conflicting_action(
    overrides: &BTreeMap<String, String>,
    binding: &str,
    excluding: &str,
) -> Option<String> {
    resolve(overrides)
        .into_iter()
        .find(|entry| entry.binding == binding && entry.action != excluding)
        .map(|entry| entry.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_is_a_valid_binding() {
        for (action, binding) in ACTIONS {
            normalize(binding)
                .unwrap_or_else(|err| panic!("{action}'s default {binding} is invalid: {err}"));
        }
    }

    #[test]
    fn no_two_actions_start_out_sharing_a_binding() {
        let mut seen: Vec<&str> = ACTIONS.iter().map(|(_, binding)| *binding).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(before, seen.len(), "two actions share a default shortcut");
    }

    #[test]
    fn the_same_shortcut_spelled_differently_normalizes_the_same_way() {
        let forms = ["Cmd+Shift+R", "ctrl+shift+r", "  meta + SHIFT + r "];
        for form in forms {
            assert_eq!(normalize(form).unwrap(), "mod+shift+r", "for {form:?}");
        }
    }

    #[test]
    fn modifiers_come_back_in_a_fixed_order() {
        assert_eq!(normalize("mod+alt+shift+k").unwrap(), "mod+shift+alt+k");
    }

    #[test]
    fn a_shortcut_without_the_primary_modifier_is_refused() {
        // Otherwise it would fire while typing into the terminal.
        assert!(normalize("shift+k").is_err());
        assert!(normalize("k").is_err());
    }

    #[test]
    fn a_shortcut_with_no_key_is_refused() {
        assert!(normalize("mod+shift").is_err());
        assert!(normalize("").is_err());
    }

    #[test]
    fn two_keys_are_refused() {
        assert!(normalize("mod+k+p").is_err());
    }

    #[test]
    fn the_terminals_own_shortcuts_are_refused() {
        for reserved in ["mod+c", "Cmd+V", "ctrl+x"] {
            let error = normalize(reserved).unwrap_err().to_string();
            assert!(error.contains("terminal"), "should explain why: {error}");
        }
    }

    #[test]
    fn resolving_prefers_an_override_and_still_reports_the_default() {
        let mut overrides = BTreeMap::new();
        overrides.insert("palette.open".to_string(), "mod+shift+p".to_string());

        let resolved = resolve(&overrides);
        let palette = resolved
            .iter()
            .find(|e| e.action == "palette.open")
            .unwrap();
        assert_eq!(palette.binding, "mod+shift+p");
        assert_eq!(palette.default_binding, "mod+k");

        // Untouched actions keep theirs.
        let quick = resolved
            .iter()
            .find(|e| e.action == "quickOpen.open")
            .unwrap();
        assert_eq!(quick.binding, "mod+p");
    }

    #[test]
    fn a_binding_already_in_use_is_reported_with_who_has_it() {
        let overrides = BTreeMap::new();
        assert_eq!(
            conflicting_action(&overrides, "mod+p", "palette.open").as_deref(),
            Some("quickOpen.open")
        );
        // An action does not conflict with itself.
        assert_eq!(
            conflicting_action(&overrides, "mod+k", "palette.open"),
            None
        );
    }

    #[test]
    fn a_binding_freed_by_an_override_stops_conflicting() {
        let mut overrides = BTreeMap::new();
        overrides.insert("quickOpen.open".to_string(), "mod+shift+p".to_string());
        assert_eq!(
            conflicting_action(&overrides, "mod+p", "palette.open"),
            None
        );
    }
}
