import { describeBinding } from './keymap'
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
  const bindings = useBeacon((s) => s.snapshot?.bindings ?? [])
  const detached = useBeacon((s) => s.detached)
  const missingRequired = useBeacon((s) =>
    s.missing.filter((entry) => entry.importance === 'required'),
  )
  const setOverlay = useBeacon((s) => s.setOverlay)

  /** Whatever the shortcut actually is now, not what it shipped as. */
  const hint = (action: string): string => {
    const binding = bindings.find((entry) => entry.action === action)
    return binding ? describeBinding(binding.binding) : ''
  }

  return (
    <footer className={styles['bar']}>
      <span className={styles['path']}>{project?.displayPath ?? ''}</span>

      {missingRequired.length > 0 ? (
        <button
          type="button"
          className={styles['notice']}
          onClick={() => setOverlay('settings')}
          title="Settings → Requirements"
        >
          {missingRequired.map((entry) => entry.name).join(', ')} not installed
        </button>
      ) : null}

      {detached ? (
        <span className={styles['notice']}>Reconnecting to the session daemon…</span>
      ) : null}

      {notice ? (
        <span className={styles['notice']}>
          {notice}
          <button type="button" className={styles['dismiss']} onClick={dismissNotice}>
            ✕
          </button>
        </span>
      ) : null}

      <span className={styles['hint']}>
        {hint('palette.open')} commands · {hint('quickOpen.open')} files ·{' '}
        {hint('panel.toggle.terminal')} terminal
      </span>
    </footer>
  )
}
