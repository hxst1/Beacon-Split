use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::{ProjectId, WorkspaceId};

/// Panel geometry, as fractions of the window so it survives resizing and
/// moving between displays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelLayout {
    /// Width of the Files/Git column, 0..1 of the window width.
    pub side_fraction: f32,
    /// Height of the terminal, 0..1 of the window height.
    pub terminal_fraction: f32,
    /// Share of the side column given to Files, 0..1, measured from the top.
    /// Defaulted so a `ui-state.json` written before this field existed still
    /// loads rather than failing the whole document.
    #[serde(default = "default_files_fraction")]
    pub files_fraction: f32,
    pub side_visible: bool,
    pub terminal_visible: bool,
}

fn default_files_fraction() -> f32 {
    0.6
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            side_fraction: 0.26,
            terminal_fraction: 0.28,
            files_fraction: default_files_fraction(),
            side_visible: true,
            terminal_visible: true,
        }
    }
}

impl PanelLayout {
    /// Keeps Claude's share of the window from collapsing, whatever the stored
    /// values or a stray drag say.
    pub fn clamped(&self) -> Self {
        Self {
            side_fraction: self.side_fraction.clamp(0.15, 0.45),
            terminal_fraction: self.terminal_fraction.clamp(0.12, 0.6),
            files_fraction: self.files_fraction.clamp(0.2, 0.85),
            side_visible: self.side_visible,
            terminal_visible: self.terminal_visible,
        }
    }
}

/// Everything needed to put the window back exactly as it was left.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workspace: Option<WorkspaceId>,
    /// Last active project per workspace, so switching workspaces returns you
    /// to where you were rather than to the first tab.
    #[serde(default)]
    pub active_project: BTreeMap<WorkspaceId, ProjectId>,
    #[serde(default)]
    pub panels: PanelLayout,
}

impl UiState {
    pub const SCHEMA_VERSION: u32 = 1;
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            active_workspace: None,
            active_project: BTreeMap::new(),
            panels: PanelLayout::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_range_panel_fractions_are_pulled_back_into_range() {
        let wild = PanelLayout {
            side_fraction: 0.95,
            terminal_fraction: 0.0,
            files_fraction: 1.4,
            side_visible: true,
            terminal_visible: false,
        };
        let safe = wild.clamped();
        assert_eq!(safe.side_fraction, 0.45);
        assert_eq!(safe.terminal_fraction, 0.12);
        assert_eq!(safe.files_fraction, 0.85);
        assert!(!safe.terminal_visible);
    }

    #[test]
    fn a_layout_saved_before_the_files_split_existed_still_loads() {
        let older = r#"{
            "sideFraction": 0.3,
            "terminalFraction": 0.25,
            "sideVisible": true,
            "terminalVisible": true
        }"#;
        let layout: PanelLayout = serde_json::from_str(older).unwrap();
        assert_eq!(layout.files_fraction, 0.6);
    }
}
