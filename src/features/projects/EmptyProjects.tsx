import { useBeacon } from '@/app/store'
import { pickFolder } from '@/ipc'
import styles from './EmptyProjects.module.css'

/** Shown when the active workspace has no projects yet. */
export function EmptyProjects(): React.ReactElement {
  const addProject = useBeacon((s) => s.addProject)
  const projectsHome = useBeacon((s) => s.snapshot?.projectsHome)

  const onAdd = async (): Promise<void> => {
    const folder = await pickFolder('Add project', projectsHome)
    if (folder) await addProject(folder)
  }

  return (
    <div className={styles['root']}>
      <div className={styles['title']}>No projects in this workspace</div>
      <div className={styles['hint']}>
        Pick a folder. Beacon reads what is already there — git, package manager, toolchain — and
        never asks you to configure it.
      </div>
      <button type="button" className={styles['action']} onClick={() => void onAdd()}>
        Add project
      </button>
    </div>
  )
}
