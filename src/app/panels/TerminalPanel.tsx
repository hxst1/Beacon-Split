import { TerminalView } from '@/features/terminal/TerminalView'
import { useBeacon } from '@/app/store'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'
import styles from './Panel.module.css'

/**
 * A real shell for the project, opened at its root.
 *
 * Keyed by project so switching tabs mounts that project's own view; the
 * underlying session and its xterm instance are cached and simply reattached.
 */
export function TerminalPanel({
  workspaceId,
  project,
  focused,
}: {
  workspaceId: string
  project: Project
  focused: boolean
}): React.ReactElement {
  const restartSession = useBeacon((s) => s.restartSession)
  const epoch = useBeacon((s) => s.sessionEpoch[`${project.id}:shell`] ?? 0)
  const attachEpoch = useBeacon((s) => s.attachEpoch)

  return (
    <Panel
      title="Terminal"
      subtitle={project.displayPath}
      focused={focused}
      actions={
        <button
          type="button"
          className={styles['action']}
          title="Restart the shell"
          onClick={() => void restartSession(project.id, 'shell')}
        >
          Restart
        </button>
      }
    >
      <TerminalView
        key={`${project.id}:${epoch}:${attachEpoch}`}
        workspaceId={workspaceId}
        projectId={project.id}
        kind="shell"
      />
    </Panel>
  )
}
