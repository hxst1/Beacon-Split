/**
 * Re-reading the file tree when something inside Beacon has changed the disk.
 *
 * Beacon does not watch the filesystem (see `docs/DECISIONS.md`, ADR-025) and
 * the tree re-reads when the window regains focus — which is exactly the event
 * that never arrives when the thing writing the files is the Claude in the
 * panel beside it, or a shell in the panel below it. Both live in this window,
 * so it never loses the focus it would need to get back.
 *
 * What this listens to instead is the sessions themselves: Claude Code says
 * when it starts a tool and when it stops, and every session says when it has
 * written something to its terminal. Neither is a filesystem event, so neither
 * proves a file was created — but between them they cover every way a file
 * appears in a project without anyone leaving the window, and a directory
 * listing is cheap enough to be wrong about.
 */

/**
 * How long to wait for a burst of reports to stop before re-reading.
 *
 * A turn is a stream: tokens, a tool, more tokens. The tree only has to be
 * right once that has gone quiet, and re-reading between two tool calls is
 * work nobody would see.
 */
export const SETTLE_MS = 400

/**
 * The longest a burst can hold the tree back.
 *
 * Without this a session that never goes quiet for {@link SETTLE_MS} — a
 * spinner, a long build scrolling past — would postpone the re-read forever,
 * which is the one case where somebody is definitely watching something happen.
 */
export const AT_MOST_EVERY_MS = 2000

export interface LiveTree {
  /** A session belonging to `project` did something. */
  report: (project: string) => void
  /** Drops anything scheduled. For unmounting. */
  cancel: () => void
}

/**
 * Collects reports and calls `refresh` once they settle.
 *
 * `looking` is asked rather than assumed so this can be tested without a DOM,
 * and so a window nobody is in front of does no work: the focus it gets back
 * re-reads everything anyway.
 */
export function liveTree(
  projectId: string,
  refresh: () => void,
  looking: () => boolean = () => !document.hidden && document.hasFocus(),
): LiveTree {
  let timer: ReturnType<typeof setTimeout> | null = null
  /** When the burst now waiting began, so it cannot be extended indefinitely. */
  let waitingSince: number | null = null

  const cancel = (): void => {
    if (timer !== null) clearTimeout(timer)
    timer = null
    waitingSince = null
  }

  const fire = (): void => {
    timer = null
    waitingSince = null
    refresh()
  }

  return {
    report: (project) => {
      // Another project's session changes another project's files.
      if (project !== projectId) return
      if (!looking()) return

      const now = Date.now()
      waitingSince ??= now

      // Settle, unless settling would push the re-read past the cap — in which
      // case what is left of the cap is the wait.
      const remaining = waitingSince + AT_MOST_EVERY_MS - now
      const wait = Math.max(0, Math.min(SETTLE_MS, remaining))

      if (timer !== null) clearTimeout(timer)
      timer = setTimeout(fire, wait)
    },
    cancel,
  }
}
