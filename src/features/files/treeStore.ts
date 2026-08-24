import { create } from 'zustand'

import { errorMessage, ipc } from '@/ipc'
import type { DirEntry } from '@/types/beacon'

interface TreeState {
  /** Directory listings, keyed `projectId:path`. Loaded on expand. */
  entries: Record<string, DirEntry[]>
  expanded: Record<string, true>
  loading: Record<string, true>
  selected: Record<string, string | undefined>
  showHidden: boolean
  error: string | null
  /** Path recorded by Copy, pasted into a directory later. */
  clipboard: { projectId: string; path: string } | null

  load: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Re-reads a directory that is already open, after it changes on disk. */
  refresh: (workspaceId: string, projectId: string, path: string) => Promise<void>
  toggle: (workspaceId: string, projectId: string, path: string) => Promise<void>
  select: (projectId: string, path: string) => void
  setShowHidden: (show: boolean) => void
  setClipboard: (projectId: string, path: string) => void
  setError: (message: string | null) => void
  forget: (projectId: string) => void
}

const key = (projectId: string, path: string): string => `${projectId}:${path}`

/** The parent directory of a project-relative path; `''` is the root. */
export function parentOf(path: string): string {
  const cut = path.lastIndexOf('/')
  return cut === -1 ? '' : path.slice(0, cut)
}

export function joinPath(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name
}

/**
 * The file tree's state.
 *
 * Listings are per directory and loaded on expand, so opening a project with a
 * large `node_modules` costs nothing until someone actually looks inside it.
 */
export const useTree = create<TreeState>((set, get) => ({
  entries: {},
  expanded: {},
  loading: {},
  selected: {},
  showHidden: false,
  error: null,
  clipboard: null,

  load: async (workspaceId, projectId, path) => {
    const id = key(projectId, path)
    if (get().entries[id] || get().loading[id]) return
    set((state) => ({ loading: { ...state.loading, [id]: true } }))

    try {
      const entries = await ipc.listDir(workspaceId, projectId, path)
      set((state) => {
        const loading = { ...state.loading }
        delete loading[id]
        return { entries: { ...state.entries, [id]: entries }, loading, error: null }
      })
    } catch (error) {
      set((state) => {
        const loading = { ...state.loading }
        delete loading[id]
        return { loading, error: errorMessage(error) }
      })
    }
  },

  refresh: async (workspaceId, projectId, path) => {
    const id = key(projectId, path)
    if (!get().entries[id]) return
    try {
      const entries = await ipc.listDir(workspaceId, projectId, path)
      set((state) => ({ entries: { ...state.entries, [id]: entries }, error: null }))
    } catch (error) {
      set({ error: errorMessage(error) })
    }
  },

  toggle: async (workspaceId, projectId, path) => {
    const id = key(projectId, path)
    const isOpen = get().expanded[id] === true

    set((state) => {
      const expanded = { ...state.expanded }
      if (isOpen) delete expanded[id]
      else expanded[id] = true
      return { expanded }
    })

    if (!isOpen) await get().load(workspaceId, projectId, path)
  },

  select: (projectId, path) =>
    set((state) => ({ selected: { ...state.selected, [projectId]: path } })),

  setShowHidden: (showHidden) => set({ showHidden }),

  setClipboard: (projectId, path) => set({ clipboard: { projectId, path } }),

  setError: (error) => set({ error }),

  forget: (projectId) =>
    set((state) => {
      const drop = <T,>(record: Record<string, T>): Record<string, T> =>
        Object.fromEntries(Object.entries(record).filter(([id]) => !id.startsWith(`${projectId}:`)))
      const selected = { ...state.selected }
      delete selected[projectId]
      return {
        entries: drop(state.entries),
        expanded: drop(state.expanded),
        loading: drop(state.loading),
        selected,
      }
    }),
}))

export const treeKey = key
