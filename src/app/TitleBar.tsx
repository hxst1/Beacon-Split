import { useState } from 'react'

import { ProjectTabs } from '@/features/projects/ProjectTabs'
import { UsageMeter } from '@/features/usage/UsageMeter'
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
  const setOverlay = useBeacon((s) => s.setOverlay)

  return (
    <header className={styles['bar']} data-tauri-drag-region>
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

      <ProjectTabs />

      <div className={styles['dragZone']} data-tauri-drag-region aria-hidden="true" />

      <UsageMeter />

      <button
        type="button"
        className={styles['gear']}
        title="Settings"
        onClick={() => setOverlay('settings')}
      >
        <GearIcon />
      </button>

      {menuAnchor ? (
        <Popover anchor={menuAnchor} onClose={() => setMenuAnchor(null)}>
          <WorkspaceMenu onDone={() => setMenuAnchor(null)} />
        </Popover>
      ) : null}

    </header>
  )
}

/**
 * Drawn inline: one icon does not justify an icon dependency.
 *
 * A ring with eight teeth around it, and the hub punched out with `evenodd` so
 * the hole shows the background rather than a painted circle — which would be
 * visible the moment the button is hovered.
 */
const GearIcon = (): React.ReactElement => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    {[0, 45, 90, 135, 180, 225, 270, 315].map((angle) => (
      <rect
        key={angle}
        x="10.4"
        y="1.4"
        width="3.2"
        height="5"
        rx="1"
        transform={`rotate(${angle} 12 12)`}
      />
    ))}
    <path
      fillRule="evenodd"
      d="M12 5a7 7 0 1 0 0 14 7 7 0 0 0 0-14Zm0 4.4a2.6 2.6 0 1 1 0 5.2 2.6 2.6 0 0 1 0-5.2Z"
    />
  </svg>
)
