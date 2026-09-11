import { describe, expect, it } from 'vitest'

import { panelsOf, prune, sourcePath, withFraction } from './layout'
import type { LayoutNode, PanelId } from '@/types/beacon'

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

describe('sourcePath', () => {
  it('is the identity when nothing is hidden', () => {
    expect(sourcePath(tree, [], [])).toEqual([])
    expect(sourcePath(tree, [], ['first'])).toEqual(['first'])
    expect(sourcePath(tree, [], ['first', 'second'])).toEqual(['first', 'second'])
  })

  /**
   * The bug this exists for: hiding the terminal collapses the root, so every
   * splitter the user can see is one level deeper in the stored tree than it
   * looks. Dragging them wrote fractions into splits that were not on screen,
   * and the panels sat there refusing to move.
   */
  it('follows a collapsed root down to the split that is really being dragged', () => {
    const hidden: PanelId[] = ['terminal']

    // What looks like the root is the claude/editor-and-sidebar split.
    expect(sourcePath(tree, hidden, [])).toEqual(['first'])
    // What looks like its first child is the claude|editor split.
    expect(sourcePath(tree, hidden, ['first'])).toEqual(['first', 'first'])
    // And its second is files|git.
    expect(sourcePath(tree, hidden, ['second'])).toEqual(['first', 'second'])
  })

  it('sees through a split that lost a child further down', () => {
    // Hiding the editor collapses claude|editor, so the visible first child of
    // the root's first child is the files|git split.
    expect(sourcePath(tree, ['editor'], ['first'])).toEqual(['first'])
    expect(sourcePath(tree, ['editor'], ['first', 'second'])).toEqual(['first', 'second'])
  })

  it('resizes the split the user is actually pointing at', () => {
    const hidden: PanelId[] = ['terminal']
    const target = sourcePath(tree, hidden, ['first'])!
    const resized = withFraction(tree, target, 0.3)

    const root = resized as Extract<LayoutNode, { type: 'split' }>
    const column = root.first as Extract<LayoutNode, { type: 'split' }>
    const claudeEditor = column.first as Extract<LayoutNode, { type: 'split' }>

    expect(claudeEditor.fraction).toBe(0.3)
    // The splits nobody touched are untouched — including the hidden one.
    expect(root.fraction).toBe(tree.fraction)
    expect(column.fraction).toBe(0.74)
  })

  it('gives nothing back for a path that leads nowhere', () => {
    expect(sourcePath(tree, ['claude', 'editor', 'files', 'git', 'terminal'], [])).toBeNull()
  })
})
