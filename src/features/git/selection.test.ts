import { describe, expect, it } from 'vitest'

import type { FileState, GitEntry, GitStatus } from '@/types/beacon'
import { isStaged, isUnstaged, reconcileGitSelection, type GitSelection } from './selection'

const entry = (
  path: string,
  staged: FileState,
  unstaged: FileState,
): GitEntry => ({ path, staged, unstaged, conflicted: false })

const conflict = (path: string): GitEntry => ({
  path,
  staged: 'conflicted',
  unstaged: 'conflicted',
  conflicted: true,
})

const status = (entries: GitEntry[], unborn = false): GitStatus => ({
  branch: 'main',
  ahead: 0,
  behind: 0,
  unborn,
  entries,
})

const selected = (
  path: string,
  staged: boolean,
  state: FileState,
): GitSelection => ({
  path,
  staged,
  untracked: !staged && state === 'untracked',
  state,
})

describe('reconcileGitSelection', () => {
  it('keeps the selected tracked side and refreshes its state', () => {
    expect(
      reconcileGitSelection(
        selected('src/app.ts', false, 'modified'),
        status([entry('src/app.ts', 'unmodified', 'deleted')]),
      ),
    ).toEqual(selected('src/app.ts', false, 'deleted'))
  })

  it('moves an untracked selection to the staged side after stage', () => {
    expect(
      reconcileGitSelection(
        selected('new.ts', false, 'untracked'),
        status([entry('new.ts', 'added', 'unmodified')]),
      ),
    ).toEqual(selected('new.ts', true, 'added'))
  })

  it.each([false, true])(
    'moves a staged addition back to untracked (unborn: %s)',
    (unborn) => {
      expect(
        reconcileGitSelection(
          selected('new.ts', true, 'added'),
          status([entry('new.ts', 'unmodified', 'untracked')], unborn),
        ),
      ).toEqual(selected('new.ts', false, 'untracked'))
    },
  )

  it('keeps the requested side for a partially staged file', () => {
    const next = status([entry('both.ts', 'modified', 'modified')])

    expect(reconcileGitSelection(selected('both.ts', true, 'modified'), next)).toEqual(
      selected('both.ts', true, 'modified'),
    )
    expect(reconcileGitSelection(selected('both.ts', false, 'modified'), next)).toEqual(
      selected('both.ts', false, 'modified'),
    )
  })

  it('preserves selection identity when a poll has not changed its side', () => {
    const current = selected('same.ts', false, 'modified')

    expect(
      reconcileGitSelection(
        current,
        status([entry('same.ts', 'unmodified', 'modified')]),
      ),
    ).toBe(current)
  })

  it('gives a conflict one selection rather than a staged and an unstaged one', () => {
    const next = status([conflict('conflict.ts')])
    const one = selected('conflict.ts', false, 'conflicted')

    expect(reconcileGitSelection(selected('conflict.ts', true, 'conflicted'), next)).toEqual(one)
    expect(reconcileGitSelection(one, next)).toEqual(one)
  })

  it('follows a resolved conflict to the side it ended up on', () => {
    expect(
      reconcileGitSelection(
        selected('conflict.ts', false, 'conflicted'),
        status([entry('conflict.ts', 'modified', 'unmodified')]),
      ),
    ).toEqual(selected('conflict.ts', true, 'modified'))
  })

  it('moves a committed staged diff to remaining working changes', () => {
    expect(
      reconcileGitSelection(
        selected('continued.ts', true, 'modified'),
        status([entry('continued.ts', 'unmodified', 'modified')]),
      ),
    ).toEqual(selected('continued.ts', false, 'modified'))
  })

  it('closes the diff after a clean commit or external removal', () => {
    expect(
      reconcileGitSelection(selected('committed.ts', true, 'modified'), status([])),
    ).toBeNull()
    expect(
      reconcileGitSelection(
        selected('removed.ts', false, 'modified'),
        status([entry('another.ts', 'unmodified', 'modified')]),
      ),
    ).toBeNull()
  })

  it('closes the diff when the project is no longer a repository', () => {
    expect(reconcileGitSelection(selected('file.ts', false, 'modified'), null)).toBeNull()
  })
})

describe('which list a path belongs in', () => {
  it('puts a conflict in neither, so it is never drawn twice', () => {
    // Both a stage and an unstage button on the same file are two ways to tell
    // git the conflict is resolved while the markers are still in it.
    const unmerged = conflict('conflict.ts')

    expect(isStaged(unmerged)).toBe(false)
    expect(isUnstaged(unmerged)).toBe(false)
  })

  it('puts a partly staged file in both, which is what it is', () => {
    const both = entry('both.ts', 'modified', 'modified')

    expect(isStaged(both)).toBe(true)
    expect(isUnstaged(both)).toBe(true)
  })

  it('leaves an untracked file out of the staged list', () => {
    const fresh = entry('new.ts', 'unmodified', 'untracked')

    expect(isStaged(fresh)).toBe(false)
    expect(isUnstaged(fresh)).toBe(true)
  })
})
