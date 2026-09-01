import { create } from 'zustand'

import { errorMessage, ipc } from '@/ipc'
import type { FileContents } from '@/types/beacon'

export interface OpenFile {
  /** Relative to the project root. */
  path: string
  name: string
  /** The text as last read or saved: what is believed to be on disk. */
  saved: string
  /**
   * The text on screen.
   *
   * This lives here rather than only in CodeMirror because the editor is
   * unmounted constantly — switching tabs, hiding the panel, going fullscreen,
   * switching project — and an unmounted CodeMirror takes its document with it.
   * Held only there, unsaved work was lost by ordinary navigation.
   */
  draft: string
  /** What the file was when it was read: text, binary, or too large to edit. */
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
  /** Set when the file is no longer on disk but the buffer is still open. */
  goneFromDisk?: true | undefined
  /**
   * Bumped only when the text underneath the editor is replaced wholesale — a
   * reload. Saving must not touch it: the editor is seeded once, at mount, so
   * anything that changes its identity throws away the undo history, the cursor
   * and the scroll position.
   */
  epoch: number
}

interface EditorState {
  /** Open files per project, so switching tabs restores what you had open. */
  byProject: Record<string, OpenFile[]>
  active: Record<string, string | undefined>
  error: string | null

  open: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Closes a file, and everything under it when given a directory's path. */
  close: (projectId: string, path: string) => void
  activate: (projectId: string, path: string) => void
  /** Records what is on screen. */
  edit: (projectId: string, path: string, text: string) => void
  /** Writes the draft back, refusing if the file moved since it was read. */
  save: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Saves the draft over whatever is on disk, having been asked to. */
  overwrite: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Throws the draft away and takes what is on disk. */
  reload: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Re-reads revisions and flags anything that moved underneath us. */
  checkForChanges: (workspaceId: string, projectId: string) => Promise<void>
  /** Forgets a project's open files, e.g. when it is removed. */
  forget: (projectId: string) => void
  /** Follows a rename, including of a directory the open files live under. */
  rename: (projectId: string, from: string, to: string) => void
  /** Clears the last error, so a message that has been read can be dismissed. */
  dismissError: () => void
}

/**
 * Which files are open, per project, and what is in each of them.
 *
 * CodeMirror still owns the editing experience — selection, undo history,
 * scroll position — but not the text itself. That has to outlive the view.
 */
export const useEditor = create<EditorState>((set, get) => ({
  byProject: {},
  active: {},
  error: null,

  open: async (workspaceId, projectId, path) => {
    const already = get().byProject[projectId]?.find((file) => file.path === path)
    if (already) {
      get().activate(projectId, path)
      return
    }

    try {
      const read = await ipc.readFile(workspaceId, projectId, path)
      const text = read.kind === 'text' ? read.text : ''
      const file: OpenFile = {
        path,
        name: nameOf(path),
        saved: text,
        draft: text,
        contents: read,
        revision: read.revision,
        epoch: 0,
      }
      set((state) => ({
        byProject: {
          ...state.byProject,
          [projectId]: [...(state.byProject[projectId] ?? []), file],
        },
        active: { ...state.active, [projectId]: path },
        error: null,
      }))
    } catch (error) {
      set({ error: errorMessage(error) })
    }
  },

  close: (projectId, path) =>
    set((state) => {
      const open = state.byProject[projectId] ?? []
      const remaining = open.filter((file) => !isAtOrUnder(file.path, path))

      // Closing the file you are looking at should land on its neighbour, not
      // jump you across the strip to whatever happens to be last.
      const activePath = state.active[projectId]
      let nextActive = activePath
      if (activePath !== undefined && isAtOrUnder(activePath, path)) {
        const wasAt = open.findIndex((file) => file.path === activePath)
        nextActive =
          remaining.find((file) => open.indexOf(file) > wasAt)?.path ?? remaining.at(-1)?.path
      }

      return {
        byProject: { ...state.byProject, [projectId]: remaining },
        active: { ...state.active, [projectId]: nextActive },
      }
    }),

  activate: (projectId, path) =>
    set((state) => ({ active: { ...state.active, [projectId]: path } })),

  edit: (projectId, path, text) =>
    set((state) => ({
      byProject: mapFile(state.byProject, projectId, path, (file) => ({ ...file, draft: text })),
    })),

  save: (workspaceId, projectId, path) => write(set, get, workspaceId, projectId, path, true),

  overwrite: (workspaceId, projectId, path) => write(set, get, workspaceId, projectId, path, false),

  reload: async (workspaceId, projectId, path) => {
    try {
      const read = await ipc.readFile(workspaceId, projectId, path)
      const text = read.kind === 'text' ? read.text : ''
      set((state) => ({
        error: null,
        byProject: mapFile(state.byProject, projectId, path, (file) => ({
          ...file,
          contents: read,
          saved: text,
          draft: text,
          revision: read.revision,
          changedOnDisk: undefined,
          goneFromDisk: undefined,
          epoch: file.epoch + 1,
        })),
      }))
    } catch (error) {
      set({ error: errorMessage(error) })
    }
  },

  checkForChanges: async (workspaceId, projectId) => {
    const open = (get().byProject[projectId] ?? []).filter((file) => file.revision !== undefined)

    // Asked for together rather than one after another: this runs on a timer
    // while the window is in front, and a project with a dozen tabs open should
    // not spend a dozen round-trips on it.
    const revisions = await Promise.all(
      open.map((file) =>
        // A file we cannot stat is not a file we should claim changed.
        ipc.fileRevision(workspaceId, projectId, file.path).catch(() => file.revision ?? null),
      ),
    )

    for (const [index, file] of open.entries()) {
      const now = revisions[index] ?? null
      if (now === file.revision) continue

      // A file that is gone is not a file to reload: that would replace a
      // perfectly good buffer with an error. Say so and leave the text alone,
      // so saving can still put it back.
      if (now === null) {
        set((state) => ({
          byProject: mapFile(state.byProject, projectId, file.path, (open) => ({
            ...open,
            goneFromDisk: true as const,
          })),
        }))
        continue
      }

      // A clean buffer can simply take the new contents: there is nothing to
      // lose, and showing stale text is its own kind of wrong.
      if (isDirty(projectId, file.path)) {
        set((state) => ({
          byProject: mapFile(state.byProject, projectId, file.path, (open) => ({
            ...open,
            changedOnDisk: true as const,
          })),
        }))
      } else {
        await get().reload(workspaceId, projectId, file.path)
      }
    }
  },

  forget: (projectId) =>
    set((state) => {
      const byProject = { ...state.byProject }
      const active = { ...state.active }
      delete byProject[projectId]
      delete active[projectId]
      return { byProject, active }
    }),

  rename: (projectId, from, to) =>
    set((state) => {
      const moved = (path: string): string =>
        path === from ? to : `${to}/${path.slice(from.length + 1)}`
      const activePath = state.active[projectId]

      return {
        byProject: {
          ...state.byProject,
          [projectId]: (state.byProject[projectId] ?? []).map((file) =>
            isAtOrUnder(file.path, from)
              ? { ...file, path: moved(file.path), name: nameOf(moved(file.path)) }
              : file,
          ),
        },
        active: {
          ...state.active,
          [projectId]:
            activePath !== undefined && isAtOrUnder(activePath, from) ? moved(activePath) : activePath,
        },
      }
    }),

  dismissError: () => set({ error: null }),
}))

/** Whether a path is the given one, or lives inside it. */
function isAtOrUnder(path: string, ancestor: string): boolean {
  return path === ancestor || path.startsWith(`${ancestor}/`)
}

const nameOf = (path: string): string => path.split('/').pop() ?? path

/** Replaces one open file, leaving the rest of the state alone. */
function mapFile(
  byProject: Record<string, OpenFile[]>,
  projectId: string,
  path: string,
  change: (file: OpenFile) => OpenFile,
): Record<string, OpenFile[]> {
  return {
    ...byProject,
    [projectId]: (byProject[projectId] ?? []).map((file) =>
      file.path === path ? change(file) : file,
    ),
  }
}

/**
 * Whether what is on screen differs from what is on disk.
 *
 * Derived rather than tracked: a flag kept alongside the text can say a file
 * was saved while the text says otherwise, which is exactly how unsaved work
 * goes missing.
 */
export function isDirty(projectId: string, path: string): boolean {
  const file = useEditor.getState().byProject[projectId]?.find((open) => open.path === path)
  return file !== undefined && file.draft !== file.saved
}

/** Every open file in a project whose draft has not been written. */
export function unsavedIn(projectId: string): OpenFile[] {
  return (useEditor.getState().byProject[projectId] ?? []).filter(
    (file) => file.draft !== file.saved,
  )
}

/** In-flight writes, per file, so saves for one file cannot overtake each other. */
const inFlight = new Map<string, Promise<void>>()

/**
 * Writes a draft back.
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
  guard: boolean,
): Promise<void> {
  const id = `${projectId}:${path}`

  // Saves for one file run one at a time. Two in flight together would both be
  // checked against the revision from before either of them, so the second
  // would be refused as a conflict with the first — and the text the user was
  // last looking at would be the text that never reached the disk.
  const queued = (inFlight.get(id) ?? Promise.resolve()).then(() =>
    writeOnce(set, get, workspaceId, projectId, path, guard),
  )
  inFlight.set(id, queued)
  try {
    await queued
  } finally {
    if (inFlight.get(id) === queued) inFlight.delete(id)
  }
}

async function writeOnce(
  set: (updater: Partial<EditorState> | ((state: EditorState) => Partial<EditorState>)) => void,
  get: () => EditorState,
  workspaceId: string,
  projectId: string,
  path: string,
  guard: boolean,
): Promise<void> {
  const file = get().byProject[projectId]?.find((candidate) => candidate.path === path)
  if (!file || file.contents.kind !== 'text') return

  // Whatever is on screen at the moment the write goes out. Anything typed
  // after this point has not reached the disk, and the tab keeps saying so
  // because dirtiness is the draft against `saved`, not a flag this clears.
  const text = file.draft

  // A guarded save with nothing to guard against is not a save Beacon can make
  // safely, and quietly turning it into an overwrite is the one outcome the
  // guard exists to prevent. Ask instead.
  if (guard && file.revision === undefined) {
    set((state) => ({
      byProject: mapFile(state.byProject, projectId, path, (open) => ({
        ...open,
        changedOnDisk: true as const,
      })),
      error: `Beacon cannot tell whether ${file.name} changed since it was opened. Reload it, or save over what is there.`,
    }))
    return
  }

  try {
    const outcome = await ipc.writeFile(
      workspaceId,
      projectId,
      path,
      text,
      guard ? (file.revision ?? null) : null,
    )
    if (outcome.outcome === 'stale') {
      set((state) => ({
        byProject: mapFile(state.byProject, projectId, path, (open) => ({
          ...open,
          changedOnDisk: true as const,
        })),
        error: `${file.name} changed on disk. Reload it, or save over what is there.`,
      }))
      return
    }

    // The write reported the revision the file now has, so the next save is
    // checked against what this one left behind rather than against a stamp we
    // made obsolete ourselves.
    set((state) => ({
      error: null,
      byProject: mapFile(state.byProject, projectId, path, (open) => ({
        ...open,
        saved: text,
        // `contents` seeds the editor whenever it is rebuilt. Left at the text
        // the file was opened with, a rebuild would put back what the user just
        // replaced, and a save that worked would look like one that never
        // happened.
        contents: { kind: 'text', text },
        revision: outcome.revision,
        changedOnDisk: undefined,
        goneFromDisk: undefined,
      })),
    }))
  } catch (error) {
    set({ error: errorMessage(error) })
  }
}
