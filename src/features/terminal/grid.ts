/** A terminal grid, in cells. */
export interface Grid {
  cols: number
  rows: number
}

/** Below this, a panel is mid-layout rather than genuinely small. */
export const MIN_USABLE_PX = 60
export const MIN_COLS = 20
export const MIN_ROWS = 4

/**
 * Decides what a terminal should be resized to, or that it should be left alone.
 *
 * Kept apart from the component because it is the whole of the policy, and the
 * component around it is unmeasurable plumbing. `null` means change nothing —
 * and it has to mean changing *nothing*: the grid and the process are resized
 * together or not at all.
 *
 * That invariant is the fix for a real bug. Resizing the grid first and then
 * declining to pass an implausible result on left xterm two cells wide while
 * the process kept its real width; the process, never told anything changed,
 * had no reason to redraw, so the terminal stayed collapsed until it was
 * rebuilt from scratch.
 *
 * @param box      The panel's measured size in CSS pixels.
 * @param proposed What the fit addon would choose, measured but not applied.
 * @param current  The grid the terminal has now.
 */
export function nextGrid(
  box: { width: number; height: number },
  proposed: Grid | undefined,
  current: Grid,
): Grid | null {
  // A panel that is hidden, collapsing, or not yet placed.
  if (box.width < MIN_USABLE_PX || box.height < MIN_USABLE_PX) return null

  if (!proposed) return null
  const { cols, rows } = proposed

  // A cell measured as zero divides into infinity rather than into something
  // small, so a range check alone would let it through.
  if (!Number.isFinite(cols) || !Number.isFinite(rows)) return null
  if (cols < MIN_COLS || rows < MIN_ROWS) return null

  if (cols === current.cols && rows === current.rows) return null

  return { cols, rows }
}

/**
 * A first guess at the grid, from the panel size and xterm's default cell.
 *
 * Only used for the initial spawn, so that the first prompt is already laid out
 * correctly instead of reflowing a moment later; the real measurement follows
 * once the font has loaded. An unmeasurable panel falls back to a conventional
 * terminal size, which is wrong by a little rather than catastrophically.
 */
export function estimateGrid(box: { width: number; height: number }): Grid {
  if (box.width < MIN_USABLE_PX || box.height < MIN_USABLE_PX) return { cols: 80, rows: 24 }

  return {
    cols: Math.max(MIN_COLS, Math.floor(box.width / 7.2)),
    rows: Math.max(MIN_ROWS, Math.floor(box.height / 16.2)),
  }
}
