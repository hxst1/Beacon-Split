import { ReleaseNotes, useReleaseNotes } from '@/features/releases/ReleaseNotes'
import { installUpdate, useUpdates } from '@/features/releases/updates'
import { describeBinding } from './keymap'
import { selectActiveProject, selectBindings, useBeacon } from './store'
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
  const bindings = useBeacon(selectBindings)
  const detached = useBeacon((s) => s.detached)
  // Selected whole and filtered here: filtering inside the selector would hand
  // the store a new array every call.
  const missing = useBeacon((s) => s.missing)
  const missingRequired = missing.filter((entry) => entry.importance === 'required')
  const setOverlay = useBeacon((s) => s.setOverlay)
  const muted = useBeacon((s) => s.snapshot?.releaseNotices === false)
  const notes = useReleaseNotes()
  const update = useUpdates((s) => s.state)

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

      {update.status === 'available' ? (
        <button
          type="button"
          className={styles['update']}
          title={`Beacon ${update.version} is out. Beacon will restart to finish.`}
          onClick={() => void installUpdate()}
        >
          Update to {update.version}
        </button>
      ) : null}

      {update.status === 'downloading' ? (
        <span className={styles['update']}>Downloading… {update.progress}%</span>
      ) : null}

      {update.status === 'ready' ? (
        <span className={styles['update']}>Restarting…</span>
      ) : null}

      <button
        type="button"
        className={styles['bell']}
        data-unread={notes.unread}
        data-muted={muted}
        title={
          notes.unread
            ? 'A new version — see what changed'
            : muted
              ? "What's new (announcements are off)"
              : "What's new"
        }
        onClick={notes.open}
      >
        <BellIcon muted={muted} />
      </button>

      <span className={styles['hint']}>
        {hint('palette.open')} commands · {hint('quickOpen.open')} files ·{' '}
        {hint('panel.toggle.terminal')} terminal
      </span>
      {notes.showing ? (
        <ReleaseNotes releases={notes.showing} onClose={notes.close} />
      ) : null}
    </footer>
  )
}

/** Drawn inline, like the gear: one icon does not justify a dependency. */
const BellIcon = ({ muted }: { muted: boolean }): React.ReactElement => (
  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
    <path
      d="M4 6.6a4 4 0 0 1 8 0v3l1.1 1.9H2.9L4 9.6v-3Z"
      stroke="currentColor"
      strokeWidth="1.2"
      strokeLinejoin="round"
    />
    <path d="M6.6 13a1.6 1.6 0 0 0 2.8 0" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    {muted ? (
      <path d="M2.6 2.6l10.8 10.8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    ) : null}
  </svg>
)
