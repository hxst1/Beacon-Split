import { TerminalView } from '@/features/terminal/TerminalView'
import { useBeacon } from '@/app/store'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'
import styles from './Panel.module.css'

/**
 * The centre of the app: the real `claude` CLI in a PTY, one session per
 * project.
 *
 * Beacon does not reimplement Claude Code. Colours, prompts, permissions,
 * selection and scrolling are whatever the CLI does, because it is the CLI.
 */
export function ClaudePanel({
  workspaceId,
  project,
  focused,
}: {
  workspaceId: string
  project: Project
  focused: boolean
}): React.ReactElement {
  const restartSession = useBeacon((s) => s.restartSession)
  const epoch = useBeacon((s) => s.sessionEpoch[`${project.id}:claude`] ?? 0)

  return (
    <Panel
      title="Claude"
      subtitle={project.displayPath}
      focused={focused}
      actions={
        <button
          type="button"
          className={styles['action']}
          title="Restart Claude"
          onClick={() => void restartSession(project.id, 'claude')}
        >
          Restart
        </button>
      }
    >
      <TerminalView
        key={`${project.id}:${epoch}`}
        workspaceId={workspaceId}
        projectId={project.id}
        kind="claude"
        autoFocus={focused}
      />
    </Panel>
  )
}
