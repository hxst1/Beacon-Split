import { describe, expect, it } from 'vitest'

import { fuzzyMatch } from '@/lib/fuzzy'
import { isHiddenPath, searchFiles } from './fileSearch'

const FILES = [
  'README.md',
  'src/app/store.ts',
  'src/features/files/FileTree.tsx',
  'src/features/files/treeStore.ts',
  '.github/workflows/ci.yml',
  'src/.env',
]

describe('searchFiles', () => {
  it('finds nothing for an empty query, rather than everything', () => {
    expect(searchFiles(FILES, '')).toEqual([])
    expect(searchFiles(FILES, '   ')).toEqual([])
  })

  it('splits a hit into its name and the folder it is in', () => {
    const [hit] = searchFiles(FILES, 'FileTree')
    expect(hit).toMatchObject({
      path: 'src/features/files/FileTree.tsx',
      name: 'FileTree.tsx',
      dir: 'src/features/files',
    })
  })

  it('gives a file at the root an empty folder', () => {
    expect(searchFiles(FILES, 'readme')[0]).toMatchObject({ name: 'README.md', dir: '' })
  })

  it('highlights only characters inside the name', () => {
    const [hit] = searchFiles(FILES, 'store')
    expect(hit?.name).toBe('store.ts')
    expect(hit?.positions).toEqual([0, 1, 2, 3, 4])
  })

  it('puts a file whose name matches ahead of one that needs its folders', () => {
    // On fuzzy score alone the path wins — a tight run at the start against
    // letters scattered through a long name — so only the name bonus can put
    // the name first.
    const byPath = 'ab/x.ts'
    const byName = 'src/a-long-name-with-a-b.ts'
    expect(fuzzyMatch(byPath, 'ab')!.score).toBeGreaterThan(fuzzyMatch('a-long-name-with-a-b.ts', 'ab')!.score)

    expect(searchFiles([byPath, byName], 'ab').map((hit) => hit.path)).toEqual([byName, byPath])
  })

  it('marks the part of a path match that falls in the name, shifted to it', () => {
    const hit = searchFiles(['src/features/files/treeStore.ts'], 'files/tree')[0]
    expect(hit?.name).toBe('treeStore.ts')
    expect(hit?.positions).toEqual([0, 1, 2, 3])
  })

  it('matches case-sensitively once the query has a capital', () => {
    expect(searchFiles(FILES, 'Tree').map((hit) => hit.path)).toEqual(['src/features/files/FileTree.tsx'])
    expect(searchFiles(FILES, 'tree').map((hit) => hit.path)).toContain('src/features/files/treeStore.ts')
  })

  it('matches across folders too', () => {
    expect(searchFiles(FILES, 'files/tree').map((hit) => hit.path)).toContain(
      'src/features/files/treeStore.ts',
    )
  })

  it('leaves dotfiles out while the tree is hiding them', () => {
    expect(searchFiles(FILES, 'ci', { showHidden: false }).map((hit) => hit.path)).not.toContain(
      '.github/workflows/ci.yml',
    )
    expect(searchFiles(FILES, 'env', { showHidden: false })).toEqual([])
    expect(searchFiles(FILES, 'env', { showHidden: true })[0]?.path).toBe('src/.env')
  })

  it('stops at the limit', () => {
    expect(searchFiles(FILES, 's', { limit: 2 })).toHaveLength(2)
  })
})

describe('isHiddenPath', () => {
  it('is true for a dotfile and for anything inside a dot-folder', () => {
    expect(isHiddenPath('.env')).toBe(true)
    expect(isHiddenPath('.github/workflows/ci.yml')).toBe(true)
    expect(isHiddenPath('src/app.ts')).toBe(false)
  })
})
