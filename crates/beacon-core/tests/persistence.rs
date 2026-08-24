//! End-to-end checks that state survives a restart of the process.

use std::path::PathBuf;

use beacon_core::Beacon;
use beacon_core::ui_state::PanelLayout;

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
fn panel_layout_is_persisted_and_clamped() {
    let (_guard, config) = scratch();
    {
        let mut beacon = Beacon::load(&config).unwrap();
        beacon
            .set_panels(PanelLayout {
                side_fraction: 0.9,
                terminal_fraction: 0.3,
                side_visible: true,
                terminal_visible: false,
            })
            .unwrap();
    }

    let panels = Beacon::load(&config).unwrap().snapshot().panels;
    assert_eq!(panels.side_fraction, 0.45);
    assert_eq!(panels.terminal_fraction, 0.3);
    assert!(!panels.terminal_visible);
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
