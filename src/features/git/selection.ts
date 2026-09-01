import type { FileState, GitEntry, GitStatus } from '@/types/beacon'

export interface GitSelection {
  path: string
  staged: boolean
  untracked: boolean
  state: FileState
}

/** Keep an open diff pointed at a side that still exists in the latest status. */
export function reconcileGitSelection(
  selection: GitSelection | null,
  status: GitStatus | null,
): GitSelection | null {
  if (!selection || !status) return null

  const entry = status.entries.find((candidate) => candidate.path === selection.path)
  if (!entry) return null

  // A conflict is one thing, not a staged half and an unstaged half, so it has
  // one selection whichever side the diff was opened from.
  if (isConflicted(entry)) return preserveIdentity(selection, conflictSelection(entry))

  if (selection.staged) {
    if (isStaged(entry)) return preserveIdentity(selection, selectionFor(entry, true))
    if (isUnstaged(entry)) return selectionFor(entry, false)
  } else {
    if (isUnstaged(entry)) return preserveIdentity(selection, selectionFor(entry, false))
    if (isStaged(entry)) return selectionFor(entry, true)
  }

  return null
}

function preserveIdentity(
  current: GitSelection,
  next: GitSelection,
): GitSelection {
  return current.path === next.path &&
    current.staged === next.staged &&
    current.untracked === next.untracked &&
    current.state === next.state
    ? current
    : next
}

export function selectionKey(selection: GitSelection): string {
  if (selection.state === 'conflicted') return `conflicted:${selection.path}`
  return `${selection.staged ? 'staged' : 'unstaged'}:${selection.path}`
}

function selectionFor(entry: GitEntry, staged: boolean): GitSelection {
  const state = staged ? entry.staged : entry.unstaged
  return {
    path: entry.path,
    staged,
    untracked: !staged && state === 'untracked',
    state,
  }
}

/** The one selection a conflicted path has, on neither side of the index. */
export function conflictSelection(entry: GitEntry): GitSelection {
  return { path: entry.path, staged: false, untracked: false, state: 'conflicted' }
}

export const isConflicted = (entry: GitEntry): boolean => entry.conflicted

/*
 * A conflicted path belongs to neither list. It has changes on both sides of
 * the index, so it used to be drawn twice — once with a stage button and once
 * with an unstage button, either of which would have told git the conflict was
 * resolved.
 */
export const isStaged = (entry: GitEntry): boolean =>
  !isConflicted(entry) && entry.staged !== 'unmodified' && entry.staged !== 'untracked'

export const isUnstaged = (entry: GitEntry): boolean =>
  !isConflicted(entry) && entry.unstaged !== 'unmodified'
