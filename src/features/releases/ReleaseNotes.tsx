import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'

import { useBeacon } from '@/app/store'
import { ipc } from '@/ipc'
import type { Release } from '@/types/beacon'
import styles from './ReleaseNotes.module.css'

/**
 * What changed, shown once per version.
 *
 * Opened by itself when a version is new, unless that was muted, and by the
 * bell whenever anyone wants it. Muting silences the announcement, not the
 * information — the bell still says there is something to read.
 */
export function ReleaseNotes({
  releases,
  onClose,
}: {
  releases: Release[]
  onClose: () => void
}): React.ReactElement | null {
  const notices = useBeacon((s) => s.snapshot?.releaseNotices ?? true)
  const setReleaseNotices = useBeacon((s) => s.setReleaseNotices)

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [onClose])

  const newest = releases[0]
  if (!newest) return null

  return createPortal(
    <div className={styles['scrim']} onPointerDown={onClose}>
      <div className={styles['panel']} onPointerDown={(event) => event.stopPropagation()}>
        <header className={styles['header']}>
          <span className={styles['title']}>What's new</span>
          <span className={styles['version']}>{newest.version}</span>
          <span className={styles['date']}>{newest.date}</span>
        </header>

        <div className={styles['body']}>
          {releases.map((release, index) => (
            <section className={styles['release']} key={release.version}>
              {index > 0 ? (
                <div className={styles['olderHeading']}>
                  {release.version} · {release.date}
                </div>
              ) : null}
              {release.summary ? <p className={styles['summary']}>{release.summary}</p> : null}
              <ul className={styles['changes']}>
                {release.changes.map((change) => (
                  <li className={styles['change']} key={change}>
                    {change}
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>

        <footer className={styles['footer']}>
          <button
            type="button"
            className={styles['mute']}
            onClick={() => void setReleaseNotices(!notices)}
          >
            {notices
              ? 'Stop opening this when a version is new'
              : 'Open this again when a version is new'}
          </button>
          <button type="button" className={styles['done']} onClick={onClose}>
            Done
          </button>
        </footer>
      </div>
    </div>,
    document.body,
  )
}

/**
 * Decides what the bell should do, and opens the notes when a version is new.
 *
 * Kept together because they are one behaviour: the bell is unread state, and
 * the panel is what clears it.
 */
export function useReleaseNotes(): {
  unread: boolean
  showing: Release[] | null
  open: () => void
  close: () => void
} {
  const unseen = useBeacon((s) => s.snapshot?.unseenReleases)
  const notices = useBeacon((s) => s.snapshot?.releaseNotices ?? true)
  const markSeen = useBeacon((s) => s.markReleasesSeen)

  const [showing, setShowing] = useState<Release[] | null>(null)
  const [announced, setAnnounced] = useState(false)

  // Announced once per launch: the notes are what mark a version seen, so
  // without this a muted-then-unmuted session could reopen them repeatedly.
  useEffect(() => {
    if (announced || !unseen || unseen.length === 0) return
    setAnnounced(true)
    if (notices) setShowing(unseen)
  }, [announced, notices, unseen])

  const close = (): void => {
    setShowing(null)
    void markSeen()
  }

  return {
    unread: (unseen?.length ?? 0) > 0,
    showing,
    open: () => {
      void ipc.releaseNotes().then((all) => setShowing(all))
    },
    close,
  }
}
