import { describe, expect, it } from 'vitest'

import { panelsOf, prune, withFraction } from './layout'
import type { LayoutNode } from '@/types/beacon'

/** The default arrangement: Claude beside the editor, files over git, terminal below. */
const tree: LayoutNode = {
  type: 'split',
  direction: 'column',
  fraction: 0.72,
  first: {
    type: 'split',
    direction: 'row',
    fraction: 0.74,
    first: {
      type: 'split',
      direction: 'row',
      fraction: 0.58,
      first: { type: 'panel', panel: 'claude' },
      second: { type: 'panel', panel: 'editor' },
    },
    second: {
      type: 'split',
      direction: 'column',
      fraction: 0.6,
      first: { type: 'panel', panel: 'files' },
      second: { type: 'panel', panel: 'git' },
    },
  },
  second: { type: 'panel', panel: 'terminal' },
}

describe('prune', () => {
  it('leaves a layout with nothing hidden alone', () => {
    expect(prune(tree, [])).toEqual(tree)
  })

  it('collapses a split whose other child is hidden', () => {
    const pruned = prune(tree, ['editor'])
    expect(panelsOf(pruned!)).toEqual(['claude', 'files', 'git', 'terminal'])

    // Claude takes the whole region rather than leaving a gap where the editor was.
    const top = (pruned as Extract<LayoutNode, { type: 'split' }>).first
    const left = (top as Extract<LayoutNode, { type: 'split' }>).first
    expect(left).toEqual({ type: 'panel', panel: 'claude' })
  })

  it('collapses nested splits when a whole side is hidden', () => {
    const pruned = prune(tree, ['files', 'git'])
    expect(panelsOf(pruned!)).toEqual(['claude', 'editor', 'terminal'])
  })

  it('returns null when everything is hidden', () => {
    expect(prune(tree, ['claude', 'editor', 'files', 'git', 'terminal'])).toBeNull()
  })

  it('does not mutate the stored tree, so unhiding restores the arrangement', () => {
    const before = JSON.stringify(tree)
    prune(tree, ['git'])
    expect(JSON.stringify(tree)).toBe(before)
  })
})

describe('withFraction', () => {
  it('changes the split named by the path and no other', () => {
    const resized = withFraction(tree, ['first'], 0.4)
    const top = (resized as Extract<LayoutNode, { type: 'split' }>).first
    expect((top as Extract<LayoutNode, { type: 'split' }>).fraction).toBe(0.4)
    // The root keeps its own size.
    expect((resized as Extract<LayoutNode, { type: 'split' }>).fraction).toBe(0.72)
  })

  it('clamps a drag that would collapse a panel', () => {
    const resized = withFraction(tree, [], 0.98)
    expect((resized as Extract<LayoutNode, { type: 'split' }>).fraction).toBe(0.9)

    const other = withFraction(tree, [], -3)
    expect((other as Extract<LayoutNode, { type: 'split' }>).fraction).toBe(0.1)
  })

  it('leaves the original tree untouched', () => {
    const before = JSON.stringify(tree)
    withFraction(tree, ['first', 'second'], 0.2)
    expect(JSON.stringify(tree)).toBe(before)
  })
})
