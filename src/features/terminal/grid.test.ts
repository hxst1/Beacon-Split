import { describe, expect, it } from 'vitest'

import { estimateGrid, nextGrid } from './grid'

const PANEL = { width: 1024, height: 520 }
const CURRENT = { cols: 142, rows: 32 }

describe('nextGrid', () => {
  it('accepts a plausible measurement', () => {
    expect(nextGrid(PANEL, { cols: 120, rows: 40 }, CURRENT)).toEqual({ cols: 120, rows: 40 })
  })

  it('does nothing when the size has not changed', () => {
    expect(nextGrid(PANEL, { ...CURRENT }, CURRENT)).toBeNull()
  })

  /**
   * The bug this module exists for. A panel measured mid-layout proposes a
   * grid a couple of cells wide; if that is applied to the terminal and only
   * then rejected for the process, the two disagree permanently — the process
   * keeps drawing at its real width into a grid that cannot hold it, and never
   * learns it should redraw.
   */
  it.each([
    ['a collapsing panel', { width: 4, height: 520 }, { cols: 1, rows: 32 }],
    ['a hidden panel', { width: 0, height: 0 }, { cols: 0, rows: 0 }],
    ['too few columns', PANEL, { cols: 2, rows: 32 }],
    ['too few rows', PANEL, { cols: 142, rows: 1 }],
    ['nothing measurable', PANEL, undefined],
    ['a zero cell width', PANEL, { cols: Infinity, rows: 32 }],
    ['a cell measured as NaN', PANEL, { cols: Number.NaN, rows: Number.NaN }],
  ])('refuses %s, leaving both sides alone', (_case, box, proposed) => {
    expect(nextGrid(box, proposed, CURRENT)).toBeNull()
  })
})

describe('estimateGrid', () => {
  it('measures a laid-out panel', () => {
    const { cols, rows } = estimateGrid(PANEL)
    expect(cols).toBeGreaterThan(100)
    expect(rows).toBeGreaterThan(20)
  })

  it('falls back to a conventional size rather than a tiny one', () => {
    expect(estimateGrid({ width: 0, height: 0 })).toEqual({ cols: 80, rows: 24 })
  })

  it('never proposes a grid the resize path would reject', () => {
    for (const box of [
      { width: 61, height: 61 },
      { width: 120, height: 80 },
      { width: 3840, height: 2160 },
    ]) {
      const grid = estimateGrid(box)
      expect(nextGrid({ width: 1024, height: 520 }, grid, { cols: 0, rows: 0 })).toEqual(grid)
    }
  })
})
