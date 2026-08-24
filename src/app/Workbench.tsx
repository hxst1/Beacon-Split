import { useEffect, useState } from 'react'

import { EmptyProjects } from '@/features/projects/EmptyProjects'
import { prune, withFraction } from '@/lib/layout'
import type { Path } from '@/lib/layout'
import type { PanelId, Project } from '@/types/beacon'
import { ClaudePanel } from './panels/ClaudePanel'
import { EditorPanel } from './panels/EditorPanel'
import { FilesPanel } from './panels/FilesPanel'
import { GitPanel } from './panels/GitPanel'
import { TerminalPanel } from './panels/TerminalPanel'
import { LayoutView } from './LayoutView'
import { selectActiveProject, selectHidden, selectLayout, useBeacon } from './store'
import styles from './Workbench.module.css'

/**
 * The panel area.
 *
 * The arrangement comes entirely from the layout tree, so switching presets or
 * hand-arranging panels needs nothing here. A drag in progress is local state:
 * only the released size goes through the backend.
 */
export function Workbench(): React.ReactElement {
  const layout = useBeacon(selectLayout)
  const hidden = useBeacon(selectHidden)
  const setLayout = useBeacon((s) => s.setLayout)
  const project = useBeacon(selectActiveProject)
  const workspaceId = useBeacon((s) => s.snapshot?.activeWorkspace)
  const fullscreen = useBeacon((s) => s.fullscreenPanel)

  const [draft, setDraft] = useState(layout)
  useEffect(() => setDraft(layout), [layout])

  if (!draft) return <div className={styles['workbench']} />

  if (!project || !workspaceId) {
    return (
      <div className={`${styles['workbench']} ${styles['empty']}`}>
        <EmptyProjects />
      </div>
    )
  }

  const render = (panel: PanelId): React.ReactNode =>
    renderPanel(panel, workspaceId, project, fullscreen)

  // Fullscreen bypasses the tree entirely rather than rewriting it, so leaving
  // fullscreen cannot lose the arrangement.
  if (fullscreen) {
    return <div className={styles['workbench']}>{render(fullscreen)}</div>
  }

  const visible = prune(draft, hidden)
  if (!visible) return <div className={styles['workbench']} />

  return (
    <div className={styles['workbench']}>
      <LayoutView
        node={visible}
        render={render}
        onResize={(path: Path, fraction: number) => setDraft(withFraction(draft, path, fraction))}
        onCommit={() => void setLayout(draft)}
      />
    </div>
  )
}

/** Maps a panel id to the component that draws it. */
function renderPanel(
  panel: PanelId,
  workspaceId: string,
  project: Project,
  fullscreen: PanelId | null,
): React.ReactNode {
  const focused = fullscreen === panel || (fullscreen === null && panel === 'claude')

  switch (panel) {
    case 'claude':
      return <ClaudePanel workspaceId={workspaceId} project={project} focused={focused} />
    case 'files':
      return <FilesPanel workspaceId={workspaceId} project={project} />
    case 'editor':
      return <EditorPanel workspaceId={workspaceId} project={project} focused={focused} />
    case 'git':
      return <GitPanel workspaceId={workspaceId} project={project} />
    case 'terminal':
      return (
        <TerminalPanel workspaceId={workspaceId} project={project} focused={focused} />
      )
  }
}
