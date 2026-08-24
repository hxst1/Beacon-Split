import { shortcutLabel } from '@/lib/platform'
import { selectActiveProject, useBeacon } from './store'
import styles from './StatusBar.module.css'

/**
 * A single quiet line: where you are, plus any error the last action produced.
 *
 * Errors surface here rather than in a dialog — nothing in Beacon should
 * interrupt what you were doing.
 */
export function StatusBar(): React.ReactElement {
  const project = useBeacon(selectActiveProject)
  const notice = useBeacon((s) => s.notice)
  const dismissNotice = useBeacon((s) => s.dismissNotice)

  return (
    <footer className={styles['bar']}>
      <span className={styles['path']}>{project?.displayPath ?? ''}</span>

      {notice ? (
        <span className={styles['notice']}>
          {notice}
          <button type="button" className={styles['dismiss']} onClick={dismissNotice}>
            ✕
          </button>
        </span>
      ) : null}

      <span className={styles['hint']}>
        {shortcutLabel('E')} files · {shortcutLabel('G')} git · {shortcutLabel('J')} terminal ·{' '}
        {shortcutLabel('↩')} fullscreen
      </span>
    </footer>
  )
}
