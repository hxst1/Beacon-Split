import { useState } from 'react'

import { AccentFrame } from '@/app/AccentFrame'
import { useBeacon } from '@/app/store'
import { ACCENT_PRESETS, applyAccent } from '@/lib/accent'
import styles from './Welcome.module.css'

/** First run: Beacon needs one workspace before it can show anything else. */
export function Welcome(): React.ReactElement {
  const createWorkspace = useBeacon((s) => s.createWorkspace)
  const [name, setName] = useState('')
  const [accent, setAccent] = useState<string>(ACCENT_PRESETS[0].value)

  const pickAccent = (value: string): void => {
    setAccent(value)
    // Preview it immediately — the accent is the point of the choice.
    applyAccent(value)
  }

  const submit = (): void => {
    if (name.trim()) void createWorkspace(name.trim(), accent)
  }

  return (
    <div className={styles['root']}>
      <AccentFrame />
      <div className={styles['card']}>
        <div className={styles['mark']} />
        <div className={styles['title']}>Beacon</div>
        <p className={styles['body']}>
          Workspaces group the projects you switch between. Each one gets a colour, so you know
          where you are before you read anything.
        </p>

        <div className={styles['field']}>
          <input
            className={styles['input']}
            placeholder="Personal"
            value={name}
            autoFocus
            spellCheck={false}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && submit()}
          />
          <div className={styles['swatches']}>
            {ACCENT_PRESETS.map((preset) => (
              <button
                key={preset.value}
                type="button"
                title={preset.name}
                className={styles['swatch']}
                style={{ background: preset.value }}
                data-selected={preset.value === accent}
                onClick={() => pickAccent(preset.value)}
              />
            ))}
          </div>
        </div>

        <button
          type="button"
          className={styles['action']}
          disabled={!name.trim()}
          onClick={submit}
        >
          Create workspace
        </button>
      </div>
    </div>
  )
}
