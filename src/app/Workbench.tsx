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
import { usePanelFocus } from './panelFocus'
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

  // Focus is observed rather than declared. One listener here beats each panel
  // reporting for itself: the panels do not all own their focusable parts —
  // xterm and CodeMirror bring their own — and anything that takes the
  // keyboard inside a panel counts, however it got there.
  const watchFocus = (event: React.FocusEvent<HTMLDivElement>): void => {
    const panel = event.target.closest<HTMLElement>('[data-panel]')?.dataset['panel']
    usePanelFocus.getState().set((panel as PanelId | undefined) ?? null)
  }

  if (!draft) return <div className={styles['workbench']} />

  if (!project || !workspaceId) {
    return (
      <div className={`${styles['workbench']} ${styles['empty']}`}>
        <EmptyProjects />
      </div>
    )
  }

  const render = (panel: PanelId): React.ReactNode => renderPanel(panel, workspaceId, project)

  // Fullscreen bypasses the tree entirely rather than rewriting it, so leaving
  // fullscreen cannot lose the arrangement.
  if (fullscreen) {
    return (
      <div className={styles['workbench']} onFocus={watchFocus}>
        {render(fullscreen)}
      </div>
    )
  }

  const visible = prune(draft, hidden)
  if (!visible) return <div className={styles['workbench']} />

  return (
    <div className={styles['workbench']} onFocus={watchFocus}>
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
function renderPanel(panel: PanelId, workspaceId: string, project: Project): React.ReactNode {
  switch (panel) {
    case 'claude':
      // The one panel that takes the keyboard unasked, because it is what the
      // window is for and typing into it is the first thing anyone does.
      return <ClaudePanel workspaceId={workspaceId} project={project} autoFocus />
    case 'files':
      return <FilesPanel workspaceId={workspaceId} project={project} />
    case 'editor':
      return <EditorPanel workspaceId={workspaceId} project={project} />
    case 'git':
      return <GitPanel workspaceId={workspaceId} project={project} />
    case 'terminal':
      return <TerminalPanel workspaceId={workspaceId} project={project} />
  }
}
