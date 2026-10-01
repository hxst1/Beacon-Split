import { useEffect, useState } from 'react'

import { windowControls } from '@/ipc'
import { isWindows } from '@/lib/platform'
import styles from './WindowControls.module.css'

/**
 * Minimise, maximise and close, at the right end of the title bar.
 *
 * Only on Windows. macOS keeps its traffic lights under the overlay title bar;
 * on Windows there is no overlay to be had, and the system title bar would be a
 * second row of chrome above Beacon's own — so the window is undecorated and
 * these stand in, shaped and placed the way every Windows application has them.
 */
export function WindowControls(): React.ReactElement | null {
  const [maximized, setMaximized] = useState(false)
  const shown = isWindows()

  useEffect(() => {
    if (!shown) return undefined

    let cancelled = false
    const refresh = (): void => {
      void windowControls.isMaximized().then((value) => {
        if (!cancelled) setMaximized(value)
      })
    }
    refresh()
    const stop = windowControls.onResized(refresh)
    return () => {
      cancelled = true
      void stop.then((unlisten) => unlisten())
    }
  }, [shown])

  if (!shown) return null

  return (
    <div className={styles['controls']}>
      <button
        type="button"
        className={styles['button']}
        title="Minimise"
        aria-label="Minimise"
        onClick={() => void windowControls.minimize()}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M0 5.5h10" />
        </svg>
      </button>
      <button
        type="button"
        className={styles['button']}
        title={maximized ? 'Restore' : 'Maximise'}
        aria-label={maximized ? 'Restore' : 'Maximise'}
        onClick={() => void windowControls.toggleMaximize()}
      >
        {maximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z" />
          </svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M.5.5h9v9h-9z" />
          </svg>
        )}
      </button>
      <button
        type="button"
        className={`${styles['button']} ${styles['close']}`}
        title="Close"
        aria-label="Close"
        onClick={() => void windowControls.close()}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M.5.5l9 9M9.5.5l-9 9" />
        </svg>
      </button>
    </div>
  )
}
