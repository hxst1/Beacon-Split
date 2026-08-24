import { useState } from 'react'

import { ProjectTabs } from '@/features/projects/ProjectTabs'
import { Settings } from '@/features/settings/Settings'
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
  const [settingsAnchor, setSettingsAnchor] = useState<DOMRect | null>(null)

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

      <button
        type="button"
        className={styles['gear']}
        data-open={settingsAnchor !== null}
        title="Settings"
        aria-haspopup="menu"
        onClick={(event) => setSettingsAnchor(event.currentTarget.getBoundingClientRect())}
      >
        <GearIcon />
      </button>

      {menuAnchor ? (
        <Popover anchor={menuAnchor} onClose={() => setMenuAnchor(null)}>
          <WorkspaceMenu onDone={() => setMenuAnchor(null)} />
        </Popover>
      ) : null}

      {settingsAnchor ? (
        <Popover anchor={settingsAnchor} align="end" onClose={() => setSettingsAnchor(null)}>
          <Settings onDone={() => setSettingsAnchor(null)} />
        </Popover>
      ) : null}
    </header>
  )
}

/** Drawn inline: one icon does not justify an icon dependency. */
const GearIcon = (): React.ReactElement => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
    <circle cx="8" cy="8" r="2.4" stroke="currentColor" strokeWidth="1.2" />
    <path
      d="M8 1.5v1.6M8 12.9v1.6M14.5 8h-1.6M3.1 8H1.5M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1M12.6 12.6l-1.1-1.1M4.5 4.5L3.4 3.4"
      stroke="currentColor"
      strokeWidth="1.2"
      strokeLinecap="round"
    />
  </svg>
)
