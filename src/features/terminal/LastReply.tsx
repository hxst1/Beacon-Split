import { useState } from 'react'

import { Popover } from '@/app/ui/Popover'
import { writeToClipboard } from '@/features/clips/clips'
import { useReplies } from './replies'
import styles from './LastReply.module.css'

/** How long "Copied" stays before the button says "Copy" again. */
const COPIED_FOR_MS = 1500

/**
 * Claude's last reply, a chip in the panel header until the next turn starts.
 *
 * The point is to find the answer after a long turn without scrolling for it:
 * the reply is what you came back for, and the tool output it ended under is
 * usually not. It used to float over the terminal's top rows, which covered
 * the work every time a turn ended whether or not the reply was wanted. In the
 * header it is a click away and covers nothing; the dot says there is one you
 * have not opened. See ADR-101.
 */
export function LastReply({ projectId }: { projectId: string }): React.ReactElement | null {
  const reply = useReplies((s) => s.replies[projectId])
  const dismiss = useReplies((s) => s.dismiss)
  const [anchor, setAnchor] = useState<DOMRect | null>(null)
  const [seen, setSeen] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  if (!reply) return null

  const unread = reply !== seen
  const close = (): void => setAnchor(null)

  const copy = async (): Promise<void> => {
    if (!(await writeToClipboard(reply))) return
    setCopied(true)
    window.setTimeout(() => setCopied(false), COPIED_FOR_MS)
  }

  return (
    <>
      <button
        type="button"
        className={styles['chip']}
        title="What Claude said at the end of the last turn"
        aria-expanded={anchor !== null}
        data-unread={unread}
        onClick={(event) => {
          setAnchor(event.currentTarget.getBoundingClientRect())
          setSeen(reply)
        }}
      >
        <span className={styles['dot']} aria-hidden="true" />
        Last reply
      </button>
      {anchor ? (
        <Popover anchor={anchor} align="end" role="dialog" onClose={close}>
          <section className={styles['reply']} aria-label="Claude's last reply">
            <div className={styles['text']}>{reply}</div>
            <div className={styles['actions']}>
              <button type="button" className={styles['action']} onClick={() => void copy()}>
                {copied ? 'Copied' : 'Copy'}
              </button>
              <button
                type="button"
                className={styles['action']}
                title="Put it away until the next reply"
                onClick={() => {
                  close()
                  dismiss(projectId)
                }}
              >
                Dismiss
              </button>
            </div>
          </section>
        </Popover>
      ) : null}
    </>
  )
}
