import { useCallback, useEffect, useRef, useState } from 'react'

import { useLiveRefresh } from '@/lib/useLiveRefresh'

import { useBeacon } from '@/app/store'
import { EnvView } from '@/features/env/EnvView'
import type { FileContents } from '@/types/beacon'
import { CodeEditor } from './CodeEditor'
import { useEditor, type OpenFile } from './openFiles'
import styles from './EditorPane.module.css'

/** `.env`, `.env.local`, `.env.production` — but not `.environment`. */
export function isEnvFile(name: string): boolean {
  return name === '.env' || name.startsWith('.env.')
}

/**
 * The editor surface: a tab per open file, and whichever view suits the file.
 *
 * `.env` files get their own view rather than the code editor, because what you
 * almost always want from one is a single value on the clipboard.
 */
export function EditorPane({
  workspaceId,
  projectId,
}: {
  workspaceId: string
  projectId: string
}): React.ReactElement {
  const files = useEditor((s) => s.byProject[projectId])
  const activePath = useEditor((s) => s.active[projectId])
  const error = useEditor((s) => s.error)
  const activate = useEditor((s) => s.activate)
  const close = useEditor((s) => s.close)
  const edit = useEditor((s) => s.edit)
  const save = useEditor((s) => s.save)
  const overwrite = useEditor((s) => s.overwrite)
  const reload = useEditor((s) => s.reload)
  const dismissError = useEditor((s) => s.dismissError)
  const checkForChanges = useEditor((s) => s.checkForChanges)
  const theme = useBeacon((s) => s.resolvedTheme)

  /** The tab whose close was asked for while it still had unsaved changes. */
  const [closing, setClosing] = useState<string | null>(null)

  // Claude edits files while it works, and it runs in a terminal inside this
  // same window — so the window never loses focus and a focus-only check would
  // almost never fire. Polled while the window is in front, and again the
  // moment it comes back.
  useLiveRefresh(
    useCallback(() => {
      void checkForChanges(workspaceId, projectId)
    }, [checkForChanges, workspaceId, projectId]),
    2000,
  )

  const active = files?.find((file) => file.path === activePath) ?? files?.at(-1)

  const onChange = useCallback(
    (text: string) => {
      if (active) edit(projectId, active.path, text)
    },
    [active, edit, projectId],
  )

  const onSave = useCallback(
    (text: string) => {
      if (!active) return
      edit(projectId, active.path, text)
      void save(workspaceId, projectId, active.path)
    },
    [active, edit, projectId, save, workspaceId],
  )

  // The tab strip scrolls, and Quick Open can activate a file whose tab is off
  // the end of it — which looks like nothing happened.
  const tabs = useRef<HTMLDivElement>(null)
  useEffect(() => {
    tabs.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [activePath])

  // A tab that stops being dirty — saved from anywhere — no longer needs asking.
  const closingFile = files?.find((file) => file.path === closing)
  useEffect(() => {
    if (closing !== null && (!closingFile || closingFile.draft === closingFile.saved)) {
      setClosing(null)
    }
  }, [closing, closingFile])

  const requestClose = (file: OpenFile): void => {
    if (file.draft !== file.saved) {
      activate(projectId, file.path)
      setClosing(file.path)
      return
    }
    close(projectId, file.path)
  }

  if (!files || files.length === 0) {
    return (
      <div className={styles['pane']}>
        {error ? <ErrorBar message={error} onDismiss={dismissError} /> : null}
        <div className={styles['empty']}>Open a file from the tree, or press it in Quick Open.</div>
      </div>
    )
  }

  return (
    <div className={styles['pane']}>
      <div ref={tabs} className={styles['tabs']} role="tablist" aria-label="Open files">
        {files.map((file) => {
          const isDirty = file.draft !== file.saved
          return (
            <div
              key={file.path}
              className={styles['tab']}
              data-active={file.path === active?.path}
              data-changed={file.changedOnDisk === true || file.goneFromDisk === true}
            >
              <button
                type="button"
                role="tab"
                aria-selected={file.path === active?.path}
                className={styles['label']}
                title={file.path}
                onClick={() => activate(projectId, file.path)}
              >
                {file.name}
              </button>
              {isDirty ? <span className={styles['dot']} aria-hidden="true" /> : null}
              <button
                type="button"
                className={`${styles['close']} ${isDirty ? styles['closeWhenDirty'] : ''}`}
                aria-label={`Close ${file.name}`}
                onClick={() => requestClose(file)}
              >
                ✕
              </button>
            </div>
          )
        })}
      </div>

      <div className={styles['body']}>
        {closingFile ? (
          <div className={styles['conflict']}>
            <span className={styles['conflictText']}>
              {closingFile.name} has changes that are not on disk.
            </span>
            <button
              type="button"
              className={styles['conflictAction']}
              onClick={() => {
                void save(workspaceId, projectId, closingFile.path).then(() => {
                  if (!isStillUnsaved(projectId, closingFile.path)) {
                    close(projectId, closingFile.path)
                  }
                })
              }}
            >
              Save and close
            </button>
            <button
              type="button"
              className={styles['conflictAction']}
              onClick={() => {
                setClosing(null)
                close(projectId, closingFile.path)
              }}
            >
              Close without saving
            </button>
            <button
              type="button"
              className={styles['conflictAction']}
              onClick={() => setClosing(null)}
            >
              Keep editing
            </button>
          </div>
        ) : null}

        {active?.goneFromDisk ? (
          <div className={styles['conflict']}>
            <span className={styles['conflictText']}>
              {active.name} is no longer on disk. Saving writes it back.
            </span>
            <button
              type="button"
              className={styles['conflictAction']}
              onClick={() => void overwrite(workspaceId, projectId, active.path)}
            >
              Save it back
            </button>
            <button
              type="button"
              className={styles['conflictAction']}
              onClick={() => close(projectId, active.path)}
            >
              Close the tab
            </button>
          </div>
        ) : active?.changedOnDisk ? (
          <div className={styles['conflict']}>
            <span className={styles['conflictText']}>
              {active.name} changed on disk while you were editing it — probably Claude.
            </span>
            <button
              type="button"
              className={styles['conflictAction']}
              onClick={() => void reload(workspaceId, projectId, active.path)}
            >
              Take theirs
            </button>
            <button
              type="button"
              className={styles['conflictAction']}
              onClick={() => void overwrite(workspaceId, projectId, active.path)}
            >
              Keep mine
            </button>
          </div>
        ) : null}

        {error ? <ErrorBar message={error} onDismiss={dismissError} /> : null}

        {active ? (
          <ActiveView
            // The epoch is in the key so a reload rebuilds the editor with what
            // is now on disk: the initial text is read once, at mount. A save
            // deliberately does not bump it — rebuilding there would throw away
            // the undo history, the cursor and the scroll position on every
            // Cmd+S.
            key={`${active.path}:${active.epoch}`}
            theme={theme}
            workspaceId={workspaceId}
            projectId={projectId}
            path={active.path}
            name={active.name}
            contents={active.contents}
            draft={active.draft}
            onChange={onChange}
            onSave={onSave}
          />
        ) : null}
      </div>
    </div>
  )
}

/** Whether a file still has work that is not on disk. */
function isStillUnsaved(projectId: string, path: string): boolean {
  const file = useEditor.getState().byProject[projectId]?.find((open) => open.path === path)
  return file !== undefined && file.draft !== file.saved
}

/**
 * A bar rather than a panel-sized message: an error about one operation should
 * not take the file you were reading off the screen.
 */
function ErrorBar({
  message,
  onDismiss,
}: {
  message: string
  onDismiss: () => void
}): React.ReactElement {
  return (
    <div className={`${styles['conflict']} ${styles['errorBar']}`} role="alert">
      <span className={`${styles['conflictText']} ${styles['error']}`}>{message}</span>
      <button
        type="button"
        className={styles['conflictAction']}
        aria-label="Dismiss"
        onClick={onDismiss}
      >
        ✕
      </button>
    </div>
  )
}

function ActiveView({
  workspaceId,
  projectId,
  path,
  name,
  theme,
  contents,
  draft,
  onChange,
  onSave,
}: {
  workspaceId: string
  projectId: string
  path: string
  name: string
  theme: 'dark' | 'light'
  contents: FileContents
  draft: string
  onChange: (text: string) => void
  onSave: (text: string) => void
}): React.ReactElement {
  if (contents.kind === 'binary') {
    return <div className={styles['notice']}>{name} is not a text file.</div>
  }
  if (contents.kind === 'tooLarge') {
    return (
      <div className={styles['notice']}>
        {name} is {Math.round(contents.size / 1024 / 1024)} MB — too large to edit here.
      </div>
    )
  }

  if (isEnvFile(name)) {
    return <EnvView workspaceId={workspaceId} projectId={projectId} path={path} />
  }

  return (
    <CodeEditor
      path={path}
      theme={theme}
      // Seeded from the draft, not from what was read: the editor is rebuilt
      // whenever you come back to a tab, and starting from disk would silently
      // undo everything typed before you left it.
      initialText={draft}
      onChange={onChange}
      onSave={onSave}
    />
  )
}
