use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::{ProjectId, WorkspaceId};
use crate::error::{CoreError, Result};
use crate::layout::{LayoutNode, LayoutPreset, PanelId, default_layout};

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
    /// Which preset the layout came from, so settings can show what is
    /// selected. Dragging a splitter does not change this; rearranging does.
    #[serde(default)]
    pub preset: LayoutPreset,
    #[serde(default = "default_layout")]
    pub layout: LayoutNode,
    /// Panels the user has toggled off. They keep their place in the tree so
    /// showing one again puts it back where it was.
    #[serde(default)]
    pub hidden: Vec<PanelId>,
}

impl UiState {
    pub const SCHEMA_VERSION: u32 = 2;

    pub fn is_hidden(&self, panel: PanelId) -> bool {
        self.hidden.contains(&panel)
    }

    pub fn toggle(&mut self, panel: PanelId) {
        if let Some(index) = self.hidden.iter().position(|p| *p == panel) {
            self.hidden.remove(index);
        } else {
            self.hidden.push(panel);
        }
    }

    /// Reads a stored document, upgrading it if it predates the current schema.
    ///
    /// Migrating rather than resetting matters even at this size: the layout is
    /// something the user arranged by hand.
    pub fn from_json(path: &std::path::Path, value: serde_json::Value) -> Result<Self> {
        let version = value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;

        if version > Self::SCHEMA_VERSION {
            return Err(CoreError::UnsupportedSchema {
                path: path.to_path_buf(),
                found: version,
                supported: Self::SCHEMA_VERSION,
            });
        }

        if version < Self::SCHEMA_VERSION {
            return migrate_v1(value, path);
        }

        serde_json::from_value(value).map_err(|source| CoreError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Everything sanitised: fractions in range, no unknown panels hidden.
    pub fn normalized(&self) -> Self {
        let mut hidden = self.hidden.clone();
        hidden.sort();
        hidden.dedup();
        // Hiding every panel would leave an empty window.
        if hidden.len() >= PanelId::ALL.len() {
            hidden.clear();
        }

        Self {
            schema_version: Self::SCHEMA_VERSION,
            active_workspace: self.active_workspace.clone(),
            active_project: self.active_project.clone(),
            preset: self.preset,
            layout: self.layout.clamped(),
            hidden,
        }
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            active_workspace: None,
            active_project: BTreeMap::new(),
            preset: LayoutPreset::default(),
            layout: default_layout(),
            hidden: Vec::new(),
        }
    }
}

/// Schema 1 stored a fixed three-region grid rather than a layout tree.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1 {
    #[serde(default)]
    active_workspace: Option<WorkspaceId>,
    #[serde(default)]
    active_project: BTreeMap<WorkspaceId, ProjectId>,
    #[serde(default)]
    panels: Option<V1Panels>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1Panels {
    side_fraction: f32,
    terminal_fraction: f32,
    #[serde(default = "v1_files_fraction")]
    files_fraction: f32,
    side_visible: bool,
    terminal_visible: bool,
}

fn v1_files_fraction() -> f32 {
    0.6
}

/// Rebuilds the old grid as the equivalent tree, keeping the sizes the user set.
fn migrate_v1(value: serde_json::Value, path: &std::path::Path) -> Result<UiState> {
    use crate::layout::SplitDirection::{Column, Row};

    let old: V1 = serde_json::from_value(value).map_err(|source| CoreError::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    let mut state = UiState {
        active_workspace: old.active_workspace,
        active_project: old.active_project,
        ..UiState::default()
    };

    if let Some(panels) = old.panels {
        // The old fractions measured the side and terminal from the far edge;
        // a tree fraction is the share of the first child.
        let sidebar = LayoutNode::split(
            Column,
            panels.files_fraction,
            LayoutNode::panel(PanelId::Files),
            LayoutNode::panel(PanelId::Git),
        );
        state.layout = LayoutNode::split(
            Column,
            1.0 - panels.terminal_fraction,
            LayoutNode::split(
                Row,
                1.0 - panels.side_fraction,
                LayoutNode::panel(PanelId::Claude),
                sidebar,
            ),
            LayoutNode::panel(PanelId::Terminal),
        );

        if !panels.side_visible {
            state.hidden.extend([PanelId::Files, PanelId::Git]);
        }
        if !panels.terminal_visible {
            state.hidden.push(PanelId::Terminal);
        }
    }

    tracing::info!("migrated ui-state.json from schema 1 to 2");
    Ok(state.normalized())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_v1_document_keeps_its_sizes_and_hidden_panels() {
        let older = serde_json::json!({
            "schemaVersion": 1,
            "panels": {
                "sideFraction": 0.3,
                "terminalFraction": 0.25,
                "filesFraction": 0.7,
                "sideVisible": true,
                "terminalVisible": false
            }
        });

        let state = UiState::from_json(std::path::Path::new("ui-state.json"), older).unwrap();

        assert_eq!(state.schema_version, UiState::SCHEMA_VERSION);
        assert!(state.is_hidden(PanelId::Terminal));
        assert!(!state.is_hidden(PanelId::Files));

        let LayoutNode::Split { fraction, .. } = &state.layout else {
            panic!("expected a split")
        };
        // The terminal took 25% from the bottom, so the rest keeps 75%.
        assert!((fraction - 0.75).abs() < 1e-6, "got {fraction}");
        state.layout.validate().unwrap();
    }

    #[test]
    fn a_document_from_a_newer_build_is_refused_rather_than_misread() {
        let future = serde_json::json!({ "schemaVersion": 99 });
        let error = UiState::from_json(std::path::Path::new("ui-state.json"), future).unwrap_err();
        assert!(matches!(error, CoreError::UnsupportedSchema { .. }));
    }

    #[test]
    fn hiding_every_panel_is_ignored() {
        let state = UiState {
            hidden: PanelId::ALL.to_vec(),
            ..UiState::default()
        };
        assert!(state.normalized().hidden.is_empty());
    }

    #[test]
    fn toggling_a_panel_twice_restores_it() {
        let mut state = UiState::default();
        state.toggle(PanelId::Git);
        assert!(state.is_hidden(PanelId::Git));
        state.toggle(PanelId::Git);
        assert!(!state.is_hidden(PanelId::Git));
    }
}
