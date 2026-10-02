import { useState } from 'react'

import { writeToClipboard } from '@/features/clips/clips'
import { useReplies } from './replies'
import styles from './LastReply.module.css'

/** How long "Copied" stays before the button says "Copy" again. */
const COPIED_FOR_MS = 1500

/**
 * Claude's last reply, above its terminal, until the next turn starts.
 *
 * The point is to find the answer after a long turn without scrolling for it:
 * the reply is what you came back for, and the tool output it ended under is
 * usually not. Folded to a few lines so it never pushes the terminal away;
 * open it to read the rest.
 */
export function LastReply({ projectId }: { projectId: string }): React.ReactElement | null {
  const reply = useReplies((s) => s.replies[projectId])
  const dismiss = useReplies((s) => s.dismiss)
  const [open, setOpen] = useState(false)
  const [copied, setCopied] = useState(false)

  if (!reply) return null

  const copy = async (): Promise<void> => {
    if (!(await writeToClipboard(reply))) return
    setCopied(true)
    window.setTimeout(() => setCopied(false), COPIED_FOR_MS)
  }

  return (
    <section className={styles['reply']} data-open={open} aria-label="Claude's last reply">
      <div className={styles['text']}>{reply}</div>
      <div className={styles['actions']}>
        <button
          type="button"
          className={styles['action']}
          aria-expanded={open}
          onClick={() => setOpen(!open)}
        >
          {open ? 'Less' : 'More'}
        </button>
        <button type="button" className={styles['action']} onClick={() => void copy()}>
          {copied ? 'Copied' : 'Copy'}
        </button>
        <button
          type="button"
          className={styles['action']}
          title="Put it away until the next reply"
          aria-label="Dismiss the last reply"
          onClick={() => {
            setOpen(false)
            dismiss(projectId)
          }}
        >
          ×
        </button>
      </div>
    </section>
  )
}
