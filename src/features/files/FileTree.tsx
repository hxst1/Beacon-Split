import { useEffect, useState } from 'react'

import { Popover } from '@/app/ui/Popover'
import { useBeacon } from '@/app/store'
import { useEditor } from '@/features/editor/openFiles'
import type { DirEntry } from '@/types/beacon'
import { FileMenu } from './FileMenu'
import { treeKey, useTree } from './treeStore'
import styles from './FileTree.module.css'

interface MenuTarget {
  entry: DirEntry | null
  anchor: DOMRect
}

/**
 * A conventional file tree, not a TUI.
 *
 * Directories load a level at a time on expand, so a project with a large
 * dependency folder costs nothing until someone opens it.
 */
export function FileTree({
  workspaceId,
  projectId,
}: {
  workspaceId: string
  projectId: string
}): React.ReactElement {
  const load = useTree((s) => s.load)
  const showHidden = useTree((s) => s.showHidden)
  const setShowHidden = useTree((s) => s.setShowHidden)
  const error = useTree((s) => s.error)
  const [menu, setMenu] = useState<MenuTarget | null>(null)

  useEffect(() => {
    void load(workspaceId, projectId, '')
  }, [load, workspaceId, projectId])

  return (
    <div className={styles['root']}>
      <div className={styles['toolbar']}>
        <span className={styles['spacer']} />
        <button
          type="button"
          className={styles['tool']}
          data-on={showHidden}
          title={showHidden ? 'Hide dotfiles' : 'Show dotfiles'}
          onClick={() => setShowHidden(!showHidden)}
        >
          .*
        </button>
        <button
          type="button"
          className={styles['tool']}
          title="New file or folder"
          onClick={(event) => setMenu({ entry: null, anchor: event.currentTarget.getBoundingClientRect() })}
        >
          +
        </button>
      </div>

      <div
        className={styles['list']}
        onContextMenu={(event) => {
          // A right-click on empty space acts on the project root.
          if (event.target !== event.currentTarget) return
          event.preventDefault()
          setMenu({ entry: null, anchor: rectAt(event.clientX, event.clientY) })
        }}
      >
        {error ? <div className={`${styles['status']} ${styles['error']}`}>{error}</div> : null}
        <Level
          workspaceId={workspaceId}
          projectId={projectId}
          path=""
          depth={0}
          onMenu={(entry, anchor) => setMenu({ entry, anchor })}
        />
      </div>

      {menu ? (
        <Popover anchor={menu.anchor} onClose={() => setMenu(null)}>
          <FileMenu
            workspaceId={workspaceId}
            projectId={projectId}
            entry={menu.entry}
            onDone={() => setMenu(null)}
          />
        </Popover>
      ) : null}
    </div>
  )
}

function Level({
  workspaceId,
  projectId,
  path,
  depth,
  onMenu,
}: {
  workspaceId: string
  projectId: string
  path: string
  depth: number
  onMenu: (entry: DirEntry, anchor: DOMRect) => void
}): React.ReactElement | null {
  const entries = useTree((s) => s.entries[treeKey(projectId, path)])
  const loading = useTree((s) => s.loading[treeKey(projectId, path)] === true)
  const showHidden = useTree((s) => s.showHidden)

  if (loading && !entries) {
    return <div className={styles['status']}>Reading…</div>
  }
  if (!entries) return null

  const visible = showHidden ? entries : entries.filter((entry) => !entry.hidden)

  return (
    <>
      {visible.map((entry) => (
        <Row
          key={entry.path}
          workspaceId={workspaceId}
          projectId={projectId}
          entry={entry}
          depth={depth}
          onMenu={onMenu}
        />
      ))}
    </>
  )
}

function Row({
  workspaceId,
  projectId,
  entry,
  depth,
  onMenu,
}: {
  workspaceId: string
  projectId: string
  entry: DirEntry
  depth: number
  onMenu: (entry: DirEntry, anchor: DOMRect) => void
}): React.ReactElement {
  const expanded = useTree((s) => s.expanded[treeKey(projectId, entry.path)] === true)
  const selected = useTree((s) => s.selected[projectId] === entry.path)
  const toggle = useTree((s) => s.toggle)
  const select = useTree((s) => s.select)
  const openFile = useEditor((s) => s.open)
  const showPanel = useBeacon((s) => s.showPanel)

  const isDirectory = entry.kind === 'directory'

  const activate = (): void => {
    select(projectId, entry.path)
    if (isDirectory) {
      void toggle(workspaceId, projectId, entry.path)
      return
    }
    // Opening a file is what brings the editor out; it starts hidden so an
    // empty pane never takes up room.
    void openFile(workspaceId, projectId, entry.path)
    void showPanel('editor')
  }

  return (
    <>
      <button
        type="button"
        className={styles['row']}
        style={{ paddingLeft: `${8 + depth * 12}px` }}
        data-selected={selected}
        data-hidden={entry.hidden}
        title={entry.path}
        onClick={activate}
        onContextMenu={(event) => {
          event.preventDefault()
          event.stopPropagation()
          select(projectId, entry.path)
          onMenu(entry, rectAt(event.clientX, event.clientY))
        }}
      >
        <span className={styles['twisty']} data-open={expanded}>
          {isDirectory ? '▶' : ''}
        </span>
        <span className={`${styles['name']} ${isDirectory ? styles['dirName'] : ''}`}>
          {entry.name}
        </span>
      </button>

      {isDirectory && expanded ? (
        <Level
          workspaceId={workspaceId}
          projectId={projectId}
          path={entry.path}
          depth={depth + 1}
          onMenu={onMenu}
        />
      ) : null}
    </>
  )
}

/** A zero-size rect at the pointer, so a menu can anchor to a click. */
function rectAt(x: number, y: number): DOMRect {
  return new DOMRect(x, y, 0, 0)
}
