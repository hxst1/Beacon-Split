import { useState } from 'react'

import { Popover } from '@/app/ui/Popover'
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
    <div className={styles['strip']}>
      {projects.map((project, index) => (
        <button
          key={project.id}
          type="button"
          className={styles['tab']}
          data-active={project.id === activeProject?.id}
          title={`${project.displayPath}${index < 9 ? ` · ${shortcutLabel(String(index + 1))}` : ''}`}
          onClick={() => void selectProject(project.id)}
          onContextMenu={(event) => {
            event.preventDefault()
            setMenu({ project, anchor: event.currentTarget.getBoundingClientRect() })
          }}
        >
          <span className={styles['status']} data-state="idle" />
          <span className={styles['name']}>{project.name}</span>
        </button>
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
