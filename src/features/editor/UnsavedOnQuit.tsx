import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { useBeacon } from '@/app/store'
import { useEditor } from './openFiles'
import styles from './UnsavedOnQuit.module.css'

/** One project's unsaved files, with the names to say out loud. */
interface Pending {
  workspaceId: string
  projectId: string
  projectName: string
  paths: string[]
}

/**
 * What stands between quitting and losing an unsaved file.
 *
 * Beacon does not ask before destructive-sounding things — removing a project
 * leaves the repository alone, and trashing a file is recoverable from Finder.
 * This is the exception, because quitting is the one action that throws away
 * work which exists nowhere else, and nothing can get it back afterwards.
 */
export function UnsavedOnQuit(): React.ReactElement | null {
  const snapshot = useBeacon((s) => s.snapshot)
  const byProject = useEditor((s) => s.byProject)
  const [pending, setPending] = useState<Pending[] | null>(null)
  const [saving, setSaving] = useState(false)

  // The close handler is registered once and reads the current state when it
  // fires. Re-registering it on every keystroke would drop the event that
  // arrived in between.
  const unsaved = useRef<Pending[]>([])
  unsaved.current = unsavedAcross(snapshot, byProject)

  useEffect(() => {
    const listening = getCurrentWindow().onCloseRequested((event) => {
      if (unsaved.current.length === 0) return
      event.preventDefault()
      setPending(unsaved.current)
    })

    return () => {
      void listening.then((unlisten) => {
        unlisten()
      })
    }
  }, [])

  if (!pending || pending.length === 0) return null

  // Closing rather than destroying would come straight back through the handler
  // above; by this point the question has been asked and answered.
  const quit = (): void => void getCurrentWindow().destroy()

  const count = pending.reduce((total, project) => total + project.paths.length, 0)

  return createPortal(
    <div className={styles['scrim']}>
      <div className={styles['panel']} role="dialog" aria-label="Unsaved changes">
        <h2 className={styles['title']}>
          {count === 1
            ? 'One file has changes that are not on disk'
            : `${count} files have changes that are not on disk`}
        </h2>
        <ul className={styles['list']}>
          {pending.map((project) =>
            project.paths.map((path) => (
              <li key={`${project.projectId}:${path}`} className={styles['item']}>
                <span className={styles['project']}>{project.projectName}</span>
                <span className={styles['path']}>{path}</span>
              </li>
            )),
          )}
        </ul>
        <div className={styles['actions']}>
          <button type="button" className={styles['later']} onClick={() => setPending(null)}>
            Stay here
          </button>
          <button
            type="button"
            className={styles['later']}
            onClick={() => {
              setPending(null)
              quit()
            }}
          >
            Quit without saving
          </button>
          <button
            type="button"
            className={styles['save']}
            disabled={saving}
            onClick={() => {
              setSaving(true)
              void saveAll(pending).then((allWritten) => {
                setSaving(false)
                setPending(null)
                // Something refused — the file moved on disk, most likely.
                // Quitting now would throw away the very work in question, so
                // the tab is left to say what happened instead.
                if (allWritten) quit()
              })
            }}
          >
            {saving ? 'Saving…' : 'Save everything and quit'}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  )
}

/** Every unsaved file across every workspace, named so the user can place it. */
function unsavedAcross(
  snapshot: ReturnType<typeof useBeacon.getState>['snapshot'],
  byProject: ReturnType<typeof useEditor.getState>['byProject'],
): Pending[] {
  if (!snapshot) return []

  const pending: Pending[] = []
  for (const workspace of snapshot.workspaces) {
    for (const project of workspace.projects) {
      const paths = (byProject[project.id] ?? [])
        .filter((file) => file.draft !== file.saved)
        .map((file) => file.path)
      if (paths.length > 0) {
        pending.push({
          workspaceId: workspace.id,
          projectId: project.id,
          projectName: project.name,
          paths,
        })
      }
    }
  }
  return pending
}

/** Saves everything listed, reporting whether all of it reached the disk. */
async function saveAll(pending: Pending[]): Promise<boolean> {
  for (const project of pending) {
    for (const path of project.paths) {
      await useEditor.getState().save(project.workspaceId, project.projectId, path)
    }
  }

  const byProject = useEditor.getState().byProject
  return pending.every((project) =>
    project.paths.every((path) => {
      const file = byProject[project.projectId]?.find((open) => open.path === path)
      return file === undefined || file.draft === file.saved
    }),
  )
}
