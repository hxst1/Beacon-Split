import { useState } from 'react'

import { useBeacon } from '@/app/store'
import { useTypeInTerminal } from './typeInTerminal'
import type { Requirement } from '@/types/beacon'
import styles from './MissingTool.module.css'

/**
 * Stands in for a panel whose program is not installed.
 *
 * Shown where the gap is felt rather than only in settings: someone opening the
 * Git panel and finding it empty should learn why there, not have to go looking
 * for a diagnostics screen they do not know exists.
 */
export function MissingTool({ requirement }: { requirement: Requirement }): React.ReactElement {
  const setOverlay = useBeacon((s) => s.setOverlay)
  const { type } = useTypeInTerminal()
  const [copied, setCopied] = useState(false)
  const first = requirement.install[0]

  return (
    <div className={styles['root']}>
      <div className={styles['title']}>
        {requirement.state === 'broken'
          ? `${requirement.name} is installed, but it did not answer`
          : `${requirement.name} is not installed`}
      </div>
      <p className={styles['body']}>
        {requirement.state === 'broken'
          ? // Telling somebody to install what they are looking at is how a
            // program convinces them it is not listening. Where it is and what
            // it did instead is the useful thing to say.
            `Beacon found it at ${requirement.path} and asked it for its version, and it did not reply. Installing it again usually fixes that.`
          : requirement.whatBreaks}
      </p>

      {first ? (
        <button
          type="button"
          className={styles['command']}
          title="Copy"
          onClick={() => {
            void navigator.clipboard.writeText(first.command)
            setCopied(true)
            window.setTimeout(() => setCopied(false), 1200)
          }}
        >
          {copied ? 'Copied' : first.command}
        </button>
      ) : null}

      {/* Beacon is the terminal here, so the command can go where it belongs
          rather than through the clipboard and a window somewhere else. It
          stops at typing it: running an installer is a decision, and this
          leaves it to the person making it. */}
      {first && type ? (
        <button
          type="button"
          className={styles['more']}
          onClick={() => type(first.command)}
        >
          Type it into a terminal here, ready to run
        </button>
      ) : null}

      <button type="button" className={styles['more']} onClick={() => setOverlay('settings')}>
        Other ways to install it, in Settings → Requirements
      </button>
    </div>
  )
}
