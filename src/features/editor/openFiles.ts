import { create } from 'zustand'

import { errorMessage, ipc } from '@/ipc'
import type { FileContents } from '@/types/beacon'

export interface OpenFile {
  /** Relative to the project root. */
  path: string
  name: string
  /** The text as last read or saved; used to tell whether there are changes. */
  saved: string
  contents: FileContents
}

interface EditorState {
  /** Open files per project, so switching tabs restores what you had open. */
  byProject: Record<string, OpenFile[]>
  active: Record<string, string | undefined>
  /** Paths whose buffer differs from what is on disk. */
  dirty: Record<string, true>
  error: string | null

  open: (workspaceId: string, projectId: string, path: string) => Promise<void>
  close: (projectId: string, path: string) => void
  activate: (projectId: string, path: string) => void
  markDirty: (projectId: string, path: string, isDirty: boolean) => void
  save: (workspaceId: string, projectId: string, path: string, text: string) => Promise<void>
  /** Forgets a project's open files, e.g. when it is removed. */
  forget: (projectId: string) => void
  /** Follows a rename so the tab keeps pointing at the same file. */
  rename: (projectId: string, from: string, to: string) => void
}

const key = (projectId: string, path: string): string => `${projectId}:${path}`

/**
 * Which files are open, per project.
 *
 * Buffers themselves live in CodeMirror, not here: this tracks what is open,
 * what is showing, and what has unsaved changes. Keeping the text in a store as
 * well would mean two copies that have to agree.
 */
export const useEditor = create<EditorState>((set, get) => ({
  byProject: {},
  active: {},
  dirty: {},
  error: null,

  open: async (workspaceId, projectId, path) => {
    const already = get().byProject[projectId]?.find((file) => file.path === path)
    if (already) {
      get().activate(projectId, path)
      return
    }

    try {
      const contents = await ipc.readFile(workspaceId, projectId, path)
      const file: OpenFile = {
        path,
        name: path.split('/').pop() ?? path,
        saved: contents.kind === 'text' ? contents.text : '',
        contents,
      }
      set((state) => ({
        byProject: { ...state.byProject, [projectId]: [...(state.byProject[projectId] ?? []), file] },
        active: { ...state.active, [projectId]: path },
        error: null,
      }))
    } catch (error) {
      set({ error: errorMessage(error) })
    }
  },

  close: (projectId, path) =>
    set((state) => {
      const remaining = (state.byProject[projectId] ?? []).filter((file) => file.path !== path)
      const dirty = { ...state.dirty }
      delete dirty[key(projectId, path)]
      return {
        byProject: { ...state.byProject, [projectId]: remaining },
        active: {
          ...state.active,
          [projectId]:
            state.active[projectId] === path ? remaining.at(-1)?.path : state.active[projectId],
        },
        dirty,
      }
    }),

  activate: (projectId, path) =>
    set((state) => ({ active: { ...state.active, [projectId]: path } })),

  markDirty: (projectId, path, isDirty) =>
    set((state) => {
      const dirty = { ...state.dirty }
      if (isDirty) dirty[key(projectId, path)] = true
      else delete dirty[key(projectId, path)]
      return { dirty }
    }),

  save: async (workspaceId, projectId, path, text) => {
    try {
      await ipc.writeFile(workspaceId, projectId, path, text)
      set((state) => {
        const dirty = { ...state.dirty }
        delete dirty[key(projectId, path)]
        return {
          dirty,
          error: null,
          byProject: {
            ...state.byProject,
            [projectId]: (state.byProject[projectId] ?? []).map((file) =>
              file.path === path ? { ...file, saved: text } : file,
            ),
          },
        }
      })
    } catch (error) {
      set({ error: errorMessage(error) })
    }
  },

  forget: (projectId) =>
    set((state) => {
      const byProject = { ...state.byProject }
      const active = { ...state.active }
      delete byProject[projectId]
      delete active[projectId]
      const dirty = Object.fromEntries(
        Object.entries(state.dirty).filter(([id]) => !id.startsWith(`${projectId}:`)),
      )
      return { byProject, active, dirty }
    }),

  rename: (projectId, from, to) =>
    set((state) => ({
      byProject: {
        ...state.byProject,
        [projectId]: (state.byProject[projectId] ?? []).map((file) =>
          file.path === from ? { ...file, path: to, name: to.split('/').pop() ?? to } : file,
        ),
      },
      active: {
        ...state.active,
        [projectId]: state.active[projectId] === from ? to : state.active[projectId],
      },
    })),
}))

export function isDirty(projectId: string, path: string): boolean {
  return useEditor.getState().dirty[key(projectId, path)] === true
}
