import { describe, expect, it } from 'vitest'

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
    const hits = searchFiles(['src/stuff/a.ts', 'lib/store.ts'], 'st')
    expect(hits.map((hit) => hit.path)).toEqual(['lib/store.ts', 'src/stuff/a.ts'])
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
