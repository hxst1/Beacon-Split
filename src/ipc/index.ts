/**
 * The only module that talks to Tauri.
 *
 * Components import from here, never from `@tauri-apps/api` directly, so the
 * transport can change — most likely to a daemon connection — without touching
 * the UI.
 */

import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

import type {
  Appearance,
  DirEntry,
  ShellSpec,
  HookStatus,
  Requirement,
  Integration,
  UsageReport,
  GitStatus,
  EnvEntry,
  FileRead,
  WriteOutcome,
  HostPlatform,
  LayoutNode,
  LayoutPreset,
  PanelId,
  ScrollbackSnapshot,
  SessionInfo,
  SessionKind,
  Snapshot,
} from '@/types/beacon'

export const ipc = {
  getSnapshot: () => invoke<Snapshot>('get_snapshot'),

  createWorkspace: (name: string, accent: string) =>
    invoke<Snapshot>('create_workspace', { name, accent }),

  /** An absent `icon` leaves it alone; an empty one clears it. */
  updateWorkspace: (
    id: string,
    changes: { name?: string; accent?: string; icon?: string },
  ) =>
    invoke<Snapshot>('update_workspace', {
      id,
      name: changes.name ?? null,
      accent: changes.accent ?? null,
      icon: changes.icon ?? null,
    }),

  deleteWorkspace: (id: string) => invoke<Snapshot>('delete_workspace', { id }),

  setActiveWorkspace: (id: string) => invoke<Snapshot>('set_active_workspace', { id }),

  setLayout: (layout: LayoutNode) => invoke<Snapshot>('set_layout', { layout }),

  setLayoutPreset: (preset: LayoutPreset) => invoke<Snapshot>('set_layout_preset', { preset }),

  layoutPresets: () =>
    invoke<Array<{ preset: LayoutPreset; layout: LayoutNode }>>('layout_presets'),

  togglePanel: (panel: PanelId) => invoke<Snapshot>('toggle_panel', { panel }),

  setAppearance: (appearance: Appearance) => invoke<Snapshot>('set_appearance', { appearance }),

  /** `null` clears the binding back to its default. */
  setBinding: (action: string, binding: string | null) =>
    invoke<Snapshot>('set_binding', { action, binding }),

  resetBindings: () => invoke<Snapshot>('reset_bindings'),

  addProject: (workspaceId: string, path: string) =>
    invoke<Snapshot>('add_project', { workspaceId, path }),

  renameProject: (workspaceId: string, projectId: string, name: string) =>
    invoke<Snapshot>('rename_project', { workspaceId, projectId, name }),

  /** Forgets the project. Never deletes anything from disk. */
  removeProject: (workspaceId: string, projectId: string) =>
    invoke<Snapshot>('remove_project', { workspaceId, projectId }),

  moveProject: (workspaceId: string, projectId: string, targetWorkspaceId: string) =>
    invoke<Snapshot>('move_project', { workspaceId, projectId, targetWorkspaceId }),

  /** Tab order is project order, so this moves the numbered shortcuts too. */
  reorderProject: (workspaceId: string, projectId: string, to: number) =>
    invoke<Snapshot>('reorder_project', { workspaceId, projectId, to }),

  /** `null` goes back to the account's own shell. */
  setShell: (shell: ShellSpec | null) => invoke<Snapshot>('set_shell', { shell }),

  setNotifications: (enabled: boolean) => invoke<Snapshot>('set_notifications', { enabled }),

  setActiveProject: (workspaceId: string, projectId: string) =>
    invoke<Snapshot>('set_active_project', { workspaceId, projectId }),

  revealProject: (workspaceId: string, projectId: string) =>
    invoke<void>('reveal_project', { workspaceId, projectId }),

  hostPlatform: () => invoke<HostPlatform>('host_platform'),

  // ---- sessions ----

  openSession: (
    workspaceId: string,
    projectId: string,
    kind: SessionKind,
    slot: number,
    cols: number,
    rows: number,
  ) => invoke<SessionInfo>('open_session', { workspaceId, projectId, kind, slot, cols, rows }),

  /** Keystrokes. Never logged, on either side of the boundary. */
  writeSession: (id: string, data: string) => invoke<void>('write_session', { id, data }),

  resizeSession: (id: string, cols: number, rows: number) =>
    invoke<void>('resize_session', { id, cols, rows }),

  sessionScrollback: (id: string) => invoke<ScrollbackSnapshot>('session_scrollback', { id }),

  closeSession: (id: string) => invoke<void>('close_session', { id }),

  restartSession: (
    workspaceId: string,
    projectId: string,
    kind: SessionKind,
    slot: number,
    cols: number,
    rows: number,
  ) => invoke<SessionInfo>('restart_session', { workspaceId, projectId, kind, slot, cols, rows }),

  stopProject: (projectId: string) => invoke<void>('stop_project', { projectId }),

  /** Ends one of a project's sessions, by slot. */
  stopSessionSlot: (projectId: string, slot: number) =>
    invoke<void>('stop_session_slot', { projectId, slot }),

  /** Sessions running right now, including any started before this window. */
  listSessions: () => invoke<SessionInfo[]>('list_sessions'),

  /** Stops the daemon, and with it every session in every project. */
  stopDaemon: () => invoke<void>('stop_daemon'),

  // ---- Claude Code integration ----

  claudeHookStatus: () => invoke<HookStatus>('claude_hook_status'),

  claudeIntegration: () => invoke<Integration>('claude_integration'),

  /** Checked on demand: someone who installs the missing thing looks again. */
  checkRequirements: () => invoke<Requirement[]>('check_requirements'),

  daemonAvailable: () => invoke<boolean>('daemon_available'),

  installClaudeStatusLine: () => invoke<Integration>('install_claude_status_line'),

  removeClaudeStatusLine: () => invoke<Integration>('remove_claude_status_line'),

  /** What each project last reported. Fetched on attach; events follow. */
  sessionUsage: () => invoke<UsageReport[]>('session_usage'),

  /** The exact command that would be registered, to show before agreeing. */
  claudeHookCommand: () => invoke<string>('claude_hook_command'),

  installClaudeHooks: () => invoke<HookStatus>('install_claude_hooks'),

  removeClaudeHooks: () => invoke<HookStatus>('remove_claude_hooks'),

  // ---- files ----
  // Every path is relative to the project; the backend refuses anything that
  // would resolve outside it.

  listDir: (workspaceId: string, projectId: string, path: string) =>
    invoke<DirEntry[]>('list_dir', { workspaceId, projectId, path }),

  readFile: (workspaceId: string, projectId: string, path: string) =>
    invoke<FileRead>('read_file', { workspaceId, projectId, path }),

  /** Whether a file has changed, without reading it. */
  fileRevision: (workspaceId: string, projectId: string, path: string) =>
    invoke<number | null>('file_revision', { workspaceId, projectId, path }),

  /** Refuses if the file changed since it was read, unless `expected` is null. */
  writeFile: (
    workspaceId: string,
    projectId: string,
    path: string,
    text: string,
    expectedRevision: number | null,
  ) => invoke<WriteOutcome>('write_file', { workspaceId, projectId, path, text, expectedRevision }),

  createFile: (workspaceId: string, projectId: string, path: string) =>
    invoke<void>('create_file', { workspaceId, projectId, path }),

  createDir: (workspaceId: string, projectId: string, path: string) =>
    invoke<void>('create_dir', { workspaceId, projectId, path }),

  renamePath: (workspaceId: string, projectId: string, from: string, to: string) =>
    invoke<void>('rename_path', { workspaceId, projectId, from, to }),

  duplicatePath: (workspaceId: string, projectId: string, path: string) =>
    invoke<string>('duplicate_path', { workspaceId, projectId, path }),

  copyInto: (workspaceId: string, projectId: string, source: string, targetDir: string) =>
    invoke<string>('copy_into', { workspaceId, projectId, source, targetDir }),

  /** Moves to the system trash. Beacon never deletes outright. */
  trashPath: (workspaceId: string, projectId: string, path: string) =>
    invoke<void>('trash_path', { workspaceId, projectId, path }),

  revealPath: (workspaceId: string, projectId: string, path: string) =>
    invoke<void>('reveal_path', { workspaceId, projectId, path }),

  /** Listed on demand: a stale file list is worse than a fresh read. */
  listProjectFiles: (workspaceId: string, projectId: string) =>
    invoke<string[]>('list_project_files', { workspaceId, projectId }),

  /** Read fresh every time; values are never cached on this side either. */
  readEnvFile: (workspaceId: string, projectId: string, path: string) =>
    invoke<EnvEntry[]>('read_env_file', { workspaceId, projectId, path }),

  // ---- git ----

  /** `null` when the project is not a repository, which is not an error. */
  gitStatus: (workspaceId: string, projectId: string) =>
    invoke<GitStatus | null>('git_status', { workspaceId, projectId }),

  gitDiff: (
    workspaceId: string,
    projectId: string,
    path: string,
    staged: boolean,
    untracked: boolean,
  ) => invoke<string>('git_diff', { workspaceId, projectId, path, staged, untracked }),

  gitStage: (workspaceId: string, projectId: string, path: string) =>
    invoke<GitStatus>('git_stage', { workspaceId, projectId, path }),

  gitUnstage: (workspaceId: string, projectId: string, path: string) =>
    invoke<GitStatus>('git_unstage', { workspaceId, projectId, path }),

  gitStageAll: (workspaceId: string, projectId: string) =>
    invoke<GitStatus>('git_stage_all', { workspaceId, projectId }),

  gitCommit: (workspaceId: string, projectId: string, message: string) =>
    invoke<GitStatus>('git_commit', { workspaceId, projectId, message }),

  gitPush: (workspaceId: string, projectId: string) =>
    invoke<string>('git_push', { workspaceId, projectId }),

  gitPull: (workspaceId: string, projectId: string) =>
    invoke<string>('git_pull', { workspaceId, projectId }),
}

/** Native folder picker. Resolves to `null` when the user cancels. */
export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title,
    ...(defaultPath ? { defaultPath } : {}),
  })
  return typeof selected === 'string' ? selected : null
}

/** Command errors arrive as plain strings from the backend. */
export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return 'Something went wrong'
}
