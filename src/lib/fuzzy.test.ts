import { describe, expect, it } from 'vitest'

import { fuzzyMatch, rank } from './fuzzy'

describe('fuzzyMatch', () => {
  it('matches characters in order, not as a substring', () => {
    expect(fuzzyMatch('src/features/git/GitPane.tsx', 'gitpane')).not.toBeNull()
    expect(fuzzyMatch('src/app/store.ts', 'sas')).not.toBeNull()
  })

  it('refuses a candidate missing one of the characters', () => {
    expect(fuzzyMatch('src/app/store.ts', 'storez')).toBeNull()
  })

  it('reports where it matched, so the result can explain itself', () => {
    // s-t-o-r-e-.-t-s: the second `s` is the last character, at index 7.
    const match = fuzzyMatch('store.ts', 'sts')
    expect(match?.positions).toEqual([0, 1, 7])
  })

  it('is case-insensitive until the query says otherwise', () => {
    expect(fuzzyMatch('GitPane.tsx', 'gitpane')).not.toBeNull()
    // An uppercase letter in the query means the user meant it.
    expect(fuzzyMatch('gitpane.tsx', 'GitPane')).toBeNull()
  })

  it('treats an empty query as matching everything', () => {
    expect(fuzzyMatch('anything', '')).toEqual({ score: 0, positions: [] })
  })

  it('ignores spaces in the query, so words can be separated', () => {
    expect(fuzzyMatch('src/features/git/GitPane.tsx', 'git pane')).not.toBeNull()
  })
})

describe('ranking', () => {
  const score = (candidate: string, query: string): number =>
    fuzzyMatch(candidate, query)?.score ?? Number.NEGATIVE_INFINITY

  it('prefers a match at a word boundary over one in the middle', () => {
    // `ap` at the start of a segment beats the same letters buried in a word.
    expect(score('src/app.ts', 'ap')).toBeGreaterThan(score('src/scrap.ts', 'ap'))
  })

  it('prefers contiguous matches over scattered ones', () => {
    expect(score('store.ts', 'store')).toBeGreaterThan(score('s-t-o-r-e.ts', 'store'))
  })

  it('prefers the shorter path when both match equally well', () => {
    expect(score('app.ts', 'app')).toBeGreaterThan(score('app/nested/deeper/thing.ts', 'app'))
  })

  it('orders results best first and drops the rest', () => {
    const files = ['src/app/store.ts', 'src/features/git/GitPane.tsx', 'README.md']
    const ranked = rank(files, 'store', (path) => path)

    expect(ranked).toHaveLength(1)
    expect(ranked[0]?.item).toBe('src/app/store.ts')
  })

  it('honours the limit', () => {
    const many = Array.from({ length: 50 }, (_, index) => `file-${index}.ts`)
    expect(rank(many, 'file', (path) => path, 10)).toHaveLength(10)
  })
})
