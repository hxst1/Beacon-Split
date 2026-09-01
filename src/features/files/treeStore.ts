import { create } from 'zustand'

import { errorMessage, ipc } from '@/ipc'
import type { DirEntry } from '@/types/beacon'

interface TreeState extends Omit<TreeView, 'showHidden'> {
  selected: Record<string, string | undefined>
  error: string | null
  /** Path recorded by Copy, pasted into a directory later. */
  clipboard: { projectId: string; path: string } | null

  /** Reads a directory, whether or not it has been read before. */
  load: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Re-reads a directory that is already on screen, after it changes on disk. */
  refresh: (workspaceId: string, projectId: string, path: string) => Promise<void>
  /** Re-reads the project root and every directory currently visible in it. */
  refreshAll: (workspaceId: string, projectId: string) => Promise<void>
  /** Opens whatever a path is inside, reads it, and selects the path. */
  reveal: (workspaceId: string, projectId: string, path: string) => Promise<void>
  toggle: (workspaceId: string, projectId: string, path: string) => Promise<void>
  setExpanded: (
    workspaceId: string,
    projectId: string,
    path: string,
    open: boolean,
  ) => Promise<void>
  select: (projectId: string, path: string) => void
  setClipboard: (projectId: string, path: string) => void
  setError: (message: string | null) => void
  forget: (projectId: string) => void
}

/** As much of the store as it takes to work out what the tree is showing. */
export interface TreeView {
  /** Directory listings, keyed `projectId:path`. Loaded on expand. */
  entries: Record<string, DirEntry[]>
  expanded: Record<string, true>
  loading: Record<string, true>
  /**
   * Whether dotfiles are listed. Not part of the store: it is one of Beacon's
   * settings, kept in the same place as every other one so that it survives a
   * restart the way the user expects.
   */
  showHidden: boolean
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

/** Every directory a path sits inside, outermost first, starting at the root. */
function ancestorsOf(path: string): string[] {
  const parts = path.split('/').slice(0, -1)
  const dirs = ['']
  for (const part of parts) dirs.push(joinPath(dirs[dirs.length - 1] ?? '', part))
  return dirs
}

/**
 * What is wrong with a name, or `null` if nothing is.
 *
 * A name is a name, not a path: `src/thing.ts` typed into New file would
 * create a directory the user did not ask for, and an empty field pressed
 * Create used to do nothing at all.
 */
export function destinationError(value: string): string | null {
  // Empty means the project root, which is a real answer here rather than a
  // missing one — moving something out to the top is what it looks like.
  const trimmed = value.trim().replace(/^\/+|\/+$/g, '')
  if (!trimmed) return null
  if (trimmed.split('/').some((part) => part === '' || part === '.' || part === '..')) {
    return 'Give a folder inside the project'
  }
  return null
}

export function nameError(value: string): string | null {
  const trimmed = value.trim()
  if (!trimmed) return 'Enter a name'
  if (trimmed.includes('/')) return 'A name cannot contain “/”'
  if (trimmed === '.' || trimmed === '..') return 'That name is reserved'
  return null
}

/** A row of the tree as it appears on screen, top to bottom. */
export type TreeRow =
  | { type: 'entry'; id: string; entry: DirEntry; depth: number; expanded: boolean }
  /** Why an expanded directory is showing nothing. */
  | { type: 'note'; id: string; depth: number; note: 'reading' | 'empty' | 'hidden' }

/**
 * The tree flattened to the rows it draws.
 *
 * Flat rather than nested because the keyboard moves through what is on
 * screen: the row below a collapsed folder is its sibling, and the row below
 * an open one is its first child. A note takes the place of an expanded
 * directory's contents, so an empty folder never looks like a broken one.
 */
export function visibleRows(view: TreeView, projectId: string): TreeRow[] {
  const rows: TreeRow[] = []

  const walk = (path: string, depth: number): void => {
    const id = key(projectId, path)
    const entries = view.entries[id]
    if (!entries) {
      if (view.loading[id]) rows.push({ type: 'note', id: `${id}/·`, depth, note: 'reading' })
      return
    }

    const visible = view.showHidden ? entries : entries.filter((entry) => !entry.hidden)
    if (visible.length === 0) {
      rows.push({
        type: 'note',
        id: `${id}/·`,
        depth,
        note: entries.length > 0 ? 'hidden' : 'empty',
      })
      return
    }

    for (const entry of visible) {
      const expanded = view.expanded[key(projectId, entry.path)] === true
      rows.push({ type: 'entry', id: entry.path, entry, depth, expanded })
      if (entry.kind === 'directory' && expanded) walk(entry.path, depth + 1)
    }
  }

  walk('', 0)
  return rows
}

/** The directories on screen, root first — the only ones worth re-reading. */
export function visibleDirectories(view: TreeView, projectId: string): string[] {
  const open = visibleRows(view, projectId).flatMap((row) =>
    row.type === 'entry' && row.expanded && row.entry.kind === 'directory' ? [row.entry.path] : [],
  )
  return ['', ...open]
}

/** Forgets a directory and everything under it. */
function dropSubtree<T>(
  record: Record<string, T>,
  projectId: string,
  path: string,
): Record<string, T> {
  const id = key(projectId, path)
  const prefix = path === '' ? `${projectId}:` : `${id}/`
  return Object.fromEntries(
    Object.entries(record).filter(([entryId]) => entryId !== id && !entryId.startsWith(prefix)),
  )
}

function without<T>(record: Record<string, T>, id: string): Record<string, T> {
  const next = { ...record }
  delete next[id]
  return next
}


/**
 * The file tree's state.
 *
 * Listings are per directory and loaded on expand, so opening a project with a
 * large `node_modules` costs nothing until someone actually looks inside it.
 */
export const useTree = create<TreeState>((set, get) => {
  /**
   * Reads one directory, replacing whatever was there.
   *
   * Never served from what is already in hand: Beacon has no filesystem
   * watcher by design (see `docs/DECISIONS.md`, ADR-025), so a listing is only
   * right if it was read just now — reopening a folder is someone asking what
   * is in it, not what was. A read already in flight is left to finish rather
   * than doubled.
   */
  const read = async (workspaceId: string, projectId: string, path: string): Promise<void> => {
    const id = key(projectId, path)
    if (get().loading[id]) return
    set((state) => ({ loading: { ...state.loading, [id]: true } }))

    try {
      const entries = await ipc.listDir(workspaceId, projectId, path)
      set((state) => ({
        entries: { ...state.entries, [id]: entries },
        loading: without(state.loading, id),
        error: null,
      }))
    } catch (error) {
      // The old listing goes with it. A directory that no longer reads was
      // renamed, trashed or unmounted, and leaving its last contents on screen
      // offers rows that cannot be opened and re-reads that fail forever.
      set((state) => ({
        entries: dropSubtree(state.entries, projectId, path),
        expanded: dropSubtree(state.expanded, projectId, path),
        loading: without(state.loading, id),
        error: errorMessage(error),
      }))
    }
  }

  return {
    entries: {},
    expanded: {},
    loading: {},
    selected: {},
    error: null,
    clipboard: null,

    load: (workspaceId, projectId, path) => read(workspaceId, projectId, path),

    refresh: async (workspaceId, projectId, path) => {
      // A directory nobody has opened has nothing on screen to correct.
      if (!get().entries[key(projectId, path)]) return
      await read(workspaceId, projectId, path)
    },

    refreshAll: async (workspaceId, projectId) => {
      // The root is read even when the last attempt failed: a project folder
      // that was briefly unmounted has to be recoverable without switching
      // projects away and back.
      await read(workspaceId, projectId, '')

      // Sequential and only what is expanded: a `node_modules` opened once and
      // collapsed must not cost thousands of entries on every focus for the
      // rest of the session. Dotfiles count as expanded even while they are
      // filtered out of the display, so turning them back on does not reveal a
      // listing from an hour ago.
      for (const path of visibleDirectories({ ...get(), showHidden: true }, projectId)) {
        if (path === '') continue
        await get().refresh(workspaceId, projectId, path)
      }
    },

    reveal: async (workspaceId, projectId, path) => {
      const parent = parentOf(path)
      const ancestors = ancestorsOf(path)

      // Everything above it is opened, or something created inside a folder
      // nobody has expanded lands where the user cannot see it.
      set((state) => {
        const expanded = { ...state.expanded }
        for (const dir of ancestors) if (dir) expanded[key(projectId, dir)] = true
        return { expanded }
      })

      for (const dir of ancestors) {
        // Only the folder it landed in has changed; the ones above it are read
        // only if they were never opened.
        const loaded = get().entries[key(projectId, dir)] !== undefined
        if (!loaded || dir === parent) await read(workspaceId, projectId, dir)
      }

      get().select(projectId, path)
    },

    setExpanded: async (workspaceId, projectId, path, open) => {
      const id = key(projectId, path)
      if ((get().expanded[id] === true) === open) return

      set((state) => {
        const expanded = { ...state.expanded }
        if (open) expanded[id] = true
        else delete expanded[id]
        return { expanded }
      })

      if (open) await read(workspaceId, projectId, path)
    },

    toggle: (workspaceId, projectId, path) =>
      get().setExpanded(
        workspaceId,
        projectId,
        path,
        get().expanded[key(projectId, path)] !== true,
      ),

    select: (projectId, path) =>
      set((state) => ({ selected: { ...state.selected, [projectId]: path } })),

    setClipboard: (projectId, path) => set({ clipboard: { projectId, path } }),

    setError: (error) => set({ error }),

    forget: (projectId) =>
      set((state) => {
        const selected = { ...state.selected }
        delete selected[projectId]
        return {
          entries: dropSubtree(state.entries, projectId, ''),
          expanded: dropSubtree(state.expanded, projectId, ''),
          loading: dropSubtree(state.loading, projectId, ''),
          selected,
        }
      }),
  }
})

export const treeKey = key
