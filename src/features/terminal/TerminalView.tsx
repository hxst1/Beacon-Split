import { useEffect, useRef, useState } from 'react'

import { errorMessage, ipc } from '@/ipc'
import type { SessionKind } from '@/types/beacon'
import { useActivity } from './activity'
import { acquire } from './terminalHost'
import styles from './TerminalView.module.css'
import '@xterm/xterm/css/xterm.css'

interface TerminalViewProps {
  workspaceId: string
  projectId: string
  kind: SessionKind
  /** Focus the terminal once it is ready. */
  autoFocus?: boolean
}

/**
 * Renders one live session.
 *
 * The xterm instance itself is owned by `terminalHost` and outlives this
 * component: mounting reparents an existing terminal when there is one, so
 * switching projects does not rebuild or replay anything.
 */
export function TerminalView({
  workspaceId,
  projectId,
  kind,
  autoFocus = false,
}: TerminalViewProps): React.ReactElement {
  const containerRef = useRef<HTMLDivElement>(null)
  const [error, setError] = useState<string | null>(null)
  const [ready, setReady] = useState(false)

  useEffect(() => {
    let cancelled = false
    let cleanup: (() => void) | undefined
    setReady(false)
    setError(null)

    const start = async (): Promise<void> => {
      const container = containerRef.current
      if (!container) return

      // Size the PTY from the panel before spawning, so the first prompt is
      // already laid out correctly rather than reflowing a moment later.
      const probe = estimateGrid(container)
      const session = await ipc.openSession(workspaceId, projectId, kind, probe.cols, probe.rows)
      if (cancelled) return
      useActivity.getState().sessionOpened(session.id, projectId)

      const terminal = await acquire(session.id, projectId, kind)
      if (cancelled) return

      container.append(terminal.element)

      /**
       * Measures the panel and tells the process what size it is.
       *
       * Guarded, because a panel caught mid-layout — hidden, collapsing, or not
       * yet placed — measures as almost nothing. Fitting to that and sending it
       * on tells the process it has two columns, and everything it prints until
       * the next resize is wrapped at two columns. That damage is already in the
       * scrollback by the time the real size arrives, which is why it looked
       * like a rendering bug that a restart fixed.
       */
      const apply = (): void => {
        const box = container.getBoundingClientRect()
        if (box.width < MIN_USABLE_PX || box.height < MIN_USABLE_PX) return

        terminal.fit.fit()
        const { cols, rows } = terminal.term
        if (cols < MIN_COLS || rows < MIN_ROWS) return

        void ipc.resizeSession(session.id, cols, rows)
      }

      // Cell metrics depend on the font, so fitting before it loads measures
      // the fallback and gets the grid wrong.
      const frame = requestAnimationFrame(() => {
        void document.fonts.ready.then(() => {
          if (cancelled) return
          apply()
          setReady(true)
          if (autoFocus) terminal.term.focus()
        })
      })

      const observer = new ResizeObserver(() => apply())
      observer.observe(container)

      cleanup = () => {
        cancelAnimationFrame(frame)
        observer.disconnect()
        // Detach the element but keep the terminal alive for the next mount.
        terminal.element.remove()
      }
    }

    void start().catch((err: unknown) => {
      if (!cancelled) setError(errorMessage(err))
    })

    return () => {
      cancelled = true
      cleanup?.()
    }
  }, [workspaceId, projectId, kind, autoFocus])

  return (
    <div className={styles['root']} ref={containerRef}>
      {error ? (
        <div className={`${styles['status']} ${styles['failed']}`}>{error}</div>
      ) : !ready ? (
        <div className={styles['status']}>Starting…</div>
      ) : null}
    </div>
  )
}

/** Below this, a panel is mid-layout rather than genuinely small. */
const MIN_USABLE_PX = 60
const MIN_COLS = 20
const MIN_ROWS = 4

/**
 * A first guess at the PTY grid, from the panel size and xterm's default cell.
 *
 * Only used for the initial spawn; the real measurement follows once the font
 * has loaded. An unmeasurable panel falls back to a conventional terminal size,
 * which is wrong by a little rather than catastrophically.
 */
function estimateGrid(container: HTMLElement): { cols: number; rows: number } {
  const { width, height } = container.getBoundingClientRect()
  if (width < MIN_USABLE_PX || height < MIN_USABLE_PX) return { cols: 80, rows: 24 }

  return {
    cols: Math.max(MIN_COLS, Math.floor(width / 7.2)),
    rows: Math.max(MIN_ROWS, Math.floor(height / 16.2)),
  }
}
