import { useState } from 'react'
import { createPortal } from 'react-dom'

import { useBeacon } from '@/app/store'
import { useNotificationPermission } from './permission'
import styles from './NotificationPrompt.module.css'

/**
 * The one time Beacon asks to be allowed to interrupt you.
 *
 * Shown on the first launch that has notifications switched on and no answer
 * from macOS yet. It exists because macOS offers its prompt once per
 * application, ever: a prompt spent in the background, on an event nobody was
 * watching, is a prompt the user never sees and can then only undo by finding
 * the right row in System Settings. So Beacon asks in the open, having said
 * what it wants it for.
 *
 * It needs nothing remembered. `notDetermined` is a state macOS never returns
 * to once answered, so this cannot appear twice to anyone who answers it.
 */
export function NotificationPrompt(): React.ReactElement | null {
  const enabled = useBeacon((s) => s.snapshot?.notifications ?? true)
  const { permission, asking, request, openSettings } = useNotificationPermission()
  const [dismissed, setDismissed] = useState(false)
  const [asked, setAsked] = useState(false)

  // Refused right here, rather than at some point in the past: worth one more
  // line telling the user where the switch went, instead of vanishing.
  const refused = asked && permission === 'denied'
  const unanswered = permission === 'notDetermined'

  if (dismissed || !enabled || (!unanswered && !refused)) return null

  return createPortal(
    <div className={styles['scrim']}>
      <div className={styles['panel']} role="dialog" aria-label="Allow notifications">
        {refused ? (
          <>
            <h2 className={styles['title']}>macOS is holding those back</h2>
            <p className={styles['body']}>
              That answer is final as far as Beacon is concerned — the system prompt is offered
              once per application. Notifications can still be switched on by hand, in System
              Settings, under Beacon Split.
            </p>
            <div className={styles['actions']}>
              <button type="button" className={styles['later']} onClick={() => setDismissed(true)}>
                Leave them off
              </button>
              <button type="button" className={styles['allow']} onClick={() => void openSettings()}>
                Open System Settings
              </button>
            </div>
          </>
        ) : (
          <>
            <h2 className={styles['title']}>Let Beacon tell you when Claude is done</h2>
            <p className={styles['body']}>
              With several projects open, the costly thing is a session finishing, or stopping to
              ask for permission, while you are in another window. Beacon can say so — naming the
              workspace and the project — and stays quiet about whatever you are already looking
              at.
            </p>
            <p className={styles['note']}>
              macOS offers this prompt once per application. Skipping it means allowing them later
              from System Settings instead.
            </p>
            <div className={styles['actions']}>
              <button type="button" className={styles['later']} onClick={() => setDismissed(true)}>
                Not now
              </button>
              <button
                type="button"
                className={styles['allow']}
                disabled={asking}
                onClick={() => {
                  setAsked(true)
                  void request()
                }}
              >
                {asking ? 'Waiting for macOS…' : 'Allow notifications'}
              </button>
            </div>
          </>
        )}
      </div>
    </div>,
    document.body,
  )
}
