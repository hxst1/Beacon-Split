use beacon_core::agent::AgentKind;
use beacon_core::domain::{ProjectId, WorkspaceId};
use beacon_core::session::SessionKind;
use beacon_core::session::{SessionInfo, SessionPrefs};
use beacon_core::workstreams::{Workstream, WorkstreamId};
use tauri::State;

use crate::error::CommandResult;
use crate::state::AppState;

/// A project's Claude conversations, and which one it is in.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workstreams {
    workstreams: Vec<Workstream>,
    current: Option<WorkstreamId>,
}

/// One conversation, and the Claude session now running it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedWorkstream {
    workstream: Workstream,
    session: SessionInfo,
}

/// An absent `agent` means Claude Code: that is what a window which has not
/// been taught about a second agent is asking for.
#[tauri::command]
pub fn list_workstreams(
    state: State<'_, AppState>,
    project_id: ProjectId,
    agent: Option<AgentKind>,
) -> CommandResult<Workstreams> {
    let (workstreams, current) = state
        .daemon()?
        .workstreams(&project_id, agent.unwrap_or_default())?;
    Ok(Workstreams {
        workstreams,
        current,
    })
}

/// Where a project's Claude runs, and what it runs in.
///
/// Read together because both come from the same lock, and separately they can
/// disagree: a project resolved against a `projectsHome` that changed between
/// the two reads would start its session somewhere else.
fn placement(
    state: &State<'_, AppState>,
    workspace_id: &WorkspaceId,
    project_id: &ProjectId,
    agent: AgentKind,
) -> CommandResult<(std::path::PathBuf, SessionPrefs)> {
    let beacon = state.beacon();
    Ok((
        beacon.session_root(workspace_id, project_id, SessionKind::for_agent(agent))?,
        SessionPrefs {
            shell: beacon.shell(),
            agents: beacon.claude_agents(),
        },
    ))
}

/// Starts a new conversation and moves the project's Claude into it.
///
/// The gesture the whole feature exists for: finishing one thing and starting
/// the next without carrying the first one's context into it.
#[tauri::command]
pub fn start_workstream(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    agent: Option<AgentKind>,
    name: Option<String>,
    cols: u16,
    rows: u16,
) -> CommandResult<OpenedWorkstream> {
    let agent = agent.unwrap_or_default();
    let (cwd, prefs) = placement(&state, &workspace_id, &project_id, agent)?;
    let (workstream, session) = state.daemon()?.start_workstream(
        &project_id,
        agent,
        name,
        &cwd,
        (cols.max(2), rows.max(2)),
        prefs,
    )?;
    Ok(OpenedWorkstream {
        workstream,
        session,
    })
}

/// Returns the project to a conversation it already has.
///
/// Refused by the daemon when that conversation is open in another Claude:
/// two processes in one conversation write over each other's history.
#[tauri::command]
pub fn resume_workstream(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    id: WorkstreamId,
    agent: Option<AgentKind>,
    cols: u16,
    rows: u16,
) -> CommandResult<OpenedWorkstream> {
    let (cwd, prefs) = placement(
        &state,
        &workspace_id,
        &project_id,
        agent.unwrap_or_default(),
    )?;
    let (workstream, session) = state.daemon()?.resume_workstream(
        &project_id,
        &id,
        &cwd,
        (cols.max(2), rows.max(2)),
        prefs,
    )?;
    Ok(OpenedWorkstream {
        workstream,
        session,
    })
}

/// Starts a new conversation carrying another's history.
///
/// What to do instead of resuming one that is already open, and how to try
/// something without spending the conversation you would want back.
#[tauri::command]
// Eight, and they are named by the caller rather than positional: this is an
// IPC boundary, and the window passes `{ workspaceId, projectId, from, agent,
// … }`. Bundling them would only move the same names one level down and make
// the frontend build an object to be unbuilt here.
#[allow(clippy::too_many_arguments)]
pub fn fork_workstream(
    state: State<'_, AppState>,
    workspace_id: WorkspaceId,
    project_id: ProjectId,
    from: WorkstreamId,
    agent: Option<AgentKind>,
    name: Option<String>,
    cols: u16,
    rows: u16,
) -> CommandResult<OpenedWorkstream> {
    let (cwd, prefs) = placement(
        &state,
        &workspace_id,
        &project_id,
        agent.unwrap_or_default(),
    )?;
    let (workstream, session) = state.daemon()?.fork_workstream(
        &project_id,
        &from,
        name,
        &cwd,
        (cols.max(2), rows.max(2)),
        prefs,
    )?;
    Ok(OpenedWorkstream {
        workstream,
        session,
    })
}

#[tauri::command]
pub fn rename_workstream(
    state: State<'_, AppState>,
    project_id: ProjectId,
    id: WorkstreamId,
    name: Option<String>,
) -> CommandResult<()> {
    Ok(state.daemon()?.rename_workstream(&project_id, &id, name)?)
}
