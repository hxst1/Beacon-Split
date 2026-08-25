use beacon_core::Snapshot;
use beacon_core::appearance::Appearance;
use beacon_core::domain::WorkspaceId;
use beacon_core::layout::{LayoutNode, LayoutPreset, PanelId};
use beacon_core::settings::ShellSpec;
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    state.beacon().snapshot()
}

#[tauri::command]
pub fn create_workspace(
    state: State<'_, AppState>,
    name: String,
    accent: String,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.create_workspace(&name, &accent)?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn update_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
    name: Option<String>,
    accent: Option<String>,
    icon: Option<String>,
) -> CommandResult<Snapshot> {
    // Three intentions, not two: no `icon` field leaves it alone, an empty one
    // clears it, anything else sets it. Collapsing the first two would make
    // renaming a workspace silently remove its icon.
    let icon: Option<Option<&str>> = icon.as_deref().map(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then_some(trimmed)
    });

    let mut beacon = state.beacon();
    beacon.update_workspace(&id, name.as_deref(), accent.as_deref(), icon)?;
    Ok(beacon.snapshot())
}

/// Sets what a terminal runs, or clears it back to the account's own shell.
///
/// Beacon is the terminal emulator, so this is a shell — not another emulator.
#[tauri::command]
pub fn set_shell(state: State<'_, AppState>, shell: Option<ShellSpec>) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_shell(shell)?;
    Ok(beacon.snapshot())
}

/// Removes a workspace from Beacon. Project folders are never touched.
#[tauri::command]
pub fn delete_workspace(state: State<'_, AppState>, id: WorkspaceId) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.delete_workspace(&id)?;
    Ok(beacon.snapshot())
}

#[tauri::command]
pub fn set_active_workspace(
    state: State<'_, AppState>,
    id: WorkspaceId,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_active_workspace(&id)?;
    Ok(beacon.snapshot())
}

/// Stores a resized layout. Rejected if it would lose or duplicate a panel.
#[tauri::command]
pub fn set_layout(state: State<'_, AppState>, layout: LayoutNode) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_layout(layout)?;
    Ok(beacon.snapshot())
}

/// Switches to one of the built-in arrangements.
#[tauri::command]
pub fn set_layout_preset(
    state: State<'_, AppState>,
    preset: LayoutPreset,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_preset(preset)?;
    Ok(beacon.snapshot())
}

/// Stores how the window should look — theme, translucency, blur.
#[tauri::command]
pub fn set_appearance(
    state: State<'_, AppState>,
    appearance: Appearance,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_appearance(appearance)?;
    Ok(beacon.snapshot())
}

/// Records that the user has been shown what is new in this version.
#[tauri::command]
pub fn mark_releases_seen(state: State<'_, AppState>) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.mark_releases_seen()?;
    Ok(beacon.snapshot())
}

/// Whether a new version announces itself on start, or waits to be asked.
#[tauri::command]
pub fn set_release_notices(state: State<'_, AppState>, enabled: bool) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_release_notices(enabled)?;
    Ok(beacon.snapshot())
}

/// Everything this build has ever shipped, newest first.
#[tauri::command]
pub fn release_notes() -> CommandResult<Vec<beacon_core::releases::Release>> {
    Ok(beacon_core::releases::all()?)
}

/// Whether Beacon may interrupt with a system notification.
#[tauri::command]
pub fn set_notifications(state: State<'_, AppState>, enabled: bool) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_notifications(enabled)?;
    Ok(beacon.snapshot())
}

/// Binds an action to a shortcut, or clears it back to the default.
#[tauri::command]
pub fn set_binding(
    state: State<'_, AppState>,
    action: String,
    binding: Option<String>,
) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.set_binding(&action, binding.as_deref())?;
    Ok(beacon.snapshot())
}

/// Puts every shortcut back to how it shipped.
#[tauri::command]
pub fn reset_bindings(state: State<'_, AppState>) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.reset_bindings()?;
    Ok(beacon.snapshot())
}

/// The built-in arrangements and the tree behind each one.
///
/// Served from the backend so a settings preview is drawn from exactly the tree
/// that would be applied, and cannot drift away from it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetOption {
    preset: LayoutPreset,
    layout: LayoutNode,
}

#[tauri::command]
pub fn layout_presets() -> Vec<PresetOption> {
    LayoutPreset::CHOOSABLE
        .iter()
        .filter_map(|preset| {
            preset.tree().map(|layout| PresetOption {
                preset: *preset,
                layout,
            })
        })
        .collect()
}

/// Shows or hides a panel without moving it in the layout.
#[tauri::command]
pub fn toggle_panel(state: State<'_, AppState>, panel: PanelId) -> CommandResult<Snapshot> {
    let mut beacon = state.beacon();
    beacon.toggle_panel(panel)?;
    Ok(beacon.snapshot())
}
