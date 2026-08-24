import { useState } from 'react'

import { ProjectTabs } from '@/features/projects/ProjectTabs'
import { WorkspaceMenu } from '@/features/workspaces/WorkspaceMenu'
import { selectActiveWorkspace, useBeacon } from './store'
import { Popover } from './ui/Popover'
import styles from './TitleBar.module.css'

/**
 * The single row of chrome at the top of the window: which workspace you are
 * in, and which project you are looking at. Everything else is a panel.
 */
export function TitleBar(): React.ReactElement {
  const workspace = useBeacon(selectActiveWorkspace)
  const [menuAnchor, setMenuAnchor] = useState<DOMRect | null>(null)

  return (
    <header className={styles['bar']}>
      <button
        type="button"
        className={styles['workspace']}
        onClick={(event) => setMenuAnchor(event.currentTarget.getBoundingClientRect())}
        aria-haspopup="menu"
      >
        <span className={styles['workspaceDot']} />
        {workspace?.name ?? 'Workspace'}
        <span className={styles['chevron']}>▼</span>
      </button>

      <div className={styles['divider']} />

      <div className={styles['tabsRegion']}>
        <ProjectTabs />
      </div>

      {menuAnchor ? (
        <Popover anchor={menuAnchor} onClose={() => setMenuAnchor(null)}>
          <WorkspaceMenu onDone={() => setMenuAnchor(null)} />
        </Popover>
      ) : null}
    </header>
  )
}
