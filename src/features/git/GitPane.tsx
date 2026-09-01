import { useCallback, useEffect, useReducer, useRef, useState } from 'react'

import { MissingTool } from '@/features/settings/MissingTool'
import { useBeacon } from '@/app/store'
import { useClips } from '@/features/clips/clips'
import { errorMessage, ipc } from '@/ipc'
import { useLiveRefresh } from '@/lib/useLiveRefresh'
import type { FileState, GitEntry, GitStatus } from '@/types/beacon'
import { noNotices, reduceNotices, type Notices } from './notices'
import { remoteActions } from './remote'
import { createRequestSequence } from './requestSequence'
import {
  conflictSelection,
  isConflicted,
  isStaged,
  isUnstaged,
  reconcileGitSelection,
  selectionKey,
  type GitSelection,
} from './selection'
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

type DiffState =
  | { identity: string; status: 'loading' }
  | { identity: string; status: 'ready'; text: string }
  | { identity: string; status: 'error'; message: string }

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
  const missingGit = useBeacon((s) => s.missing.find((entry) => entry.id === 'git'))
  const overlay = useBeacon((s) => s.overlay)
  const clipDrawerOpen = useClips((s) => s.open)
  const [{ status, selected, diffRevision }, setView] = useState<{
    status: GitStatus | null | undefined
    selected: GitSelection | null
    diffRevision: number
  }>({ status: undefined, selected: null, diffRevision: 0 })
  const [notices, notify] = useReducer(reduceNotices, noNotices)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const [diffState, setDiffState] = useState<DiffState | null>(null)

  const busyRef = useRef(false)
  const backRef = useRef<HTMLButtonElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const rowButtons = useRef(new Map<string, HTMLButtonElement>())
  const restoreFocusKey = useRef<string | null>(null)
  const previousSelection = useRef<GitSelection | null>(null)
  const gitRequests = useRef(createRequestSequence())
  const diffRequests = useRef(createRequestSequence())

  // Which diff is being fetched right now, so a poll does not restart one that
  // is still running. Any `git diff` slower than the poll interval would
  // otherwise be superseded by its own successor every time and never arrive.
  const diffInFlight = useRef<string | null>(null)

  // Preserve the semantic identity of an unchanged status. A separate revision
  // still lets a successful poll refresh an open diff whose content changed.
  const lastSeen = useRef<string | null>(null)

  const applyStatus = useCallback((next: GitStatus | null) => {
    const fingerprint = JSON.stringify(next)
    const statusChanged = fingerprint !== lastSeen.current
    lastSeen.current = fingerprint

    setView((current) => {
      const reconciled = reconcileGitSelection(current.selected, next)
      return {
        status: statusChanged ? next : current.status,
        selected: reconciled,
        diffRevision: current.diffRevision + 1,
      }
    })
  }, [])

  const refresh = useCallback(async (duringAction = false): Promise<boolean> => {
    if (busyRef.current && !duringAction) return false
    const request = gitRequests.current.begin()

    try {
      const next = await ipc.gitStatus(workspaceId, projectId)
      if (!gitRequests.current.isCurrent(request)) return false
      applyStatus(next)
      notify({ type: 'pollSucceeded' })
      return true
    } catch (err) {
      if (!gitRequests.current.isCurrent(request)) return false
      notify({ type: 'pollFailed', text: errorMessage(err) })
      return false
    }
  }, [applyStatus, workspaceId, projectId])

  useEffect(() => {
    gitRequests.current.invalidate()
    diffRequests.current.invalidate()
    diffInFlight.current = null
    setView((current) => ({ ...current, selected: null }))
    setDiffState(null)
    restoreFocusKey.current = null
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

  const diffIdentity = selected
    ? JSON.stringify([workspaceId, projectId, selected, status])
    : null

  // The response carries the identity of the selection and status that
  // requested it. This prevents the previous file's text from being rendered
  // in the frame before this effect starts the next request.
  useEffect(() => {
    if (!selected || !diffIdentity) {
      diffRequests.current.invalidate()
      diffInFlight.current = null
      setDiffState(null)
      return
    }

    setDiffState((current) =>
      current?.identity === diffIdentity && current.status === 'ready'
        ? current
        : { identity: diffIdentity, status: 'loading' },
    )

    // A poll every two seconds asks for this diff again so that editing the
    // open file is visible without clicking anything. Restarting a request
    // that has not answered yet would mean a slow diff never answers at all,
    // so the one already running is left to finish and its result stands until
    // the next poll after it.
    if (diffInFlight.current === diffIdentity) return

    diffInFlight.current = diffIdentity
    const request = diffRequests.current.begin()
    const settle = (next: DiffState): void => {
      if (!diffRequests.current.isCurrent(request)) return
      diffInFlight.current = null
      setDiffState(next)
    }

    ipc
      .gitDiff(workspaceId, projectId, selected.path, selected.staged, selected.untracked)
      .then((text) => settle({ identity: diffIdentity, status: 'ready', text }))
      .catch((err: unknown) =>
        settle({ identity: diffIdentity, status: 'error', message: errorMessage(err) }),
      )
  }, [diffIdentity, diffRevision, selected, workspaceId, projectId])

  useEffect(() => {
    const previous = previousSelection.current
    previousSelection.current = selected

    if (selected) {
      restoreFocusKey.current = selectionKey(selected)
      if (!previous) backRef.current?.focus()
      return
    }

    const key = restoreFocusKey.current
    if (!key) return

    ;(rowButtons.current.get(key) ?? listRef.current)?.focus()
    restoreFocusKey.current = null
  }, [selected])

  const openDiff = (selection: GitSelection, rowKey: string): void => {
    restoreFocusKey.current = rowKey
    setView((current) => ({ ...current, selected: selection }))
  }

  const closeDiff = (): void => {
    setView((current) => ({ ...current, selected: null }))
    setDiffState(null)
  }

  /**
   * Runs one thing the user asked for, and reports how it went.
   *
   * Push and pull answer with what git said rather than with a status, and
   * that answer is the only evidence the click did anything — a push that
   * worked and a push that was never sent look identical otherwise.
   */
  const act = async (action: () => Promise<GitStatus | string>): Promise<boolean> => {
    if (busyRef.current) return false
    busyRef.current = true
    gitRequests.current.invalidate()
    const mutation = gitRequests.current.begin()
    setBusy(true)
    notify({ type: 'actionStarted' })
    try {
      const result = await action()
      if (!gitRequests.current.isCurrent(mutation)) return false
      if (typeof result === 'string') {
        notify({ type: 'actionSucceeded', text: result })
        if (!(await refresh(true))) return false
      } else {
        applyStatus(result)
        notify({ type: 'actionSucceeded' })
      }
      return true
    } catch (err) {
      if (!gitRequests.current.isCurrent(mutation)) return false
      notify({ type: 'actionFailed', text: errorMessage(err) })
      return false
    } finally {
      busyRef.current = false
      setBusy(false)
    }
  }

  if (missingGit) return <MissingTool requirement={missingGit} />

  // A first read that failed leaves nothing to draw the panel around, and a
  // repository can fail every read there is: dubious ownership, a project
  // moved out from under Beacon, a `.git` that is no longer one. Without the
  // reason and a way to ask again, the panel is inert and silent while it
  // keeps polling something that will not answer.
  if (status === undefined) {
    return (
      <div className={styles['root']}>
        <div className={styles['branchBar']}>
          <span className={styles['branch']}>Git</span>
          <span className={styles['spacer']} />
          <button
            type="button"
            className={styles['action']}
            disabled={busy}
            onClick={() => void refresh()}
          >
            Try again
          </button>
        </div>
        {notices.poll ? (
          <div className={`${styles['status']} ${styles['error']}`}>{notices.poll}</div>
        ) : (
          <div className={styles['status']}>Reading…</div>
        )}
      </div>
    )
  }

  if (status === null) {
    return <div className={styles['status']}>This project is not a git repository.</div>
  }

  const conflicts = status.entries.filter((entry) => isConflicted(entry))
  const staged = status.entries.filter((entry) => isStaged(entry))
  const unstaged = status.entries.filter((entry) => isUnstaged(entry))
  const remote = remoteActions(status)

  // Git refuses to commit with unmerged paths anyway; saying so before the
  // click is friendlier than relaying `error: Committing is not possible`.
  const canCommit = staged.length > 0 && conflicts.length === 0

  const commit = async (): Promise<void> => {
    if (busyRef.current || !message.trim() || !canCommit) return
    const submittedMessage = message
    const succeeded = await act(() => ipc.gitCommit(workspaceId, projectId, submittedMessage))
    if (succeeded) {
      setMessage((current) => (current === submittedMessage ? '' : current))
    }
  }

  return (
    <div
      className={styles['root']}
      onKeyDown={(event) => {
        if (
          !selected ||
          overlay ||
          clipDrawerOpen ||
          event.key !== 'Escape' ||
          event.defaultPrevented
        ) {
          return
        }
        event.preventDefault()
        event.stopPropagation()
        closeDiff()
      }}
    >
      <div className={styles['branchBar']}>
        <span className={styles['branch']}>{status.branch ?? 'detached'}</span>
        {status.ahead > 0 || status.behind > 0 ? (
          <span className={styles['tracking']}>
            {status.ahead > 0 ? `↑${status.ahead}` : ''}
            {status.behind > 0 ? `↓${status.behind}` : ''}
          </span>
        ) : null}
        {/* Said out loud rather than left to a tooltip on a disabled button,
            which is where an explanation goes to be never read. */}
        {status.branch && !status.upstream ? (
          <span className={styles['tracking']} title={remote.reason ?? undefined}>
            no upstream
          </span>
        ) : null}
        <span className={styles['spacer']} />
        <button
          type="button"
          className={styles['action']}
          title="Refresh"
          aria-label="Refresh"
          disabled={busy}
          onClick={() => void refresh()}
        >
          ↻
        </button>
        <button
          type="button"
          className={styles['action']}
          title={remote.reason ?? 'Pull, fast-forward only'}
          disabled={busy || !remote.canPull}
          onClick={() => void act(() => ipc.gitPull(workspaceId, projectId))}
        >
          Pull
        </button>
        <button
          type="button"
          className={styles['action']}
          title={remote.reason ?? 'Push this branch'}
          disabled={busy || !remote.canPush}
          onClick={() => void act(() => ipc.gitPush(workspaceId, projectId))}
        >
          Push
        </button>
      </div>

      {selected ? (
        <div className={styles['diffView']}>
          <div className={styles['diffHeader']}>
            <button
              ref={backRef}
              type="button"
              className={styles['back']}
              title="Back to changes (Esc)"
              onClick={closeDiff}
            >
              ← Changes
            </button>
            <span className={styles['diffPath']} title={selected.path}>
              {selected.path}
            </span>
            <span className={styles['diffBadge']}>
              {selected.state === 'conflicted'
                ? 'Conflict'
                : selected.staged
                  ? 'Staged'
                  : selected.untracked
                    ? 'Untracked'
                    : 'Working tree'}
            </span>
            {selected.state === 'conflicted' ? null : (
              <button
                type="button"
                className={styles['diffAction']}
                disabled={busy}
                onClick={() =>
                  void act(() =>
                    selected.staged
                      ? ipc.gitUnstage(workspaceId, projectId, selected.path)
                      : ipc.gitStage(workspaceId, projectId, selected.path),
                  )
                }
              >
                {selected.staged ? 'Unstage' : 'Stage'}
              </button>
            )}
          </div>
          <NoticeLines notices={notices} />
          {!diffState || diffState.identity !== diffIdentity || diffState.status === 'loading' ? (
            <div className={styles['status']}>Loading diff…</div>
          ) : diffState.status === 'error' ? (
            <div className={`${styles['status']} ${styles['error']}`}>{diffState.message}</div>
          ) : (
            <Diff text={diffState.text} />
          )}
        </div>
      ) : (
        <div ref={listRef} className={styles['list']} tabIndex={-1} aria-label="Git changes">
          <NoticeLines notices={notices} />

          {status.entries.length === 0 ? (
            <div className={styles['status']}>
              {status.unborn ? 'Nothing committed yet.' : 'No changes.'}
            </div>
          ) : null}

          {conflicts.length > 0 ? (
            <>
              <div className={styles['section']}>Conflicts</div>
              {/* Beacon does not resolve conflicts, and a stage or unstage
                  button here would be a one-click way to tell git a file with
                  `<<<<<<<` still in it is finished. */}
              <div className={styles['hint']}>
                Resolve these where you can see both sides, then mark them resolved.
              </div>
              {conflicts.map((entry) => (
                <Row
                  key={`conflicted:${entry.path}`}
                  entry={entry}
                  state="conflicted"
                  buttonRef={(node) =>
                    registerRowButton(rowButtons.current, `conflicted:${entry.path}`, node)
                  }
                  onSelect={() =>
                    openDiff(conflictSelection(entry), `conflicted:${entry.path}`)
                  }
                  action={{
                    label: '✓',
                    title: 'Mark resolved',
                    onAction: () =>
                      void act(() => ipc.gitStage(workspaceId, projectId, entry.path)),
                  }}
                />
              ))}
            </>
          ) : null}

          {staged.length > 0 ? (
            <>
              <div className={styles['section']}>Staged</div>
              {staged.map((entry) => (
                <Row
                  key={`staged:${entry.path}`}
                  entry={entry}
                  state={entry.staged}
                  buttonRef={(node) => registerRowButton(rowButtons.current, `staged:${entry.path}`, node)}
                  onSelect={() =>
                    openDiff(
                      { path: entry.path, staged: true, untracked: false, state: entry.staged },
                      `staged:${entry.path}`,
                    )
                  }
                  action={{
                    label: '−',
                    title: 'Unstage',
                    onAction: () =>
                      void act(() => ipc.gitUnstage(workspaceId, projectId, entry.path)),
                  }}
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
                  buttonRef={(node) => registerRowButton(rowButtons.current, `unstaged:${entry.path}`, node)}
                  onSelect={() =>
                    openDiff(
                      {
                        path: entry.path,
                        staged: false,
                        untracked: entry.unstaged === 'untracked',
                        state: entry.unstaged,
                      },
                      `unstaged:${entry.path}`,
                    )
                  }
                  action={{
                    label: '+',
                    title: 'Stage',
                    onAction: () => void act(() => ipc.gitStage(workspaceId, projectId, entry.path)),
                  }}
                />
              ))}
            </>
          ) : null}
        </div>
      )}

      <div className={styles['commit']}>
        <input
          className={styles['message']}
          placeholder={
            conflicts.length > 0
              ? 'Resolve the conflicts first'
              : staged.length > 0
                ? 'Commit message'
                : 'Stage something first'
          }
          value={message}
          spellCheck={false}
          onChange={(event) => setMessage(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== 'Enter' || busyRef.current || !message.trim() || !canCommit) {
              return
            }
            void commit()
          }}
        />
        <button
          type="button"
          className={styles['commitButton']}
          disabled={busy || !message.trim() || !canCommit}
          onClick={() => void commit()}
        >
          Commit
        </button>
      </div>
    </div>
  )
}

/** What a row's own button does, when it has one. */
interface RowAction {
  label: string
  title: string
  onAction: () => void
}

function Row({
  entry,
  state,
  buttonRef,
  onSelect,
  action,
}: {
  entry: GitEntry
  state: FileState
  buttonRef: (node: HTMLButtonElement | null) => void
  onSelect: () => void
  action: RowAction
}): React.ReactElement {
  const label = entry.originalPath ? `${entry.originalPath} → ${entry.path}` : entry.path

  return (
    <div className={styles['row']}>
      <span className={styles['code']} data-state={state} title={state}>
        {CODE[state]}
      </span>
      <button ref={buttonRef} type="button" className={styles['path']} title={label} onClick={onSelect}>
        {label}
      </button>
      <button
        type="button"
        className={styles['stage']}
        title={action.title}
        aria-label={`${action.title} ${entry.path}`}
        onClick={action.onAction}
      >
        {action.label}
      </button>
    </div>
  )
}

/**
 * The panel's two kinds of news, each in its own line.
 *
 * Kept apart because they answer different questions: one is what happened
 * when you clicked, the other is why the panel is not keeping up.
 */
function NoticeLines({ notices }: { notices: Notices }): React.ReactElement | null {
  if (!notices.action && !notices.poll) return null

  return (
    <>
      {notices.action ? (
        <div
          className={
            notices.action.tone === 'error'
              ? `${styles['status']} ${styles['error']}`
              : styles['status']
          }
        >
          {notices.action.text}
        </div>
      ) : null}
      {notices.poll ? (
        <div className={`${styles['status']} ${styles['error']}`}>{notices.poll}</div>
      ) : null}
    </>
  )
}

function registerRowButton(
  buttons: Map<string, HTMLButtonElement>,
  key: string,
  node: HTMLButtonElement | null,
): void {
  if (node) buttons.set(key, node)
  else buttons.delete(key)
}

/**
 * A unified diff, coloured by line.
 *
 * Deliberately not a side-by-side view with intra-line highlighting: this is
 * for checking what you are about to commit, and `git diff` in the terminal is
 * two keystrokes away when it is not enough.
 */
function Diff({ text }: { text: string }): React.ReactElement {
  // A scroll container is only scrollable from the keyboard once it can hold
  // focus: without this, arrow keys and Page Down did nothing at all and a
  // long diff could only be read with a pointer.
  const scrollable = {
    className: styles['diff'],
    tabIndex: 0,
    role: 'region',
    'aria-label': 'Diff',
  }

  if (!text.trim()) {
    return (
      <div {...scrollable}>
        <div className={`${styles['diffLine']} ${styles['meta']}`}>No changes to show.</div>
      </div>
    )
  }

  return (
    <div {...scrollable}>
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
