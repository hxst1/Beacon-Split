import { useCallback, useEffect, useRef, useState } from 'react'

import { errorMessage, ipc } from '@/ipc'
import { useLiveRefresh } from '@/lib/useLiveRefresh'
import type { FileState, GitEntry, GitStatus } from '@/types/beacon'
import styles from './GitPane.module.css'

/** The single letter git itself would print for a state. */
const CODE: Record<FileState, string> = {
  unmodified: ' ',
  modified: 'M',
  added: 'A',
  deleted: 'D',
  renamed: 'R',
  copied: 'C',
  typeChanged: 'T',
  untracked: '?',
  ignored: '!',
  conflicted: 'U',
}

interface Selection {
  path: string
  staged: boolean
  untracked: boolean
}

/**
 * Status, diff, stage and commit — the parts of git that belong beside the work
 * rather than in their own application.
 *
 * Anything more involved is what the terminal panel is for; Beacon is not
 * trying to be a git client.
 */
export function GitPane({
  workspaceId,
  projectId,
}: {
  workspaceId: string
  projectId: string
}): React.ReactElement {
  const [status, setStatus] = useState<GitStatus | null | undefined>(undefined)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const [selected, setSelected] = useState<Selection | null>(null)
  const [diff, setDiff] = useState<string | null>(null)

  // What the last poll saw, so an unchanged repository does not re-render the
  // panel every couple of seconds and take the diff's scroll position with it.
  const lastSeen = useRef<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      const next = await ipc.gitStatus(workspaceId, projectId)
      const fingerprint = JSON.stringify(next)
      if (fingerprint !== lastSeen.current) {
        lastSeen.current = fingerprint
        setStatus(next)
      }
      setError(null)
    } catch (err) {
      setError(errorMessage(err))
    }
  }, [workspaceId, projectId])

  useEffect(() => {
    setSelected(null)
    setDiff(null)
    lastSeen.current = null
    void refresh()
  }, [refresh])

  // `git status` is milliseconds on a normal repository, so a short poll while
  // the window is focused is cheaper than making the user think about it.
  useLiveRefresh(
    useCallback(() => {
      void refresh()
    }, [refresh]),
    2000,
  )

  // The diff follows the selection rather than being fetched with the status:
  // most of the time nobody is looking at one.
  useEffect(() => {
    if (!selected) {
      setDiff(null)
      return
    }
    let cancelled = false
    ipc
      .gitDiff(workspaceId, projectId, selected.path, selected.staged, selected.untracked)
      .then((text) => {
        if (!cancelled) setDiff(text)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err))
      })
    return () => {
      cancelled = true
    }
  }, [selected, workspaceId, projectId, status])

  const act = async (action: () => Promise<GitStatus | string>): Promise<void> => {
    setBusy(true)
    try {
      const result = await action()
      if (typeof result === 'string') {
        await refresh()
      } else {
        lastSeen.current = JSON.stringify(result)
        setStatus(result)
      }
      setError(null)
    } catch (err) {
      setError(errorMessage(err))
    } finally {
      setBusy(false)
    }
  }

  if (status === undefined) return <div className={styles['status']}>Reading…</div>
  if (status === null) {
    return <div className={styles['status']}>This project is not a git repository.</div>
  }

  const staged = status.entries.filter((entry) => isStaged(entry))
  const unstaged = status.entries.filter((entry) => isUnstaged(entry))

  return (
    <div className={styles['root']}>
      <div className={styles['branchBar']}>
        <span className={styles['branch']}>{status.branch ?? 'detached'}</span>
        {status.ahead > 0 || status.behind > 0 ? (
          <span className={styles['tracking']}>
            {status.ahead > 0 ? `↑${status.ahead}` : ''}
            {status.behind > 0 ? `↓${status.behind}` : ''}
          </span>
        ) : null}
        <span className={styles['spacer']} />
        <button
          type="button"
          className={styles['action']}
          title="Refresh"
          aria-label="Refresh"
          onClick={() => void refresh()}
        >
          ↻
        </button>
        <button
          type="button"
          className={styles['action']}
          disabled={busy}
          onClick={() => void act(() => ipc.gitPull(workspaceId, projectId))}
        >
          Pull
        </button>
        <button
          type="button"
          className={styles['action']}
          disabled={busy}
          onClick={() => void act(() => ipc.gitPush(workspaceId, projectId))}
        >
          Push
        </button>
      </div>

      <div className={styles['list']}>
        {error ? <div className={`${styles['status']} ${styles['error']}`}>{error}</div> : null}

        {status.entries.length === 0 ? (
          <div className={styles['status']}>
            {status.unborn ? 'Nothing committed yet.' : 'No changes.'}
          </div>
        ) : null}

        {staged.length > 0 ? (
          <>
            <div className={styles['section']}>Staged</div>
            {staged.map((entry) => (
              <Row
                key={`staged:${entry.path}`}
                entry={entry}
                state={entry.staged}
                selected={selected?.path === entry.path && selected.staged}
                actionLabel="−"
                actionTitle="Unstage"
                onSelect={() => setSelected({ path: entry.path, staged: true, untracked: false })}
                onAction={() => void act(() => ipc.gitUnstage(workspaceId, projectId, entry.path))}
              />
            ))}
          </>
        ) : null}

        {unstaged.length > 0 ? (
          <>
            <div className={styles['section']}>
              Changes
              <span className={styles['spacer']} />
              <button
                type="button"
                className={styles['action']}
                disabled={busy}
                onClick={() => void act(() => ipc.gitStageAll(workspaceId, projectId))}
              >
                Stage all
              </button>
            </div>
            {unstaged.map((entry) => (
              <Row
                key={`unstaged:${entry.path}`}
                entry={entry}
                state={entry.unstaged}
                selected={selected?.path === entry.path && !selected.staged}
                actionLabel="+"
                actionTitle="Stage"
                onSelect={() =>
                  setSelected({
                    path: entry.path,
                    staged: false,
                    untracked: entry.unstaged === 'untracked',
                  })
                }
                onAction={() => void act(() => ipc.gitStage(workspaceId, projectId, entry.path))}
              />
            ))}
          </>
        ) : null}
      </div>

      {diff !== null ? <Diff text={diff} /> : null}

      <div className={styles['commit']}>
        <input
          className={styles['message']}
          placeholder={staged.length > 0 ? 'Commit message' : 'Stage something first'}
          value={message}
          spellCheck={false}
          onChange={(event) => setMessage(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter' || !message.trim() || staged.length === 0) return
            void act(() => ipc.gitCommit(workspaceId, projectId, message)).then(() => setMessage(''))
          }}
        />
        <button
          type="button"
          className={styles['commitButton']}
          disabled={busy || !message.trim() || staged.length === 0}
          onClick={() => {
            void act(() => ipc.gitCommit(workspaceId, projectId, message)).then(() => setMessage(''))
          }}
        >
          Commit
        </button>
      </div>
    </div>
  )
}

function Row({
  entry,
  state,
  selected,
  actionLabel,
  actionTitle,
  onSelect,
  onAction,
}: {
  entry: GitEntry
  state: FileState
  selected: boolean
  actionLabel: string
  actionTitle: string
  onSelect: () => void
  onAction: () => void
}): React.ReactElement {
  const label = entry.originalPath ? `${entry.originalPath} → ${entry.path}` : entry.path

  return (
    <div className={styles['row']} data-selected={selected}>
      <span className={styles['code']} data-state={state} title={state}>
        {CODE[state]}
      </span>
      <button type="button" className={styles['path']} title={label} onClick={onSelect}>
        {label}
      </button>
      <button
        type="button"
        className={styles['stage']}
        title={actionTitle}
        aria-label={`${actionTitle} ${entry.path}`}
        onClick={onAction}
      >
        {actionLabel}
      </button>
    </div>
  )
}

/**
 * A unified diff, coloured by line.
 *
 * Deliberately not a side-by-side view with intra-line highlighting: this is
 * for checking what you are about to commit, and `git diff` in the terminal is
 * two keystrokes away when it is not enough.
 */
function Diff({ text }: { text: string }): React.ReactElement {
  if (!text.trim()) {
    return <div className={styles['diff']}>
      <div className={`${styles['diffLine']} ${styles['meta']}`}>No changes to show.</div>
    </div>
  }

  return (
    <div className={styles['diff']}>
      {text.split('\n').map((line, index) => (
        <div key={index} className={`${styles['diffLine']} ${styles[classOf(line)] ?? ''}`}>
          {line || ' '}
        </div>
      ))}
    </div>
  )
}

function classOf(line: string): string {
  if (line.startsWith('@@')) return 'hunk'
  // `+++` and `---` are file headers, not content.
  if (line.startsWith('+++') || line.startsWith('---')) return 'meta'
  if (line.startsWith('+')) return 'added'
  if (line.startsWith('-')) return 'removed'
  if (line.startsWith('diff ') || line.startsWith('index ')) return 'meta'
  return ''
}

const isStaged = (entry: GitEntry): boolean =>
  entry.staged !== 'unmodified' && entry.staged !== 'untracked'

const isUnstaged = (entry: GitEntry): boolean => entry.unstaged !== 'unmodified'
