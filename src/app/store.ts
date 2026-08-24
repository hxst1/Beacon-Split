import { create } from 'zustand'

import { useEditor } from '@/features/editor/openFiles'
import { useTree } from '@/features/files/treeStore'
import { useActivity } from '@/features/terminal/activity'
import { disposeFor, disposeProject, refreshAccent } from '@/features/terminal/terminalHost'
import { errorMessage, ipc } from '@/ipc'
import { applyAccent } from '@/lib/accent'
import { setPlatform } from '@/lib/platform'
import type {
  LayoutNode,
  LayoutPreset,
  PanelId,
  Project,
  SessionKind,
  Snapshot,
  Workspace,
} from '@/types/beacon'

type Status = 'loading' | 'ready' | 'error'

interface BeaconState {
  status: Status
  /** Set when loading failed outright; transient action errors use `notice`. */
  fatal: string | null
  /** A short, dismissable message shown in the status bar. */
  notice: string | null
  snapshot: Snapshot | null
  /** Panel temporarily expanded to fill the window. Never persisted. */
  fullscreenPanel: PanelId | null
  /** Which overlay is open, if any. Never persisted. */
  overlay: 'palette' | 'quickOpen' | 'settings' | null
  /**
   * Bumped when a session is replaced, keyed `projectId:kind`.
   *
   * Terminal views are keyed on it, so a restart rebuilds the view rather than
   * leaving it attached to a session that no longer exists.
   */
  sessionEpoch: Record<string, number>

  load: () => Promise<void>
  createWorkspace: (name: string, accent: string) => Promise<void>
  updateWorkspace: (id: string, changes: { name?: string; accent?: string }) => Promise<void>
  deleteWorkspace: (id: string) => Promise<void>
  selectWorkspace: (id: string) => Promise<void>
  addProject: (path: string) => Promise<void>
  renameProject: (projectId: string, name: string) => Promise<void>
  removeProject: (projectId: string) => Promise<void>
  /** Stops the project's processes without removing the project. */
  stopProject: (projectId: string) => Promise<void>
  /** Replaces one of the project's sessions with a fresh one. */
  restartSession: (projectId: string, kind: SessionKind) => Promise<void>
  moveProject: (projectId: string, targetWorkspaceId: string) => Promise<void>
  selectProject: (projectId: string) => Promise<void>
  selectProjectAt: (index: number) => Promise<void>
  revealProject: (projectId: string) => Promise<void>
  /** Stores a resized tree, e.g. after a splitter drag. */
  setLayout: (layout: LayoutNode) => Promise<void>
  setPreset: (preset: LayoutPreset) => Promise<void>
  togglePanel: (panel: PanelId) => Promise<void>
  /** Reveals a panel if it is hidden. Showing an already-visible panel is a no-op. */
  showPanel: (panel: PanelId) => Promise<void>
  toggleFullscreen: (panel: PanelId) => void
  setOverlay: (overlay: 'palette' | 'quickOpen' | 'settings' | null) => void
  dismissNotice: () => void
}

export const useBeacon = create<BeaconState>((set, get) => {
  /** Applies a snapshot returned by any command and keeps the accent in sync. */
  const accept = (snapshot: Snapshot): void => {
    const active = snapshot.workspaces.find((w) => w.id === snapshot.activeWorkspace)
    if (active) {
      applyAccent(active.accent)
      refreshAccent()
    }
    set({ snapshot, status: 'ready' })
  }

  /**
   * Runs a command, replacing state on success and surfacing the backend's
   * message on failure. Failures leave the previous snapshot untouched, so the
   * UI never shows a state the backend does not agree with.
   */
  const run = async (action: () => Promise<Snapshot>): Promise<void> => {
    try {
      accept(await action())
    } catch (error) {
      set({ notice: errorMessage(error) })
    }
  }

  const requireWorkspace = (): string | null => get().snapshot?.activeWorkspace ?? null

  return {
    status: 'loading',
    fatal: null,
    notice: null,
    snapshot: null,
    fullscreenPanel: null,
    overlay: null,
    sessionEpoch: {},

    load: async () => {
      try {
        setPlatform(await ipc.hostPlatform())
        accept(await ipc.getSnapshot())
      } catch (error) {
        set({ status: 'error', fatal: errorMessage(error) })
      }
    },

    createWorkspace: (name, accent) => run(() => ipc.createWorkspace(name, accent)),

    updateWorkspace: (id, changes) => run(() => ipc.updateWorkspace(id, changes)),

    deleteWorkspace: (id) => run(() => ipc.deleteWorkspace(id)),

    selectWorkspace: (id) => run(() => ipc.setActiveWorkspace(id)),

    addProject: async (path) => {
      const workspaceId = requireWorkspace()
      if (!workspaceId) return
      await run(() => ipc.addProject(workspaceId, path))
    },

    renameProject: async (projectId, name) => {
      const workspaceId = requireWorkspace()
      if (!workspaceId) return
      await run(() => ipc.renameProject(workspaceId, projectId, name))
    },

    removeProject: async (projectId) => {
      const workspaceId = requireWorkspace()
      if (!workspaceId) return
      // The backend stops the processes; this drops the views that showed them.
      await run(() => ipc.removeProject(workspaceId, projectId))
      disposeProject(projectId)
      useActivity.getState().projectStopped(projectId)
      useEditor.getState().forget(projectId)
      useTree.getState().forget(projectId)
    },

    stopProject: async (projectId) => {
      try {
        await ipc.stopProject(projectId)
        disposeProject(projectId)
        useActivity.getState().projectStopped(projectId)
      } catch (error) {
        set({ notice: errorMessage(error) })
      }
    },

    restartSession: async (projectId, kind) => {
      const workspaceId = requireWorkspace()
      if (!workspaceId) return
      try {
        // The view is rebuilt from scratch, so its size is measured again
        // anyway; these are only the grid the new process starts with.
        await ipc.restartSession(workspaceId, projectId, kind, 80, 24)
        disposeFor(projectId, kind)
        set((state) => {
          const key = `${projectId}:${kind}`
          return {
            sessionEpoch: { ...state.sessionEpoch, [key]: (state.sessionEpoch[key] ?? 0) + 1 },
          }
        })
      } catch (error) {
        set({ notice: errorMessage(error) })
      }
    },

    moveProject: async (projectId, targetWorkspaceId) => {
      const workspaceId = requireWorkspace()
      if (!workspaceId) return
      await run(() => ipc.moveProject(workspaceId, projectId, targetWorkspaceId))
    },

    selectProject: async (projectId) => {
      const workspaceId = requireWorkspace()
      if (!workspaceId) return
      await run(() => ipc.setActiveProject(workspaceId, projectId))
    },

    selectProjectAt: async (index) => {
      const project = selectProjects(get())[index]
      if (project) await get().selectProject(project.id)
    },

    revealProject: async (projectId) => {
      const workspaceId = requireWorkspace()
      if (!workspaceId) return
      try {
        await ipc.revealProject(workspaceId, projectId)
      } catch (error) {
        set({ notice: errorMessage(error) })
      }
    },

    setLayout: (layout) => run(() => ipc.setLayout(layout)),

    setPreset: (preset) => run(() => ipc.setLayoutPreset(preset)),

    togglePanel: (panel) => run(() => ipc.togglePanel(panel)),

    showPanel: async (panel) => {
      if (!get().snapshot?.hidden.includes(panel)) return
      await run(() => ipc.togglePanel(panel))
    },

    toggleFullscreen: (panel) =>
      set((state) => ({ fullscreenPanel: state.fullscreenPanel === panel ? null : panel })),

    setOverlay: (overlay) => set({ overlay }),

    dismissNotice: () => set({ notice: null }),
  }
})

// ---- selectors -------------------------------------------------------------
// Kept as plain functions so they can be used both inside the store and from
// components via `useBeacon(selectX)`.

export function selectActiveWorkspace(state: BeaconState): Workspace | null {
  const snapshot = state.snapshot
  if (!snapshot) return null
  return snapshot.workspaces.find((w) => w.id === snapshot.activeWorkspace) ?? null
}

export function selectProjects(state: BeaconState): Project[] {
  return selectActiveWorkspace(state)?.projects ?? []
}

export function selectLayout(state: BeaconState): LayoutNode | null {
  return state.snapshot?.layout ?? null
}

export function selectHidden(state: BeaconState): PanelId[] {
  return state.snapshot?.hidden ?? EMPTY_PANELS
}

/** Stable reference, so a component reading `hidden` does not re-render on
 *  every snapshot that happens to have none. */
const EMPTY_PANELS: PanelId[] = []

export function selectActiveProject(state: BeaconState): Project | null {
  const workspace = selectActiveWorkspace(state)
  if (!workspace) return null
  const activeId = state.snapshot?.activeProject[workspace.id]
  return workspace.projects.find((p) => p.id === activeId) ?? workspace.projects[0] ?? null
}
