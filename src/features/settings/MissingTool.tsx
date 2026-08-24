import { useState } from 'react'

import { useBeacon } from '@/app/store'
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
  const [copied, setCopied] = useState(false)
  const first = requirement.install[0]

  return (
    <div className={styles['root']}>
      <div className={styles['title']}>{requirement.name} is not installed</div>
      <p className={styles['body']}>{requirement.whatBreaks}</p>

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

      <button type="button" className={styles['more']} onClick={() => setOverlay('settings')}>
        Other ways to install it, in Settings → Requirements
      </button>
    </div>
  )
}
