import { useState } from 'react'

import { Popover } from '@/app/ui/Popover'
import { useProjectActivity } from '@/features/terminal/activity'
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

/** A single tab. Split out so only the busy one re-renders as activity changes. */
function ProjectTab({
  project,
  index,
  active,
  onSelect,
  onMenu,
}: {
  project: Project
  index: number
  active: boolean
  onSelect: () => void
  onMenu: (anchor: DOMRect) => void
}): React.ReactElement {
  const activity = useProjectActivity(project.id)
  const shortcut = index < 9 ? ` · ${shortcutLabel(String(index + 1))}` : ''

  return (
    <button
      type="button"
      className={styles['tab']}
      data-active={active}
      title={`${project.displayPath}${shortcut}`}
      onClick={onSelect}
      onContextMenu={(event) => {
        event.preventDefault()
        onMenu(event.currentTarget.getBoundingClientRect())
      }}
    >
      <span className={styles['status']} data-state={activity} />
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

  const onAdd = async (): Promise<void> => {
    const folder = await pickFolder('Add project', projectsHome)
    if (folder) await addProject(folder)
  }

  return (
    <div className={styles['strip']} data-tauri-drag-region>
      {projects.map((project, index) => (
        <ProjectTab
          key={project.id}
          project={project}
          index={index}
          active={project.id === activeProject?.id}
          onSelect={() => void selectProject(project.id)}
          onMenu={(anchor) => setMenu({ project, anchor })}
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
