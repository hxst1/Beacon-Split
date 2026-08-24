/**
 * The only module that talks to Tauri.
 *
 * Components import from here, never from `@tauri-apps/api` directly, so the
 * transport can change — most likely to a daemon connection — without touching
 * the UI.
 */

import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'

import type { HostPlatform, PanelLayout, Snapshot } from '@/types/beacon'

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

  setPanels: (panels: PanelLayout) => invoke<Snapshot>('set_panels', { panels }),

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
