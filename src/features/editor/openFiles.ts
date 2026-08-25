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
  /**
   * What the file looked like on disk when it was read.
   *
   * Beacon exists to work beside Claude, and Claude edits files that are open.
   * Without this, saving would overwrite its work with a stale buffer and say
   * nothing about it.
   */
  revision: number | undefined
  /** Set when the file changed on disk and the buffer has not caught up. */
  changedOnDisk?: true | undefined
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
  /** Saves over whatever is on disk, having been asked to. */
  overwrite: (workspaceId: string, projectId: string, path: string, text: string) => Promise<void>
  /** Throws the buffer away and takes what is on disk. */
  reload: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Re-reads revisions and flags anything that moved underneath us. */
  checkForChanges: (workspaceId: string, projectId: string) => Promise<void>
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
      const read = await ipc.readFile(workspaceId, projectId, path)
      const file: OpenFile = {
        path,
        name: path.split('/').pop() ?? path,
        saved: read.kind === 'text' ? read.text : '',
        contents: read,
        revision: read.revision,
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

  save: (workspaceId, projectId, path, text) => write(set, get, workspaceId, projectId, path, text, true),

  overwrite: (workspaceId, projectId, path, text) =>
    write(set, get, workspaceId, projectId, path, text, false),

  reload: async (workspaceId, projectId, path) => {
    try {
      const read = await ipc.readFile(workspaceId, projectId, path)
      set((state) => {
        const dirty = { ...state.dirty }
        delete dirty[key(projectId, path)]
        return {
          dirty,
          error: null,
          byProject: {
            ...state.byProject,
            [projectId]: (state.byProject[projectId] ?? []).map((file) =>
              file.path === path
                ? {
                    ...file,
                    contents: read,
                    saved: read.kind === 'text' ? read.text : '',
                    revision: read.revision,
                    changedOnDisk: undefined,
                  }
                : file,
            ),
          },
        }
      })
    } catch (error) {
      set({ error: errorMessage(error) })
    }
  },

  checkForChanges: async (workspaceId, projectId) => {
    const open = get().byProject[projectId] ?? []
    const stale: string[] = []

    for (const file of open) {
      if (file.revision === undefined) continue
      try {
        const now = await ipc.fileRevision(workspaceId, projectId, file.path)
        if (now !== file.revision) stale.push(file.path)
      } catch {
        // A file we cannot stat is not a file we should claim changed.
      }
    }
    if (stale.length === 0) return

    // A clean buffer can simply take the new contents: there is nothing to
    // lose, and showing stale text is its own kind of wrong.
    for (const path of stale) {
      const file = open.find((candidate) => candidate.path === path)
      const isDirty = get().dirty[key(projectId, path)] === true
      if (!isDirty) {
        await get().reload(workspaceId, projectId, path)
        continue
      }
      if (file) markChanged(set, projectId, path)
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

function markChanged(
  set: (updater: (state: EditorState) => Partial<EditorState>) => void,
  projectId: string,
  path: string,
): void {
  set((state) => ({
    byProject: {
      ...state.byProject,
      [projectId]: (state.byProject[projectId] ?? []).map((file) =>
        file.path === path ? { ...file, changedOnDisk: true as const } : file,
      ),
    },
  }))
}

/**
 * Writes a buffer back.
 *
 * `guard` is what separates saving from overwriting: with it, a file that moved
 * since it was read is refused and the tab says so. Without it, the user has
 * seen that message and asked for it anyway.
 */
async function write(
  set: (updater: Partial<EditorState> | ((state: EditorState) => Partial<EditorState>)) => void,
  get: () => EditorState,
  workspaceId: string,
  projectId: string,
  path: string,
  text: string,
  guard: boolean,
): Promise<void> {
  const file = get().byProject[projectId]?.find((candidate) => candidate.path === path)
  const expected = guard ? (file?.revision ?? null) : null

  try {
    const outcome = await ipc.writeFile(workspaceId, projectId, path, text, expected)
    if (outcome === 'stale') {
      markChanged(set as never, projectId, path)
      set({
        error: `${file?.name ?? path} changed on disk. Reload it, or save over what is there.`,
      })
      return
    }

    // Written: the file on disk is now this buffer, so the revision moves with
    // it — otherwise the next save would be refused against a stamp we made
    // obsolete ourselves.
    const revision = await ipc.fileRevision(workspaceId, projectId, path).catch(() => null)

    set((state: EditorState) => {
      const dirty = { ...state.dirty }
      delete dirty[key(projectId, path)]
      return {
        dirty,
        error: null,
        byProject: {
          ...state.byProject,
          [projectId]: (state.byProject[projectId] ?? []).map((candidate) =>
            candidate.path === path
              ? {
                  ...candidate,
                  saved: text,
                  revision: revision ?? undefined,
                  changedOnDisk: undefined,
                }
              : candidate,
          ),
        },
      }
    })
  } catch (error) {
    set({ error: errorMessage(error) })
  }
}
