import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { Popover } from '@/app/ui/Popover'
import { useBeacon } from '@/app/store'
import { useLiveRefresh } from '@/lib/useLiveRefresh'
import { useEditor } from '@/features/editor/openFiles'
import type { DirEntry } from '@/types/beacon'
import { FileMenu, type MenuPrompt } from './FileMenu'
import { parentOf, useTree, visibleRows, type TreeRow } from './treeStore'
import styles from './FileTree.module.css'

interface MenuTarget {
  entry: DirEntry | null
  anchor: DOMRect
  /** Set when a key asked for one thing rather than for the whole menu. */
  prompt?: MenuPrompt | undefined
}

const NOTES: Record<'reading' | 'empty' | 'hidden', string> = {
  reading: 'Reading…',
  empty: 'Empty',
  hidden: 'Only hidden files',
}

/**
 * A conventional file tree, not a TUI.
 *
 * Directories load a level at a time on expand, so a project with a large
 * dependency folder costs nothing until someone opens it. The rows are drawn
 * from one flat list rather than nested components: the keyboard moves through
 * what is on screen, and what is on screen is that list.
 */
export function FileTree({
  workspaceId,
  projectId,
}: {
  workspaceId: string
  projectId: string
}): React.ReactElement {
  const load = useTree((s) => s.load)
  const refreshAll = useTree((s) => s.refreshAll)
  const entries = useTree((s) => s.entries)
  const expanded = useTree((s) => s.expanded)
  const loading = useTree((s) => s.loading)
  // A setting rather than panel state, so it is read from the same place as
  // every other one and outlives the window.
  const showHidden = useBeacon((s) => s.snapshot?.showHiddenFiles ?? false)
  const setShowHidden = useBeacon((s) => s.setShowHiddenFiles)
  const selected = useTree((s) => s.selected[projectId])
  const select = useTree((s) => s.select)
  const toggle = useTree((s) => s.toggle)
  const setExpanded = useTree((s) => s.setExpanded)
  const error = useTree((s) => s.error)

  const openFile = useEditor((s) => s.open)
  const showPanel = useBeacon((s) => s.showPanel)

  const [menu, setMenu] = useState<MenuTarget | null>(null)
  const rows = useRef<Map<string, HTMLButtonElement>>(new Map())

  useEffect(() => {
    void load(workspaceId, projectId, '')
  }, [load, workspaceId, projectId])

  // Focus only: re-reading on a timer would fight with scrolling and selection.
  useLiveRefresh(
    useCallback(() => {
      void refreshAll(workspaceId, projectId)
    }, [refreshAll, workspaceId, projectId]),
    null,
  )

  const visible = useMemo(
    () => visibleRows({ entries, expanded, loading, showHidden }, projectId),
    [entries, expanded, loading, showHidden, projectId],
  )
  const items = useMemo(
    () => visible.flatMap((row) => (row.type === 'entry' ? [row] : [])),
    [visible],
  )

  // Roving tabindex: one row is in the tab order, and it is the selected one
  // unless the selection has scrolled out of the tree entirely.
  const active =
    items.find((row) => row.entry.path === selected)?.entry.path ?? items[0]?.entry.path

  const focusRow = (path: string | undefined): void => {
    if (path === undefined) return
    select(projectId, path)
    rows.current.get(path)?.focus()
  }

  const activate = (entry: DirEntry): void => {
    select(projectId, entry.path)
    if (entry.kind === 'directory') {
      void toggle(workspaceId, projectId, entry.path)
      return
    }
    // Opening a file is what brings the editor out; it starts hidden so an
    // empty pane never takes up room.
    void openFile(workspaceId, projectId, entry.path)
    void showPanel('editor')
  }

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    // Trash is the one thing people reach for with a modifier held — it is
    // Cmd+Backspace on a Mac — so it is the one key that accepts them.
    const trashing = event.key === 'Delete' || event.key === 'Backspace'
    if (!trashing && (event.metaKey || event.ctrlKey || event.altKey)) return

    const index = Math.max(
      items.findIndex((row) => row.entry.path === active),
      0,
    )
    const row = items[index]
    const step = (to: number): void => {
      focusRow(items[Math.min(Math.max(to, 0), items.length - 1)]?.entry.path)
    }

    switch (event.key) {
      case 'ArrowDown':
        step(index + 1)
        break
      case 'ArrowUp':
        step(index - 1)
        break
      case 'Home':
        step(0)
        break
      case 'End':
        step(items.length - 1)
        break
      case 'ArrowRight':
        if (!row) break
        if (row.entry.kind !== 'directory') break
        // Open it, or — already open — walk into what it contains.
        if (row.expanded) step(index + 1)
        else void setExpanded(workspaceId, projectId, row.entry.path, true)
        break
      case 'ArrowLeft':
        if (!row) break
        if (row.entry.kind === 'directory' && row.expanded) {
          void setExpanded(workspaceId, projectId, row.entry.path, false)
        } else {
          const parent = parentOf(row.entry.path)
          if (parent) focusRow(parent)
        }
        break
      case 'F2':
        if (row) {
          setMenu({
            entry: row.entry,
            anchor: rectOf(rows.current.get(row.entry.path)),
            prompt: 'rename',
          })
        }
        break
      case 'Delete':
      case 'Backspace':
        if (row) {
          setMenu({
            entry: row.entry,
            anchor: rectOf(rows.current.get(row.entry.path)),
            prompt: 'confirm-trash',
          })
        }
        break
      default:
        return
    }
    event.preventDefault()
  }

  return (
    <div className={styles['root']}>
      <div className={styles['toolbar']}>
        <span className={styles['spacer']} />
        <button
          type="button"
          className={styles['tool']}
          title="Refresh"
          aria-label="Refresh"
          onClick={() => void refreshAll(workspaceId, projectId)}
        >
          ↻
        </button>
        <button
          type="button"
          className={styles['tool']}
          data-on={showHidden}
          title={showHidden ? 'Hide dotfiles' : 'Show dotfiles'}
          onClick={() => void setShowHidden(!showHidden)}
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
        onKeyDown={onKeyDown}
        onContextMenu={(event) => {
          // A right-click on empty space acts on the project root.
          if (event.target !== event.currentTarget) return
          event.preventDefault()
          setMenu({ entry: null, anchor: rectAt(event.clientX, event.clientY) })
        }}
      >
        {error ? (
          <div className={`${styles['status']} ${styles['error']}`} role="alert">
            {error}
          </div>
        ) : null}

        {/* The rows are their own element so that everything a tree owns is a
            row, and the message above it is not read as one. */}
        <div role="tree" aria-label="Files">
          {visible.map((row) =>
            row.type === 'note' ? (
              <div
                key={row.id}
                role="none"
                className={styles['note']}
                style={{ paddingLeft: `${20 + row.depth * 12}px` }}
              >
                {NOTES[row.note]}
              </div>
            ) : (
              <Row
                key={row.id}
                row={row}
                active={row.entry.path === active}
                selected={row.entry.path === selected}
                onRef={(node) => {
                  if (node) rows.current.set(row.entry.path, node)
                  else rows.current.delete(row.entry.path)
                }}
                onActivate={activate}
                onMenu={(entry, anchor) => {
                  select(projectId, entry.path)
                  setMenu({ entry, anchor })
                }}
              />
            ),
          )}
        </div>
      </div>

      {menu ? (
        <Popover anchor={menu.anchor} onClose={() => setMenu(null)}>
          <FileMenu
            workspaceId={workspaceId}
            projectId={projectId}
            entry={menu.entry}
            initialPrompt={menu.prompt}
            onDone={() => setMenu(null)}
          />
        </Popover>
      ) : null}
    </div>
  )
}

function Row({
  row,
  active,
  selected,
  onRef,
  onActivate,
  onMenu,
}: {
  row: Extract<TreeRow, { type: 'entry' }>
  active: boolean
  selected: boolean
  onRef: (node: HTMLButtonElement | null) => void
  onActivate: (entry: DirEntry) => void
  onMenu: (entry: DirEntry, anchor: DOMRect) => void
}): React.ReactElement {
  const { entry } = row
  const isDirectory = entry.kind === 'directory'

  return (
    <button
      ref={onRef}
      type="button"
      role="treeitem"
      aria-level={row.depth + 1}
      aria-selected={selected}
      {...(isDirectory ? { 'aria-expanded': row.expanded } : {})}
      tabIndex={active ? 0 : -1}
      className={styles['row']}
      style={{ paddingLeft: `${8 + row.depth * 12}px` }}
      data-selected={selected}
      data-hidden={entry.hidden}
      title={entry.path}
      onClick={(event) => {
        // WebKit does not focus a button when it is clicked, and the arrow
        // keys have to carry on from wherever the pointer left off.
        event.currentTarget.focus()
        onActivate(entry)
      }}
      onContextMenu={(event) => {
        event.preventDefault()
        event.stopPropagation()
        onMenu(entry, rectAt(event.clientX, event.clientY))
      }}
    >
      <span className={styles['twisty']} data-open={row.expanded}>
        {isDirectory ? '▶' : ''}
      </span>
      <span className={`${styles['name']} ${isDirectory ? styles['dirName'] : ''}`}>
        {entry.name}
      </span>
    </button>
  )
}

/** A zero-size rect at the pointer, so a menu can anchor to a click. */
function rectAt(x: number, y: number): DOMRect {
  return new DOMRect(x, y, 0, 0)
}

/** Where a row is, for a menu opened from the keyboard rather than a click. */
function rectOf(element: HTMLElement | undefined): DOMRect {
  return element?.getBoundingClientRect() ?? rectAt(0, 0)
}
