import { useEffect, useRef, useState } from 'react'

import { errorMessage, ipc } from '@/ipc'
import type { SessionKind } from '@/types/beacon'
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

      const terminal = await acquire(session.id, projectId)
      if (cancelled) return

      container.append(terminal.element)

      const apply = (): void => {
        terminal.fit.fit()
        void ipc.resizeSession(session.id, terminal.term.cols, terminal.term.rows)
      }

      // Wait a frame so the element has been laid out before measuring.
      const frame = requestAnimationFrame(() => {
        apply()
        setReady(true)
        if (autoFocus) terminal.term.focus()
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

/**
 * A first guess at the PTY grid, from the panel size and xterm's default cell.
 *
 * Only used for the initial spawn; the real measurement follows one frame later.
 */
function estimateGrid(container: HTMLElement): { cols: number; rows: number } {
  const { width, height } = container.getBoundingClientRect()
  return {
    cols: Math.max(20, Math.floor(width / 7.2)),
    rows: Math.max(5, Math.floor(height / 16.2)),
  }
}
