import { useEffect, useState } from 'react'

import { errorMessage, ipc } from '@/ipc'
import type { EnvEntry } from '@/types/beacon'
import styles from './EnvView.module.css'

/**
 * A `.env` file, for the thing people actually do with one: find a variable and
 * put its value on the clipboard.
 *
 * Values are masked until asked for, one at a time. Nothing here is persisted,
 * logged, or sent anywhere — the entries live in this component's state for as
 * long as it is mounted, are read fresh from the file each time, and the file
 * stays the only place they exist.
 */
export function EnvView({
  workspaceId,
  projectId,
  path,
}: {
  workspaceId: string
  projectId: string
  path: string
}): React.ReactElement {
  const [entries, setEntries] = useState<EnvEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [revealed, setRevealed] = useState<Set<string>>(new Set())
  const [copied, setCopied] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    ipc
      .readEnvFile(workspaceId, projectId, path)
      .then((found) => {
        if (!cancelled) setEntries(found)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err))
      })

    return () => {
      cancelled = true
      // Drop the values with the view rather than leaving them in memory
      // behind a hidden panel.
      setEntries(null)
      setRevealed(new Set())
    }
  }, [workspaceId, projectId, path])

  const toggle = (key: string): void =>
    setRevealed((current) => {
      const next = new Set(current)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })

  const copy = (label: string, text: string): void => {
    void navigator.clipboard.writeText(text)
    setCopied(label)
    window.setTimeout(() => setCopied((current) => (current === label ? null : current)), 1200)
  }

  if (error) return <div className={styles['empty']}>{error}</div>
  if (!entries) return <div className={styles['empty']}>Reading…</div>
  if (entries.length === 0) return <div className={styles['empty']}>No variables in {path}.</div>

  const allRevealed = revealed.size === entries.length

  return (
    <div className={styles['root']}>
      <div className={styles['header']}>
        <span className={styles['count']}>
          {entries.length} {entries.length === 1 ? 'variable' : 'variables'} · {path}
        </span>
        <button
          type="button"
          className={styles['toggleAll']}
          onClick={() =>
            setRevealed(allRevealed ? new Set() : new Set(entries.map((entry) => entry.key)))
          }
        >
          {allRevealed ? 'Hide all' : 'Show all'}
        </button>
      </div>

      {entries.map((entry) => {
        const isRevealed = revealed.has(entry.key)
        return (
          <div className={styles['row']} key={`${entry.key}:${entry.line}`}>
            <span className={styles['key']} title={entry.key}>
              {entry.key}
            </span>

            <span
              className={`${styles['value']} ${isRevealed ? styles['revealed'] : styles['masked']}`}
              title={isRevealed ? entry.value : undefined}
            >
              {isRevealed ? entry.value || '(empty)' : '•'.repeat(Math.min(entry.value.length || 6, 18))}
            </span>

            <span className={styles['actions']}>
              <button type="button" className={styles['action']} onClick={() => toggle(entry.key)}>
                {isRevealed ? 'Hide' : 'Show'}
              </button>
              <button
                type="button"
                className={`${styles['action']} ${copied === entry.key ? styles['copied'] : ''}`}
                onClick={() => copy(entry.key, entry.value)}
              >
                {copied === entry.key ? 'Copied' : 'Copy'}
              </button>
              <button
                type="button"
                className={`${styles['action']} ${
                  copied === `${entry.key}=` ? styles['copied'] : ''
                }`}
                onClick={() => copy(`${entry.key}=`, `${entry.key}=${entry.value}`)}
              >
                {copied === `${entry.key}=` ? 'Copied' : 'Copy KEY=value'}
              </button>
            </span>
          </div>
        )
      })}
    </div>
  )
}
