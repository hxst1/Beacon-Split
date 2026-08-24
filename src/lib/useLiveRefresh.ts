import { useEffect } from 'react'

/**
 * Runs `refresh` when the window regains focus, and on an interval while it is
 * focused.
 *
 * Beacon deliberately does not watch the filesystem. A recursive watch over a
 * project is cheap on macOS and expensive on Linux, where inotify needs a watch
 * per directory and a large `node_modules` can exhaust the system limit. What
 * this covers is the case that actually happens: you changed something in a
 * terminal, and you want the panel to agree when you look at it.
 *
 * Pass `intervalMs` as `null` for focus-only refreshing, which is right for
 * anything whose refresh would disturb what the user is doing.
 */
export function useLiveRefresh(refresh: () => void, intervalMs: number | null): void {
  useEffect(() => {
    const run = (): void => {
      // Nothing to update if nobody is looking.
      if (document.hidden || !document.hasFocus()) return
      refresh()
    }

    window.addEventListener('focus', run)
    const timer = intervalMs === null ? null : window.setInterval(run, intervalMs)

    return () => {
      window.removeEventListener('focus', run)
      if (timer !== null) window.clearInterval(timer)
    }
  }, [refresh, intervalMs])
}
