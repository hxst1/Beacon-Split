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

  updateWorkspace: (id: string, changes: { name?: string; accent?: string }) =>
    invoke<Snapshot>('update_workspace', {
      id,
      name: changes.name ?? null,
      accent: changes.accent ?? null,
    }),

  deleteWorkspace: (id: string) => invoke<Snapshot>('delete_workspace', { id }),

  setActiveWorkspace: (id: string) => invoke<Snapshot>('set_active_workspace', { id }),

  setLayout: (layout: LayoutNode) => invoke<Snapshot>('set_layout', { layout }),

  setLayoutPreset: (preset: LayoutPreset) => invoke<Snapshot>('set_layout_preset', { preset }),

  layoutPresets: () =>
    invoke<Array<{ preset: LayoutPreset; layout: LayoutNode }>>('layout_presets'),

  togglePanel: (panel: PanelId) => invoke<Snapshot>('toggle_panel', { panel }),

  addProject: (workspaceId: string, path: string) =>
    invoke<Snapshot>('add_project', { workspaceId, path }),

  renameProject: (workspaceId: string, projectId: string, name: string) =>
    invoke<Snapshot>('rename_project', { workspaceId, projectId, name }),

  /** Forgets the project. Never deletes anything from disk. */
  removeProject: (workspaceId: string, projectId: string) =>
    invoke<Snapshot>('remove_project', { workspaceId, projectId }),

  moveProject: (workspaceId: string, projectId: string, targetWorkspaceId: string) =>
    invoke<Snapshot>('move_project', { workspaceId, projectId, targetWorkspaceId }),

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
    cols: number,
    rows: number,
  ) => invoke<SessionInfo>('open_session', { workspaceId, projectId, kind, cols, rows }),

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
    cols: number,
    rows: number,
  ) => invoke<SessionInfo>('restart_session', { workspaceId, projectId, kind, cols, rows }),

  stopProject: (projectId: string) => invoke<void>('stop_project', { projectId }),
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
