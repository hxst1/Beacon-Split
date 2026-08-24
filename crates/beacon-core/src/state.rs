use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::detect::{ProjectKind, detect_kinds, suggest_name};
use crate::domain::{Project, ProjectId, Workspace, WorkspaceId, WorkspacesFile, normalize_accent};
use crate::error::{CoreError, Result};
use crate::layout::{LayoutNode, LayoutPreset, PanelId};
use crate::paths::{ProjectPath, default_config_dir};
use crate::settings::Settings;
use crate::store::{JsonStore, ensure_schema};
use crate::ui_state::UiState;

/// A project flattened for the UI: identity plus the paths the frontend would
/// otherwise have to reconstruct itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: ProjectId,
    pub name: String,
    pub absolute_path: String,
    pub display_path: String,
    pub kinds: Vec<ProjectKind>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub id: WorkspaceId,
    pub name: String,
    pub accent: String,
    pub icon: Option<String>,
    pub projects: Vec<ProjectView>,
}

/// One payload that fully describes the app to the frontend.
///
/// The UI never asks for pieces: every mutation returns a fresh snapshot, which
/// removes a whole class of "frontend and backend disagree" bugs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub workspaces: Vec<WorkspaceView>,
    pub active_workspace: Option<WorkspaceId>,
    pub active_project: std::collections::BTreeMap<WorkspaceId, ProjectId>,
    pub layout: LayoutNode,
    pub preset: LayoutPreset,
    pub hidden: Vec<PanelId>,
    pub projects_home: String,
}

/// Owns Beacon's persisted state and every operation on it.
///
/// This type knows nothing about Tauri or about windows. When session and
/// process management move into a background daemon, this is what moves with
/// them; the Tauri layer becomes a client.
pub struct Beacon {
    settings_store: JsonStore,
    workspaces_store: JsonStore,
    ui_store: JsonStore,

    settings: Settings,
    workspaces: WorkspacesFile,
    ui: UiState,
}

impl Beacon {
    /// Loads state from `config_dir`, creating nothing until something is saved.
    pub fn load(config_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = config_dir.as_ref();
        let settings_store = JsonStore::new(dir.join("settings.json"));
        let workspaces_store = JsonStore::new(dir.join("workspaces.json"));
        let ui_store = JsonStore::new(dir.join("ui-state.json"));

        let settings: Settings = settings_store.read()?;
        ensure_schema(
            settings_store.path(),
            settings.schema_version,
            Settings::SCHEMA_VERSION,
        )?;

        let workspaces: WorkspacesFile = workspaces_store.read()?;
        ensure_schema(
            workspaces_store.path(),
            workspaces.schema_version,
            WorkspacesFile::SCHEMA_VERSION,
        )?;

        // The UI document is read through its own loader: its shape depends on
        // the stored schema version, and an old one is migrated rather than
        // discarded.
        let raw = ui_store.read_raw()?;
        let stored_version = raw
            .as_ref()
            .and_then(|value| value.get("schemaVersion"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(UiState::SCHEMA_VERSION as u64) as u32;

        let ui = match raw {
            Some(value) => UiState::from_json(ui_store.path(), value)?,
            None => UiState::default(),
        };

        // Write a migrated document back immediately. Otherwise the upgrade is
        // redone on every launch until something else happens to save.
        if stored_version < UiState::SCHEMA_VERSION {
            ui_store.write(&ui)?;
        }

        Ok(Self {
            settings_store,
            workspaces_store,
            ui_store,
            settings,
            workspaces,
            ui,
        })
    }

    pub fn load_default() -> Result<Self> {
        Self::load(default_config_dir())
    }

    pub fn projects_home(&self) -> PathBuf {
        self.settings.projects_home_path()
    }

    // ---- reads -------------------------------------------------------------

    pub fn snapshot(&self) -> Snapshot {
        let home = self.projects_home();
        Snapshot {
            workspaces: self
                .workspaces
                .workspaces
                .iter()
                .map(|w| self.workspace_view(w, &home))
                .collect(),
            active_workspace: self.ui.active_workspace.clone(),
            active_project: self.ui.active_project.clone(),
            layout: self.ui.layout.clamped(),
            preset: self.ui.preset,
            hidden: self.ui.hidden.clone(),
            projects_home: home.to_string_lossy().into_owned(),
        }
    }

    fn workspace_view(&self, workspace: &Workspace, home: &Path) -> WorkspaceView {
        WorkspaceView {
            id: workspace.id.clone(),
            name: workspace.name.clone(),
            accent: workspace.accent.clone(),
            icon: workspace.icon.clone(),
            projects: workspace
                .projects
                .iter()
                .map(|p| ProjectView {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    absolute_path: p.path.resolve(home).to_string_lossy().into_owned(),
                    display_path: p.path.display(home),
                    kinds: p.kinds.clone(),
                })
                .collect(),
        }
    }

    pub fn resolve_project_path(
        &self,
        workspace: &WorkspaceId,
        project: &ProjectId,
    ) -> Result<PathBuf> {
        let home = self.projects_home();
        Ok(self
            .workspace(workspace)?
            .project(project)
            .ok_or_else(|| CoreError::ProjectNotFound(project.to_string()))?
            .path
            .resolve(&home))
    }

    /// The projects a workspace holds, for callers that need to act on all of
    /// them before the workspace goes away.
    pub fn project_ids(&self, workspace: &WorkspaceId) -> Result<Vec<ProjectId>> {
        Ok(self
            .workspace(workspace)?
            .projects
            .iter()
            .map(|project| project.id.clone())
            .collect())
    }

    fn workspace(&self, id: &WorkspaceId) -> Result<&Workspace> {
        self.workspaces
            .workspaces
            .iter()
            .find(|w| &w.id == id)
            .ok_or_else(|| CoreError::WorkspaceNotFound(id.to_string()))
    }

    fn workspace_mut(&mut self, id: &WorkspaceId) -> Result<&mut Workspace> {
        self.workspaces
            .workspaces
            .iter_mut()
            .find(|w| &w.id == id)
            .ok_or_else(|| CoreError::WorkspaceNotFound(id.to_string()))
    }

    // ---- workspace mutations ----------------------------------------------

    pub fn create_workspace(&mut self, name: &str, accent: &str) -> Result<WorkspaceId> {
        let name = require_name(name)?;
        let workspace = Workspace::new(name, accent)?;
        let id = workspace.id.clone();
        self.workspaces.workspaces.push(workspace);
        if self.ui.active_workspace.is_none() {
            self.ui.active_workspace = Some(id.clone());
            self.save_ui()?;
        }
        self.save_workspaces()?;
        Ok(id)
    }

    pub fn update_workspace(
        &mut self,
        id: &WorkspaceId,
        name: Option<&str>,
        accent: Option<&str>,
        icon: Option<Option<&str>>,
    ) -> Result<()> {
        let name = name.map(require_name).transpose()?;
        let accent = accent
            .map(|a| normalize_accent(a.to_string()))
            .transpose()?;

        let workspace = self.workspace_mut(id)?;
        if let Some(name) = name {
            workspace.name = name;
        }
        if let Some(accent) = accent {
            workspace.accent = accent;
        }
        if let Some(icon) = icon {
            workspace.icon = icon.map(str::to_string);
        }
        self.save_workspaces()
    }

    /// Forgets a workspace and its project entries. Never touches the folders
    /// those projects point at.
    pub fn delete_workspace(&mut self, id: &WorkspaceId) -> Result<()> {
        let before = self.workspaces.workspaces.len();
        self.workspaces.workspaces.retain(|w| &w.id != id);
        if self.workspaces.workspaces.len() == before {
            return Err(CoreError::WorkspaceNotFound(id.to_string()));
        }

        self.ui.active_project.remove(id);
        if self.ui.active_workspace.as_ref() == Some(id) {
            self.ui.active_workspace = self.workspaces.workspaces.first().map(|w| w.id.clone());
        }
        self.save_workspaces()?;
        self.save_ui()
    }

    // ---- project mutations -------------------------------------------------

    /// Adds a folder to a workspace, sniffing what kind of project it is.
    pub fn add_project(
        &mut self,
        workspace_id: &WorkspaceId,
        absolute_path: &Path,
    ) -> Result<ProjectId> {
        if !absolute_path.is_dir() {
            return Err(CoreError::invalid(format!(
                "{} is not a folder",
                absolute_path.display()
            )));
        }

        let home = self.projects_home();
        let canonical = absolute_path
            .canonicalize()
            .map_err(|err| CoreError::io(absolute_path, err))?;
        let stored = ProjectPath::from_absolute(&canonical, &home);

        let workspace = self.workspace_mut(workspace_id)?;
        if let Some(existing) = workspace.projects.iter().find(|p| p.path == stored) {
            return Ok(existing.id.clone());
        }

        let project = Project::new(suggest_name(&canonical), stored, detect_kinds(&canonical));
        let id = project.id.clone();
        workspace.projects.push(project);
        self.save_workspaces()?;
        Ok(id)
    }

    pub fn rename_project(
        &mut self,
        workspace_id: &WorkspaceId,
        project_id: &ProjectId,
        name: &str,
    ) -> Result<()> {
        let name = require_name(name)?;
        let project = self
            .workspace_mut(workspace_id)?
            .project_mut(project_id)
            .ok_or_else(|| CoreError::ProjectNotFound(project_id.to_string()))?;
        project.name = name;
        self.save_workspaces()
    }

    /// Removes a project from Beacon. This is not a delete: the folder on disk
    /// is left completely untouched.
    pub fn remove_project(
        &mut self,
        workspace_id: &WorkspaceId,
        project_id: &ProjectId,
    ) -> Result<()> {
        let workspace = self.workspace_mut(workspace_id)?;
        let before = workspace.projects.len();
        workspace.projects.retain(|p| &p.id != project_id);
        if workspace.projects.len() == before {
            return Err(CoreError::ProjectNotFound(project_id.to_string()));
        }

        if self.ui.active_project.get(workspace_id) == Some(project_id) {
            self.ui.active_project.remove(workspace_id);
        }
        self.save_workspaces()?;
        self.save_ui()
    }

    pub fn move_project(
        &mut self,
        from: &WorkspaceId,
        project_id: &ProjectId,
        to: &WorkspaceId,
    ) -> Result<()> {
        if from == to {
            return Ok(());
        }
        // Fail before mutating anything if the destination is bogus.
        self.workspace(to)?;

        let source = self.workspace_mut(from)?;
        let index = source
            .projects
            .iter()
            .position(|p| &p.id == project_id)
            .ok_or_else(|| CoreError::ProjectNotFound(project_id.to_string()))?;
        let project = source.projects.remove(index);

        self.workspace_mut(to)?.projects.push(project);
        if self.ui.active_project.get(from) == Some(project_id) {
            self.ui.active_project.remove(from);
        }
        self.save_workspaces()?;
        self.save_ui()
    }

    // ---- ui state ----------------------------------------------------------

    pub fn set_active_workspace(&mut self, id: &WorkspaceId) -> Result<()> {
        self.workspace(id)?;
        self.ui.active_workspace = Some(id.clone());
        self.save_ui()
    }

    pub fn set_active_project(
        &mut self,
        workspace_id: &WorkspaceId,
        project_id: &ProjectId,
    ) -> Result<()> {
        let workspace = self.workspace(workspace_id)?;
        if workspace.project(project_id).is_none() {
            return Err(CoreError::ProjectNotFound(project_id.to_string()));
        }
        self.ui.active_workspace = Some(workspace_id.clone());
        self.ui
            .active_project
            .insert(workspace_id.clone(), project_id.clone());
        self.save_ui()
    }

    /// Replaces the layout tree, e.g. after dragging a splitter.
    ///
    /// Resizing keeps the current preset; only a rearrangement makes the layout
    /// custom, which is what `set_preset` and a future editor go through.
    pub fn set_layout(&mut self, layout: LayoutNode) -> Result<()> {
        layout.validate()?;
        self.ui.layout = layout.clamped();
        self.save_ui()
    }

    /// Switches to one of the built-in arrangements.
    pub fn set_preset(&mut self, preset: LayoutPreset) -> Result<()> {
        let Some(tree) = preset.tree() else {
            return Err(CoreError::invalid(
                "a custom layout is chosen by arranging panels, not by name",
            ));
        };
        self.ui.preset = preset;
        self.ui.layout = tree;
        self.save_ui()
    }

    /// Shows or hides a panel, leaving its place in the tree intact.
    pub fn toggle_panel(&mut self, panel: PanelId) -> Result<()> {
        self.ui.toggle(panel);
        self.ui = self.ui.normalized();
        self.save_ui()
    }

    // ---- persistence -------------------------------------------------------

    fn save_workspaces(&self) -> Result<()> {
        self.workspaces_store.write(&self.workspaces)
    }

    fn save_ui(&self) -> Result<()> {
        self.ui_store.write(&self.ui)
    }

    pub fn save_settings(&self) -> Result<()> {
        self.settings_store.write(&self.settings)
    }
}

fn require_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CoreError::invalid("name cannot be empty"));
    }
    Ok(trimmed.to_string())
}
