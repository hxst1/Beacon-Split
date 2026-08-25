import { useRef, useState } from 'react'

import { Popover } from '@/app/ui/Popover'
import { useProjectActivity, useProjectDetail } from '@/features/terminal/activity'
import { selectActiveProject, selectProjects, useBeacon } from '@/app/store'
import { pickFolder } from '@/ipc'
import { shortcutLabel } from '@/lib/platform'
import type { Project } from '@/types/beacon'
import { ProjectMenu } from './ProjectMenu'
import styles from './ProjectTabs.module.css'

interface MenuTarget {
  project: Project
  anchor: DOMRect
}

/** Far enough that a click is not mistaken for the start of a drag. */
const DRAG_THRESHOLD_PX = 5

interface Dragging {
  id: string
  from: number
  /** Where it would land if released now. */
  to: number
  started: boolean
}

const ACTIVITY_LABELS: Record<string, string> = {
  working: 'Claude is working',
  waiting: 'Claude is waiting for you',
  done: 'Claude finished',
  idle: 'Idle',
  stopped: 'Stopped',
}

/** A single tab. Split out so only the busy one re-renders as activity changes. */
function ProjectTab({
  project,
  index,
  active,
  dragging,
  drop,
  onSelect,
  onMenu,
  onDragStart,
  onDragMove,
  onDragEnd,
}: {
  project: Project
  index: number
  active: boolean
  dragging: boolean
  drop: 'before' | 'after' | undefined
  onSelect: () => void
  onMenu: (anchor: DOMRect) => void
  onDragStart: () => void
  onDragMove: (clientX: number, movedBy: number) => void
  onDragEnd: () => void
}): React.ReactElement {
  const origin = useRef<number | null>(null)
  const activity = useProjectActivity(project.id)
  const detail = useProjectDetail(project.id)
  const shortcut = index < 9 ? ` · ${shortcutLabel(String(index + 1))}` : ''
  const doing = ACTIVITY_LABELS[activity]
  const said = detail ? `${doing} (${detail})` : doing

  return (
    <button
      type="button"
      data-tab
      className={styles['tab']}
      data-active={active}
      data-dragging={dragging}
      data-drop={drop}
      title={`${project.displayPath}${shortcut}\n${said}`}
      onClick={() => {
        // A drag that moved is not also a click on whatever it ended over.
        if (origin.current === null) onSelect()
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return
        origin.current = event.clientX
        event.currentTarget.setPointerCapture(event.pointerId)
        onDragStart()
      }}
      onPointerMove={(event) => {
        if (origin.current === null) return
        onDragMove(event.clientX, Math.abs(event.clientX - origin.current))
      }}
      onPointerUp={(event) => {
        const moved = origin.current !== null && Math.abs(event.clientX - origin.current) > 3
        event.currentTarget.releasePointerCapture(event.pointerId)
        onDragEnd()
        // Cleared after the click handler would have run, so a real click still
        // selects but the end of a drag does not.
        if (!moved) origin.current = null
        else window.setTimeout(() => (origin.current = null), 0)
      }}
      onContextMenu={(event) => {
        event.preventDefault()
        onMenu(event.currentTarget.getBoundingClientRect())
      }}
    >
      <span className={styles['status']} data-state={activity} title={said} />
      <span className={styles['name']}>{project.name}</span>
    </button>
  )
}

/**
 * One tab per project in the active workspace.
 *
 * Switching is a single command and a single re-render; nothing here waits on
 * the filesystem.
 */
export function ProjectTabs(): React.ReactElement {
  const projects = useBeacon(selectProjects)
  const activeProject = useBeacon(selectActiveProject)
  const selectProject = useBeacon((s) => s.selectProject)
  const addProject = useBeacon((s) => s.addProject)
  const projectsHome = useBeacon((s) => s.snapshot?.projectsHome)
  const [menu, setMenu] = useState<MenuTarget | null>(null)
  const [dragging, setDragging] = useState<Dragging | null>(null)
  const reorderProject = useBeacon((s) => s.reorderProject)
  const stripRef = useRef<HTMLDivElement>(null)

  /**
   * Which slot the pointer is over.
   *
   * Measured from the tabs themselves rather than from arithmetic on widths:
   * they are not all the same width, and a project called `a` next to one
   * called `visacashapprb-com` is exactly the case where guessing goes wrong.
   */
  const slotAt = (clientX: number): number => {
    const tabs = [...(stripRef.current?.querySelectorAll('[data-tab]') ?? [])]
    for (let index = 0; index < tabs.length; index += 1) {
      const box = tabs[index]?.getBoundingClientRect()
      if (box && clientX < box.left + box.width / 2) return index
    }
    return tabs.length - 1
  }

  const onAdd = async (): Promise<void> => {
    const folder = await pickFolder('Add project', projectsHome)
    if (folder) await addProject(folder)
  }

  return (
    <div className={styles['strip']} data-tauri-drag-region ref={stripRef}>
      {projects.map((project, index) => (
        <ProjectTab
          key={project.id}
          project={project}
          index={index}
          active={project.id === activeProject?.id}
          dragging={dragging?.started === true && dragging.id === project.id}
          drop={
            dragging?.started && dragging.id !== project.id && dragging.to === index
              ? dragging.from < index
                ? 'after'
                : 'before'
              : undefined
          }
          onSelect={() => void selectProject(project.id)}
          onMenu={(anchor) => setMenu({ project, anchor })}
          onDragStart={() => setDragging({ id: project.id, from: index, to: index, started: false })}
          onDragMove={(clientX, moved) => {
            setDragging((current) =>
              current && current.id === project.id
                ? { ...current, started: current.started || moved > DRAG_THRESHOLD_PX, to: slotAt(clientX) }
                : current,
            )
          }}
          onDragEnd={() => {
            setDragging((current) => {
              if (current?.started && current.to !== current.from) {
                void reorderProject(current.id, current.to)
              }
              return null
            })
          }}
        />
      ))}

      <button
        type="button"
        className={styles['add']}
        title="Add project"
        onClick={() => void onAdd()}
      >
        +
      </button>

      {menu ? (
        <Popover anchor={menu.anchor} onClose={() => setMenu(null)}>
          <ProjectMenu project={menu.project} onDone={() => setMenu(null)} />
        </Popover>
      ) : null}
    </div>
  )
}
