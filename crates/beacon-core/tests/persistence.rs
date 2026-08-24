//! End-to-end checks that state survives a restart of the process.

use std::path::PathBuf;

use beacon_core::Beacon;
use beacon_core::layout::{LayoutNode, LayoutPreset, PanelId, SplitDirection};

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    (dir, config)
}

#[test]
fn workspaces_and_projects_survive_a_reload() {
    let (_guard, config) = scratch();
    let project_dir = _guard.path().join("some-app");
    std::fs::create_dir_all(project_dir.join(".git")).unwrap();
    std::fs::write(project_dir.join("Cargo.toml"), "").unwrap();

    let project_id = {
        let mut beacon = Beacon::load(&config).unwrap();
        let ws = beacon.create_workspace("Personal", "#8B5CF6").unwrap();
        let project = beacon.add_project(&ws, &project_dir).unwrap();
        beacon.set_active_project(&ws, &project).unwrap();
        project
    };

    let reloaded = Beacon::load(&config).unwrap().snapshot();
    assert_eq!(reloaded.workspaces.len(), 1);
    let workspace = &reloaded.workspaces[0];
    assert_eq!(workspace.name, "Personal");
    assert_eq!(workspace.accent, "#8b5cf6");
    assert_eq!(workspace.projects.len(), 1);
    assert_eq!(workspace.projects[0].name, "some-app");
    assert!(!workspace.projects[0].kinds.is_empty());
    assert_eq!(
        reloaded.active_project.get(&workspace.id),
        Some(&project_id)
    );
}

#[test]
fn adding_the_same_folder_twice_does_not_duplicate_it() {
    let (guard, config) = scratch();
    let project_dir = guard.path().join("app");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut beacon = Beacon::load(&config).unwrap();
    let ws = beacon.create_workspace("Work", "#4f8df7").unwrap();
    let first = beacon.add_project(&ws, &project_dir).unwrap();
    let second = beacon.add_project(&ws, &project_dir).unwrap();

    assert_eq!(first, second);
    assert_eq!(beacon.snapshot().workspaces[0].projects.len(), 1);
}

#[test]
fn removing_a_project_leaves_the_folder_on_disk() {
    let (guard, config) = scratch();
    let project_dir = guard.path().join("app");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(project_dir.join("keep-me.txt"), "important").unwrap();

    let mut beacon = Beacon::load(&config).unwrap();
    let ws = beacon.create_workspace("Work", "#4f8df7").unwrap();
    let project = beacon.add_project(&ws, &project_dir).unwrap();
    beacon.remove_project(&ws, &project).unwrap();

    assert!(beacon.snapshot().workspaces[0].projects.is_empty());
    assert!(project_dir.join("keep-me.txt").exists());
}

#[test]
fn a_project_can_move_between_workspaces() {
    let (guard, config) = scratch();
    let project_dir = guard.path().join("app");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut beacon = Beacon::load(&config).unwrap();
    let personal = beacon.create_workspace("Personal", "#8b5cf6").unwrap();
    let work = beacon.create_workspace("Work", "#4f8df7").unwrap();
    let project = beacon.add_project(&personal, &project_dir).unwrap();

    beacon.move_project(&personal, &project, &work).unwrap();

    let snapshot = beacon.snapshot();
    let personal_view = snapshot
        .workspaces
        .iter()
        .find(|w| w.id == personal)
        .unwrap();
    let work_view = snapshot.workspaces.iter().find(|w| w.id == work).unwrap();
    assert!(personal_view.projects.is_empty());
    assert_eq!(work_view.projects.len(), 1);
}

#[test]
fn a_resized_layout_is_persisted_and_clamped() {
    let (_guard, config) = scratch();
    {
        let mut beacon = Beacon::load(&config).unwrap();
        let stretched = LayoutNode::split(
            SplitDirection::Row,
            // Beyond what a splitter allows; the backend is the last word.
            2.5,
            LayoutNode::panel(PanelId::Claude),
            LayoutNode::split(
                SplitDirection::Column,
                0.4,
                LayoutNode::split(
                    SplitDirection::Column,
                    0.7,
                    LayoutNode::panel(PanelId::Files),
                    LayoutNode::panel(PanelId::Git),
                ),
                LayoutNode::panel(PanelId::Terminal),
            ),
        );
        beacon.set_layout(stretched).unwrap();
    }

    let layout = Beacon::load(&config).unwrap().snapshot().layout;
    let LayoutNode::Split { fraction, .. } = layout else {
        panic!("expected a split")
    };
    assert_eq!(fraction, 0.9);
}

#[test]
fn choosing_a_preset_replaces_the_layout() {
    let (_guard, config) = scratch();
    {
        let mut beacon = Beacon::load(&config).unwrap();
        beacon.set_preset(LayoutPreset::ClaudeRightTall).unwrap();
    }

    let snapshot = Beacon::load(&config).unwrap().snapshot();
    assert_eq!(snapshot.preset, LayoutPreset::ClaudeRightTall);
    assert_eq!(
        snapshot.layout,
        LayoutPreset::ClaudeRightTall.tree().unwrap()
    );
}

#[test]
fn a_hidden_panel_survives_a_reload() {
    let (_guard, config) = scratch();
    {
        let mut beacon = Beacon::load(&config).unwrap();
        beacon.toggle_panel(PanelId::Git).unwrap();
    }

    let snapshot = Beacon::load(&config).unwrap().snapshot();
    assert!(snapshot.hidden.contains(&PanelId::Git));
    // Hiding does not remove it from the tree, so showing it again puts it back.
    assert!(snapshot.layout.panels().contains(&PanelId::Git));
}

#[test]
fn a_layout_without_claude_is_refused() {
    let (_guard, config) = scratch();
    let mut beacon = Beacon::load(&config).unwrap();
    let broken = LayoutNode::split(
        SplitDirection::Row,
        0.5,
        LayoutNode::panel(PanelId::Files),
        LayoutNode::panel(PanelId::Terminal),
    );
    assert!(beacon.set_layout(broken).is_err());
}

#[test]
fn a_layout_that_repeats_a_panel_is_refused() {
    let (_guard, config) = scratch();
    let mut beacon = Beacon::load(&config).unwrap();
    let broken = LayoutNode::split(
        SplitDirection::Row,
        0.5,
        LayoutNode::panel(PanelId::Claude),
        LayoutNode::panel(PanelId::Claude),
    );
    assert!(beacon.set_layout(broken).is_err());
}

#[test]
fn the_editor_starts_hidden_so_nothing_empty_is_shown() {
    let (_guard, config) = scratch();
    let snapshot = Beacon::load(&config).unwrap().snapshot();
    assert!(snapshot.hidden.contains(&PanelId::Editor));
    assert!(snapshot.layout.panels().contains(&PanelId::Editor));
}

#[test]
fn deleting_a_workspace_moves_the_active_selection_elsewhere() {
    let (_guard, config) = scratch();
    let mut beacon = Beacon::load(&config).unwrap();
    let first = beacon.create_workspace("First", "#4f8df7").unwrap();
    let second = beacon.create_workspace("Second", "#8b5cf6").unwrap();

    beacon.set_active_workspace(&second).unwrap();
    beacon.delete_workspace(&second).unwrap();

    assert_eq!(beacon.snapshot().active_workspace, Some(first));
}

#[test]
fn a_v1_ui_state_is_upgraded_on_disk_once() {
    let (_guard, config) = scratch();
    std::fs::create_dir_all(&config).unwrap();
    let path = config.join("ui-state.json");
    std::fs::write(
        &path,
        r#"{
            "schemaVersion": 1,
            "panels": {
                "sideFraction": 0.3,
                "terminalFraction": 0.25,
                "sideVisible": true,
                "terminalVisible": true
            }
        }"#,
    )
    .unwrap();

    // Loading migrates and writes back, so the upgrade is not redone next time.
    let snapshot = Beacon::load(&config).unwrap().snapshot();
    snapshot.layout.validate().unwrap();

    let upgraded: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(upgraded["schemaVersion"], 2);
    assert!(upgraded.get("panels").is_none());
}

#[test]
fn a_layout_stored_before_the_editor_existed_gains_it_hidden() {
    // The exact document a build without an editor panel would have written.
    let (_guard, config) = scratch();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("ui-state.json"),
        r#"{
            "schemaVersion": 2,
            "preset": "claude-left",
            "hidden": [],
            "layout": {
                "type": "split", "direction": "column", "fraction": 0.72,
                "first": {
                    "type": "split", "direction": "row", "fraction": 0.74,
                    "first": { "type": "panel", "panel": "claude" },
                    "second": {
                        "type": "split", "direction": "column", "fraction": 0.6,
                        "first": { "type": "panel", "panel": "files" },
                        "second": { "type": "panel", "panel": "git" }
                    }
                },
                "second": { "type": "panel", "panel": "terminal" }
            }
        }"#,
    )
    .unwrap();

    let snapshot = Beacon::load(&config).unwrap().snapshot();
    assert!(
        snapshot.layout.panels().contains(&PanelId::Editor),
        "the editor should have been added to the layout"
    );
    assert!(
        snapshot.hidden.contains(&PanelId::Editor),
        "a panel introduced by a repair must start hidden, not show an empty pane"
    );
}

#[test]
fn a_rebound_shortcut_survives_a_reload() {
    let (_guard, config) = scratch();
    {
        let mut beacon = Beacon::load(&config).unwrap();
        beacon
            .set_binding("palette.open", Some("Cmd+Shift+P"))
            .unwrap();
    }

    let bindings = Beacon::load(&config).unwrap().snapshot().bindings;
    let palette = bindings
        .iter()
        .find(|b| b.action == "palette.open")
        .unwrap();
    assert_eq!(palette.binding, "mod+shift+p", "stored in normalised form");
    assert_eq!(palette.default_binding, "mod+k");
}

#[test]
fn a_shortcut_another_action_already_has_is_refused_by_name() {
    let (_guard, config) = scratch();
    let mut beacon = Beacon::load(&config).unwrap();

    let error = beacon
        .set_binding("palette.open", Some("mod+p"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("quickOpen.open"),
        "should say who has it; got: {error}"
    );
}

#[test]
fn clearing_a_binding_returns_it_to_the_default() {
    let (_guard, config) = scratch();
    let mut beacon = Beacon::load(&config).unwrap();

    beacon
        .set_binding("panel.toggle.git", Some("mod+shift+g"))
        .unwrap();
    beacon.set_binding("panel.toggle.git", None).unwrap();

    let bindings = beacon.snapshot().bindings;
    let git = bindings
        .iter()
        .find(|b| b.action == "panel.toggle.git")
        .unwrap();
    assert_eq!(git.binding, git.default_binding);
}

#[test]
fn binding_an_action_to_its_own_default_stores_nothing() {
    let (_guard, config) = scratch();
    std::fs::create_dir_all(&config).unwrap();
    {
        let mut beacon = Beacon::load(&config).unwrap();
        beacon.set_binding("palette.open", Some("mod+k")).unwrap();
    }

    // Otherwise a later change to the default would never reach them.
    let stored = std::fs::read_to_string(config.join("settings.json")).unwrap_or_default();
    assert!(!stored.contains("palette.open"), "got: {stored}");
}

#[test]
fn an_unknown_action_cannot_be_bound() {
    let (_guard, config) = scratch();
    let mut beacon = Beacon::load(&config).unwrap();
    assert!(beacon.set_binding("nothing.here", Some("mod+y")).is_err());
}
