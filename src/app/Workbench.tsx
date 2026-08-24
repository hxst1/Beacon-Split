import { useEffect, useState } from 'react'

import { EmptyProjects } from '@/features/projects/EmptyProjects'
import { ClaudePanel } from './panels/ClaudePanel'
import { FilesPanel } from './panels/FilesPanel'
import { GitPanel } from './panels/GitPanel'
import { TerminalPanel } from './panels/TerminalPanel'
import { Resizer } from './Resizer'
import { selectActiveProject, useBeacon } from './store'
import styles from './Workbench.module.css'

/**
 * The panel layout.
 *
 * Panel sizes live in the store (and on disk), but a drag in progress is local
 * state: we only push the final size through the backend on release.
 */
export function Workbench(): React.ReactElement {
  const panels = useBeacon((s) => s.snapshot?.panels)
  const setPanels = useBeacon((s) => s.setPanels)
  const project = useBeacon(selectActiveProject)
  const workspaceId = useBeacon((s) => s.snapshot?.activeWorkspace)
  const fullscreen = useBeacon((s) => s.fullscreenPanel)

  const [draft, setDraft] = useState(panels)
  useEffect(() => setDraft(panels), [panels])

  if (!draft) return <div className={styles['workbench']} />

  const commit = (): void => {
    void setPanels(draft)
  }

  const sideVisible = draft.sideVisible && !fullscreen
  const terminalVisible = draft.terminalVisible && !fullscreen

  const style = {
    '--side-width': `${(sideVisible ? draft.sideFraction : 0) * 100}%`,
    '--terminal-height': `${(terminalVisible ? draft.terminalFraction : 0) * 100}%`,
    '--files-height': `${draft.filesFraction * 100}%`,
  } as React.CSSProperties

  if (!project || !workspaceId) {
    return (
      <div className={styles['workbench']} style={style}>
        <div className={styles['empty']}>
          <EmptyProjects />
        </div>
      </div>
    )
  }

  if (fullscreen) {
    return (
      <div className={styles['workbench']} style={style}>
        <div className={styles['fullscreen']}>
          {fullscreen === 'claude' ? <ClaudePanel project={project} focused /> : null}
          {fullscreen === 'terminal' ? (
            <TerminalPanel workspaceId={workspaceId} project={project} focused />
          ) : null}
          {fullscreen === 'side' ? <FilesPanel project={project} /> : null}
        </div>
      </div>
    )
  }

  return (
    <div
      className={styles['workbench']}
      style={style}
      data-side={sideVisible}
      data-terminal={terminalVisible}
    >
      <div className={styles['main']}>
        <ClaudePanel project={project} focused />
      </div>

      {sideVisible ? (
        <Resizer
          orientation="vertical"
          area="vsplit"
          onDrag={(fraction) => setDraft({ ...draft, sideFraction: clamp(fraction, 0.15, 0.45) })}
          onCommit={commit}
        />
      ) : null}

      {sideVisible ? (
        <aside className={styles['side']}>
          <FilesPanel project={project} />
          <Resizer
            orientation="horizontal"
            from="start"
            within="parent"
            onDrag={(fraction) => setDraft({ ...draft, filesFraction: clamp(fraction, 0.2, 0.85) })}
            onCommit={commit}
          />
          <GitPanel />
        </aside>
      ) : null}

      {terminalVisible ? (
        <Resizer
          orientation="horizontal"
          area="hsplit"
          onDrag={(fraction) =>
            setDraft({ ...draft, terminalFraction: clamp(fraction, 0.12, 0.6) })
          }
          onCommit={commit}
        />
      ) : null}

      {terminalVisible ? (
        <div className={styles['terminal']}>
          <TerminalPanel workspaceId={workspaceId} project={project} focused={false} />
        </div>
      ) : null}
    </div>
  )
}

const clamp = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, value))
