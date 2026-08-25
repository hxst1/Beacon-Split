import { useCallback, useRef } from 'react'

import { useLiveRefresh } from '@/lib/useLiveRefresh'

import { useBeacon } from '@/app/store'
import { EnvView } from '@/features/env/EnvView'
import { CodeEditor } from './CodeEditor'
import { useEditor } from './openFiles'
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
  const dirty = useEditor((s) => s.dirty)
  const error = useEditor((s) => s.error)
  const activate = useEditor((s) => s.activate)
  const close = useEditor((s) => s.close)
  const markDirty = useEditor((s) => s.markDirty)
  const save = useEditor((s) => s.save)
  const overwrite = useEditor((s) => s.overwrite)
  const reload = useEditor((s) => s.reload)
  const checkForChanges = useEditor((s) => s.checkForChanges)
  const theme = useBeacon((s) => s.resolvedTheme)

  // The live buffer, so the tab's save button writes what is on screen.
  const buffer = useRef<string>('')

  // Claude edits files while they are open, so an open buffer can be stale
  // within seconds. Checked when the window comes back rather than on a timer:
  // the moment you look at it is the moment it matters.
  useLiveRefresh(
    useCallback(() => {
      void checkForChanges(workspaceId, projectId)
    }, [checkForChanges, workspaceId, projectId]),
    null,
  )

  const active = files?.find((file) => file.path === activePath) ?? files?.at(-1)

  const onChange = useCallback(
    (text: string) => {
      buffer.current = text
      if (active) markDirty(projectId, active.path, text !== active.saved)
    },
    [active, markDirty, projectId],
  )

  const onSave = useCallback(
    (text: string) => {
      if (active) void save(workspaceId, projectId, active.path, text)
    },
    [active, projectId, save, workspaceId],
  )

  if (!files || files.length === 0) {
    return (
      <div className={styles['empty']}>
        Open a file from the tree, or press it in Quick Open.
      </div>
    )
  }

  return (
    <div className={styles['pane']}>
      <div className={styles['tabs']}>
        {files.map((file) => {
          const isDirty = dirty[`${projectId}:${file.path}`] === true
          return (
            <button
              key={file.path}
              type="button"
              className={styles['tab']}
              data-active={file.path === active?.path}
              data-changed={file.changedOnDisk === true}
              title={file.path}
              onClick={() => activate(projectId, file.path)}
            >
              <span className={styles['label']}>{file.name}</span>
              {isDirty ? <span className={styles['dot']} /> : null}
              <span
                className={`${styles['close']} ${isDirty ? styles['closeWhenDirty'] : ''}`}
                role="button"
                aria-label={`Close ${file.name}`}
                onClick={(event) => {
                  event.stopPropagation()
                  close(projectId, file.path)
                }}
              >
                ✕
              </span>
            </button>
          )
        })}
      </div>

      <div className={styles['body']}>
        {active?.changedOnDisk ? (
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
              onClick={() => void overwrite(workspaceId, projectId, active.path, buffer.current)}
            >
              Keep mine
            </button>
          </div>
        ) : null}

        {error ? <div className={`${styles['notice']} ${styles['error']}`}>{error}</div> : null}
        {active ? <ActiveView
          // The revision is in the key so a reload rebuilds the editor with
          // what is now on disk: the initial text is read once, at mount.
          key={`${active.path}:${active.revision ?? 0}`}
          theme={theme}
          workspaceId={workspaceId}
          projectId={projectId}
          path={active.path}
          name={active.name}
          contents={active.contents}
          onChange={onChange}
          onSave={onSave}
        /> : null}
      </div>
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
  onChange,
  onSave,
}: {
  workspaceId: string
  projectId: string
  path: string
  name: string
  theme: 'dark' | 'light'
  contents: import('@/types/beacon').FileContents
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
      initialText={contents.text}
      onChange={onChange}
      onSave={onSave}
    />
  )
}
