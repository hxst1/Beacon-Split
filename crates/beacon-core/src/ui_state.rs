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
    pub side_visible: bool,
    pub terminal_visible: bool,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            side_fraction: 0.26,
            terminal_fraction: 0.28,
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
            side_visible: true,
            terminal_visible: false,
        };
        let safe = wild.clamped();
        assert_eq!(safe.side_fraction, 0.45);
        assert_eq!(safe.terminal_fraction, 0.12);
        assert!(!safe.terminal_visible);
    }
}
