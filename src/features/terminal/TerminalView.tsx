import { useEffect, useRef, useState } from 'react'

import { errorMessage, ipc } from '@/ipc'
import type { SessionKind } from '@/types/beacon'
import { useActivity } from './activity'
import { estimateGrid, nextGrid } from './grid'
import { acquire } from './terminalHost'
import styles from './TerminalView.module.css'
import '@xterm/xterm/css/xterm.css'

interface TerminalViewProps {
  workspaceId: string
  projectId: string
  kind: SessionKind
  /** Which of the project's sessions of this kind. Claude only ever uses 0. */
  slot: number
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
  slot,
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
      const probe = estimateGrid(container.getBoundingClientRect())
      const session = await ipc.openSession(
        workspaceId,
        projectId,
        kind,
        slot,
        probe.cols,
        probe.rows,
      )
      if (cancelled) return
      useActivity.getState().sessionOpened(session.id, projectId)

      const terminal = await acquire(session.id, projectId, kind, slot)
      if (cancelled) return

      container.append(terminal.element)

      /**
       * Measures the panel and resizes the grid and the process together.
       *
       * `proposeDimensions` rather than `fit`, because the decision of whether
       * to resize at all has to be made before anything moves; `nextGrid`
       * carries that decision and why it is shaped this way.
       */
      const apply = (): void => {
        const { term, fit } = terminal
        const grid = nextGrid(container.getBoundingClientRect(), fit.proposeDimensions(), term)
        if (!grid) return

        term.resize(grid.cols, grid.rows)
        void ipc.resizeSession(session.id, grid.cols, grid.rows)
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
  }, [workspaceId, projectId, kind, slot, autoFocus])

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
