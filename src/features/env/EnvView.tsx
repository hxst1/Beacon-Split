import { useEffect, useRef, useState } from 'react'

import { isDirty, useEditor } from '@/features/editor/openFiles'
import { errorMessage, ipc } from '@/ipc'
import type { EnvEntry } from '@/types/beacon'
import { formatEnvValue, isEnvKey, readEnvLine, readEnvLines, setEnvValue } from './envText'
import styles from './EnvView.module.css'

/** What the form at the bottom, or the one standing in for a row, is doing. */
type Form = { kind: 'add' } | { kind: 'edit'; key: string; line: number }

/** One variable to set, and — for an edit — which line it is on. */
interface Change {
  key: string
  value: string
  line?: number | undefined
}

/**
 * A `.env` file, for the things people actually do with one: find a variable
 * and put its value on the clipboard, or add one.
 *
 * Values are masked until asked for, one at a time. Nothing here is persisted,
 * logged, or sent anywhere — the entries live in this component's state for as
 * long as it is mounted, are read fresh from the file each time, and the file
 * stays the only place they exist.
 *
 * Changes go through the open tab rather than straight to disk, so they get the
 * same guard a save from the code editor does: a file Claude changed since it
 * was read is not quietly written over.
 */
export function EnvView({
  workspaceId,
  projectId,
  path,
  onShowAsText,
}: {
  workspaceId: string
  projectId: string
  path: string
  onShowAsText: () => void
}): React.ReactElement {
  const [entries, setEntries] = useState<EnvEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [revealed, setRevealed] = useState<Set<string>>(new Set())
  const [copied, setCopied] = useState<string | null>(null)
  const [form, setForm] = useState<Form | null>(null)

  const edit = useEditor((s) => s.edit)
  const save = useEditor((s) => s.save)
  // Read again whenever the file is written, from here or anywhere else. The
  // epoch as well as the revision, for a file Beacon could not stat — its
  // revision never changes, but a reload still bumps the epoch.
  const revision = useEditor((s) => s.byProject[projectId]?.find((f) => f.path === path)?.revision)
  const epoch = useEditor((s) => s.byProject[projectId]?.find((f) => f.path === path)?.epoch)
  // The tab holds text that is not on disk — typed as text, or a change from
  // here that a save refused. This list shows the disk, so it would not show
  // that, and a change made here would save it along with its own.
  const unsaved = useEditor((s) => {
    const file = s.byProject[projectId]?.find((f) => f.path === path)
    return file !== undefined && file.draft !== file.saved
  })

  useEffect(() => {
    let cancelled = false
    ipc
      .readEnvFile(workspaceId, projectId, path)
      .then((found) => {
        if (cancelled) return
        setEntries(found)
        setError(null)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err))
      })

    return () => {
      cancelled = true
    }
  }, [workspaceId, projectId, path, revision, epoch])

  useEffect(
    () => () => {
      // Drop the values with the view rather than leaving them in memory
      // behind a hidden panel.
      setEntries(null)
      setRevealed(new Set())
    },
    [workspaceId, projectId, path],
  )

  // The variable being edited, followed by key when the file changes under the
  // form: the line it was on may now hold something else, or nothing.
  const editing =
    form?.kind === 'edit' && entries
      ? (entries.find((entry) => entry.line === form.line && entry.key === form.key) ??
        onlyOne(entries.filter((entry) => entry.key === form.key)))
      : undefined
  const lostTrack = form?.kind === 'edit' && entries !== null && editing === undefined
  useEffect(() => {
    if (lostTrack) setForm(null)
  }, [lostTrack])

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

  /** Sets variables in the file and saves it, as one write. */
  const write = async (changes: Change[]): Promise<void> => {
    const file = useEditor.getState().byProject[projectId]?.find((f) => f.path === path)
    if (!file || file.draft !== file.saved) return
    const text = changes.reduce(
      (current, change) => setEnvValue(current, change.key, change.value, change.line),
      file.draft,
    )
    edit(projectId, path, text)
    await save(workspaceId, projectId, path)
    // A refused save leaves the change in the tab, and the pane says why and
    // offers to write it anyway — so the form has nothing left to do.
    setForm(null)
    // What was just written is what somebody wants to check.
    if (!isDirty(projectId, path)) {
      setRevealed((current) => new Set([...current, ...changes.map((change) => change.key)]))
    }
  }

  if (error) return <div className={styles['empty']}>{error}</div>
  if (!entries) return <div className={styles['empty']}>Reading…</div>

  const allRevealed = entries.length > 0 && revealed.size === entries.length
  const keys = entries.map((entry) => entry.key)

  return (
    <div className={styles['root']}>
      <div className={styles['header']}>
        <span className={styles['count']}>
          {entries.length} {entries.length === 1 ? 'variable' : 'variables'} · {path}
        </span>
        <button
          type="button"
          className={styles['toggleAll']}
          disabled={unsaved}
          onClick={() => setForm({ kind: 'add' })}
        >
          Add variable
        </button>
        {entries.length > 0 ? (
          <button
            type="button"
            className={styles['toggleAll']}
            onClick={() => setRevealed(allRevealed ? new Set() : new Set(keys))}
          >
            {allRevealed ? 'Hide all' : 'Show all'}
          </button>
        ) : null}
        <button type="button" className={styles['toggleAll']} onClick={onShowAsText}>
          Edit as text
        </button>
      </div>

      {unsaved ? (
        <div className={styles['note']}>
          <span>
            The tab has changes that are not on disk, and this list shows the disk. Save or undo
            them as text before changing anything here.
          </span>
          <button type="button" className={styles['noteAction']} onClick={onShowAsText}>
            Edit as text
          </button>
        </div>
      ) : null}

      {entries.length === 0 && form === null ? (
        <div className={styles['none']}>No variables in {path} yet.</div>
      ) : null}

      {entries.map((entry) => {
        if (entry === editing && !unsaved) {
          return (
            <EntryForm
              // Not the row's key: the form outlives its line moving.
              key="editing"
              fixedKey={entry.key}
              initialValue={entry.value}
              existing={keys}
              onSubmit={(changes) =>
                write(changes.map((change) => ({ ...change, line: entry.line })))
              }
              onCancel={() => setForm(null)}
            />
          )
        }

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
              {unsaved ? null : (
                <button
                  type="button"
                  className={styles['action']}
                  onClick={() => setForm({ kind: 'edit', key: entry.key, line: entry.line })}
                >
                  Edit
                </button>
              )}
            </span>
          </div>
        )
      })}

      {form?.kind === 'add' && !unsaved ? (
        <EntryForm
          initialValue=""
          existing={keys}
          onSubmit={write}
          onCancel={() => setForm(null)}
        />
      ) : null}
    </div>
  )
}

function onlyOne<T>(items: T[]): T | undefined {
  return items.length === 1 ? items[0] : undefined
}

/**
 * A row's worth of form: a key and a value, for a new variable or — with the
 * key fixed — a new value for one that is already there.
 *
 * Variables usually arrive as `KEY=value` lines, from a dashboard or a
 * teammate, so that is what the key takes: typing `=` moves on to the value,
 * pasting a line fills in both, and pasting several offers to add them all.
 *
 * The value is a text area, one line tall until it needs more: a private key
 * is several lines, and a single-line field would quietly join them.
 */
function EntryForm({
  fixedKey,
  initialValue,
  existing,
  onSubmit,
  onCancel,
}: {
  fixedKey?: string
  initialValue: string
  existing: string[]
  onSubmit: (changes: Change[]) => Promise<void>
  onCancel: () => void
}): React.ReactElement {
  const [key, setKey] = useState(fixedKey ?? '')
  const [value, setValue] = useState(initialValue)
  const [pasted, setPasted] = useState<{ key: string; value: string }[] | null>(null)
  const [busy, setBusy] = useState(false)
  const [tried, setTried] = useState(false)
  const [failed, setFailed] = useState<string | null>(null)
  const root = useRef<HTMLFormElement>(null)
  const keyInput = useRef<HTMLInputElement>(null)
  const valueInput = useRef<HTMLTextAreaElement>(null)
  const addAll = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    root.current?.scrollIntoView({ block: 'nearest' })
    if (fixedKey === undefined) keyInput.current?.focus()
    else valueInput.current?.select()
  }, [fixedKey])

  // Focused so Enter adds them and Escape goes back, as in the form.
  const showingPasted = pasted !== null
  useEffect(() => {
    if (showingPasted) addAll.current?.focus()
    else keyInput.current?.focus()
  }, [showingPasted])

  const trimmed = key.trim()
  const problem =
    trimmed === ''
      ? 'Give it a name.'
      : !isEnvKey(trimmed)
        ? 'Letters, digits, _ and . only.'
        : formatEnvValue(value) === null
          ? 'This value cannot be written so it reads back the same. Edit the file as text.'
          : null
  const replacing = fixedKey === undefined && existing.includes(trimmed)

  const takeLine = (text: string): void => {
    const line = readEnvLine(text)
    const equals = text.indexOf('=')
    setKey(line?.key ?? text.slice(0, equals).trim())
    // `KEY=` typed into a form that already has a value is not asking to clear it.
    if (line && text.slice(equals + 1).trim() !== '') setValue(line.value)
    valueInput.current?.focus()
  }

  const submit = (changes: Change[]): void => {
    if (busy) return
    setBusy(true)
    setFailed(null)
    onSubmit(changes)
      .catch((err: unknown) => setFailed(errorMessage(err)))
      .finally(() => setBusy(false))
  }

  // Values in a paste that could not be written back as they are. Named rather
  // than dropped, so nobody finds out later that one is missing.
  const unwritable = (pasted ?? []).filter((line) => formatEnvValue(line.value) === null)

  const cancelOnEscape = (event: React.KeyboardEvent): void => {
    if (event.key === 'Escape' && !event.nativeEvent.isComposing) {
      event.stopPropagation()
      if (pasted) setPasted(null)
      else onCancel()
    }
  }

  if (pasted) {
    return (
      <form
        ref={root}
        className={`${styles['row']} ${styles['form']}`}
        onSubmit={(event) => {
          event.preventDefault()
          if (unwritable.length === 0) submit(pasted)
        }}
        onKeyDown={cancelOnEscape}
      >
        <span className={styles['pasted']}>
          {pasted.length} variables pasted:{' '}
          {pasted.map((line, index) => (
            <span key={`${line.key}:${index}`}>
              {index > 0 ? ', ' : null}
              <span className={styles['pastedKey']}>{line.key}</span>
              {existing.includes(line.key) ? ' (replaces)' : null}
            </span>
          ))}
        </span>
        <span className={styles['formActions']}>
          <button
            ref={addAll}
            type="submit"
            className={styles['primary']}
            disabled={busy || unwritable.length > 0}
          >
            Add all
          </button>
          <button type="button" className={styles['action']} onClick={() => setPasted(null)}>
            Back
          </button>
        </span>
        {unwritable.length > 0 ? (
          <span className={`${styles['hint']} ${styles['problem']}`}>
            {unwritable.map((line) => line.key).join(', ')} cannot be written so{' '}
            {unwritable.length === 1 ? 'it reads' : 'they read'} back the same. Add them as text.
          </span>
        ) : failed ? (
          <span className={`${styles['hint']} ${styles['problem']}`}>{failed}</span>
        ) : null}
      </form>
    )
  }

  return (
    <form
      ref={root}
      className={`${styles['row']} ${styles['form']}`}
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (problem === null) submit([{ key: trimmed, value }])
      }}
      onKeyDown={cancelOnEscape}
    >
      {fixedKey === undefined ? (
        <input
          ref={keyInput}
          className={styles['input']}
          value={key}
          placeholder="NAME"
          aria-label="Variable name"
          aria-invalid={tried && problem !== null}
          autoComplete="off"
          autoCapitalize="off"
          spellCheck={false}
          onChange={(event) => {
            const text = event.target.value
            if (text.includes('=')) takeLine(text)
            else setKey(text)
          }}
          onPaste={(event) => {
            // Taken over only when everything pasted reads as `.env` lines.
            // Anything else — a PEM body, whose base64 can end in `=` — is
            // left to paste the ordinary way.
            const text = event.clipboardData.getData('text')
            const lines = readEnvLines(text)
            const meaningful = text
              .split('\n')
              .filter((raw) => raw.trim() !== '' && !raw.trim().startsWith('#'))
            if (lines.length === 0 || lines.length !== meaningful.length) return
            event.preventDefault()
            const [only] = lines
            if (lines.length > 1 || !only) {
              setPasted(lines)
              return
            }
            setKey(only.key)
            setValue(only.value)
            valueInput.current?.focus()
          }}
        />
      ) : (
        <span className={styles['key']} title={fixedKey}>
          {fixedKey}
        </span>
      )}

      <textarea
        ref={valueInput}
        className={`${styles['input']} ${styles['valueInput']}`}
        value={value}
        rows={Math.min(value.split('\n').length, 8)}
        placeholder="value"
        aria-label={`Value of ${trimmed || 'the variable'}`}
        autoComplete="off"
        autoCapitalize="off"
        spellCheck={false}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          // Enter saves, as in a single-line field; Shift+Enter is for the
          // values that really are several lines.
          if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
            event.preventDefault()
            event.currentTarget.form?.requestSubmit()
          }
        }}
      />

      <span className={styles['formActions']}>
        <button type="submit" className={styles['primary']} disabled={busy}>
          {fixedKey !== undefined ? 'Save' : replacing ? 'Replace' : 'Add'}
        </button>
        <button type="button" className={styles['action']} onClick={onCancel}>
          Cancel
        </button>
      </span>

      {tried && problem ? (
        <span className={`${styles['hint']} ${styles['problem']}`}>{problem}</span>
      ) : failed ? (
        <span className={`${styles['hint']} ${styles['problem']}`}>{failed}</span>
      ) : replacing ? (
        <span className={styles['hint']}>{trimmed} is already set — this replaces its value.</span>
      ) : null}
    </form>
  )
}
