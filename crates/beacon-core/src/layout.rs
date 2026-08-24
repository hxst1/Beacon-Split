use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// The panels Beacon can place. Adding one here is the only change needed for
/// it to become placeable in every preset and in a custom layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PanelId {
    Claude,
    Editor,
    Files,
    Git,
    Terminal,
}

impl PanelId {
    pub const ALL: [PanelId; 5] = [
        PanelId::Claude,
        PanelId::Editor,
        PanelId::Files,
        PanelId::Git,
        PanelId::Terminal,
    ];

    /// Panels that start out of the way rather than showing something empty.
    pub const HIDDEN_BY_DEFAULT: [PanelId; 1] = [PanelId::Editor];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    /// Children sit side by side.
    Row,
    /// Children stack.
    Column,
}

/// How the window is divided.
///
/// A binary split tree rather than a fixed grid: the four presets, the mirror of
/// each, and anything a custom layout might want are all the same structure, so
/// there is nothing to special-case when one is chosen over another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LayoutNode {
    #[serde(rename_all = "camelCase")]
    Panel { panel: PanelId },
    #[serde(rename_all = "camelCase")]
    Split {
        direction: SplitDirection,
        /// Share of the space given to `first`, 0..1.
        fraction: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    pub fn panel(panel: PanelId) -> Self {
        Self::Panel { panel }
    }

    pub fn split(direction: SplitDirection, fraction: f32, first: Self, second: Self) -> Self {
        Self::Split {
            direction,
            fraction,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Every panel in the tree, in layout order.
    pub fn panels(&self) -> Vec<PanelId> {
        match self {
            Self::Panel { panel } => vec![*panel],
            Self::Split { first, second, .. } => {
                let mut found = first.panels();
                found.extend(second.panels());
                found
            }
        }
    }

    /// Rejects trees that place a panel twice.
    ///
    /// Completeness is deliberately not required here: a stored layout written
    /// before a panel existed is repaired rather than refused. See [`repaired`].
    pub fn validate(&self) -> Result<()> {
        let panels = self.panels();
        let mut seen = panels.clone();
        seen.sort();
        seen.dedup();

        if seen.len() != panels.len() {
            return Err(CoreError::invalid("a layout cannot place a panel twice"));
        }
        if !panels.contains(&PanelId::Claude) {
            return Err(CoreError::invalid("a layout must place the Claude panel"));
        }
        Ok(())
    }

    /// Adds any panel the tree does not place yet.
    ///
    /// Adding a panel in a new version must not invalidate a layout somebody
    /// arranged. A missing panel is attached beside Claude, which is where new
    /// content belongs and is somewhere the user can find it — and it arrives
    /// hidden, so nothing moves until they ask for it.
    pub fn repaired(&self) -> Self {
        let placed = self.panels();
        let mut tree = self.clone();

        for panel in PanelId::ALL {
            if !placed.contains(&panel) {
                tree = tree.attach_beside(PanelId::Claude, panel);
            }
        }
        tree
    }

    fn attach_beside(&self, anchor: PanelId, panel: PanelId) -> Self {
        match self {
            Self::Panel { panel: existing } if *existing == anchor => Self::split(
                SplitDirection::Row,
                0.58,
                Self::panel(anchor),
                Self::panel(panel),
            ),
            Self::Panel { panel } => Self::Panel { panel: *panel },
            Self::Split {
                direction,
                fraction,
                first,
                second,
            } => Self::Split {
                direction: *direction,
                fraction: *fraction,
                first: Box::new(first.attach_beside(anchor, panel)),
                second: Box::new(second.attach_beside(anchor, panel)),
            },
        }
    }

    /// Pulls every split fraction back into a usable range.
    pub fn clamped(&self) -> Self {
        match self {
            Self::Panel { panel } => Self::Panel { panel: *panel },
            Self::Split {
                direction,
                fraction,
                first,
                second,
            } => Self::Split {
                direction: *direction,
                fraction: fraction.clamp(0.1, 0.9),
                first: Box::new(first.clamped()),
                second: Box::new(second.clamped()),
            },
        }
    }
}

/// The arrangements Beacon ships with.
///
/// `Custom` means the stored tree was edited directly and should be left alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutPreset {
    /// Claude left, Files over Git on the right, Terminal across the bottom.
    #[default]
    ClaudeLeft,
    /// The mirror: Files over Git on the left, Claude right.
    ClaudeRight,
    /// Files over Git then Terminal in a left column, Claude filling the right.
    ClaudeRightTall,
    /// The mirror: Claude filling the left column.
    ClaudeLeftTall,
    Custom,
}

impl LayoutPreset {
    /// Every preset a settings screen should offer, in display order.
    pub const CHOOSABLE: [LayoutPreset; 4] = [
        LayoutPreset::ClaudeLeft,
        LayoutPreset::ClaudeRight,
        LayoutPreset::ClaudeRightTall,
        LayoutPreset::ClaudeLeftTall,
    ];

    /// The tree for this preset, or `None` for `Custom`, which has no canonical
    /// shape.
    pub fn tree(self) -> Option<LayoutNode> {
        use PanelId::*;
        use SplitDirection::*;

        let sidebar = || {
            LayoutNode::split(
                Column,
                0.6,
                LayoutNode::panel(Files),
                LayoutNode::panel(Git),
            )
        };
        // The editor sits beside Claude, and starts hidden, so a preset looks
        // exactly the same until a file is actually opened.
        let main = || {
            LayoutNode::split(
                Row,
                0.58,
                LayoutNode::panel(Claude),
                LayoutNode::panel(Editor),
            )
        };

        Some(match self {
            Self::ClaudeLeft => LayoutNode::split(
                Column,
                0.72,
                LayoutNode::split(Row, 0.74, main(), sidebar()),
                LayoutNode::panel(Terminal),
            ),
            Self::ClaudeRight => LayoutNode::split(
                Column,
                0.72,
                LayoutNode::split(Row, 0.26, sidebar(), main()),
                LayoutNode::panel(Terminal),
            ),
            Self::ClaudeRightTall => LayoutNode::split(
                Row,
                0.32,
                LayoutNode::split(Column, 0.62, sidebar(), LayoutNode::panel(Terminal)),
                main(),
            ),
            Self::ClaudeLeftTall => LayoutNode::split(
                Row,
                0.68,
                main(),
                LayoutNode::split(Column, 0.62, sidebar(), LayoutNode::panel(Terminal)),
            ),
            Self::Custom => return None,
        })
    }
}

pub fn default_layout() -> LayoutNode {
    LayoutPreset::ClaudeLeft
        .tree()
        .expect("built-in presets always have a tree")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_places_every_panel_exactly_once() {
        for preset in LayoutPreset::CHOOSABLE {
            let tree = preset.tree().expect("a choosable preset has a tree");
            tree.validate()
                .unwrap_or_else(|err| panic!("{preset:?} is not a valid layout: {err}"));
        }
    }

    #[test]
    fn custom_has_no_canonical_tree() {
        assert!(LayoutPreset::Custom.tree().is_none());
    }

    #[test]
    fn a_layout_that_repeats_a_panel_is_rejected() {
        let tree = LayoutNode::split(
            SplitDirection::Row,
            0.5,
            LayoutNode::panel(PanelId::Claude),
            LayoutNode::panel(PanelId::Claude),
        );
        assert!(tree.validate().is_err());
    }

    #[test]
    fn a_layout_without_claude_is_rejected() {
        let tree = LayoutNode::split(
            SplitDirection::Row,
            0.5,
            LayoutNode::panel(PanelId::Files),
            LayoutNode::panel(PanelId::Terminal),
        );
        assert!(tree.validate().is_err());
    }

    #[test]
    fn a_layout_from_before_a_panel_existed_is_repaired_rather_than_refused() {
        // Exactly the shape stored by a build that had no editor panel.
        let older = LayoutNode::split(
            SplitDirection::Column,
            0.72,
            LayoutNode::split(
                SplitDirection::Row,
                0.74,
                LayoutNode::panel(PanelId::Claude),
                LayoutNode::split(
                    SplitDirection::Column,
                    0.6,
                    LayoutNode::panel(PanelId::Files),
                    LayoutNode::panel(PanelId::Git),
                ),
            ),
            LayoutNode::panel(PanelId::Terminal),
        );

        assert!(older.validate().is_ok(), "an older layout is still usable");

        let repaired = older.repaired();
        for panel in PanelId::ALL {
            assert!(
                repaired.panels().contains(&panel),
                "{panel:?} was not added"
            );
        }
        repaired.validate().unwrap();
    }

    #[test]
    fn repairing_a_complete_layout_changes_nothing() {
        let tree = LayoutPreset::ClaudeLeft.tree().unwrap();
        assert_eq!(tree.repaired(), tree);
    }

    #[test]
    fn clamping_reaches_nested_splits() {
        let tree = LayoutNode::split(
            SplitDirection::Row,
            2.0,
            LayoutNode::panel(PanelId::Claude),
            LayoutNode::split(
                SplitDirection::Column,
                -1.0,
                LayoutNode::panel(PanelId::Files),
                LayoutNode::panel(PanelId::Git),
            ),
        );

        let LayoutNode::Split {
            fraction, second, ..
        } = tree.clamped()
        else {
            panic!("expected a split")
        };
        assert_eq!(fraction, 0.9);

        let LayoutNode::Split { fraction, .. } = *second else {
            panic!("expected a nested split")
        };
        assert_eq!(fraction, 0.1);
    }

    #[test]
    fn presets_differ_from_one_another() {
        let trees: Vec<_> = LayoutPreset::CHOOSABLE
            .iter()
            .map(|preset| preset.tree().unwrap())
            .collect();

        for (i, a) in trees.iter().enumerate() {
            for b in trees.iter().skip(i + 1) {
                assert_ne!(a, b, "two presets produced the same layout");
            }
        }
    }
}
