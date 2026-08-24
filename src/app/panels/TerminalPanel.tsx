import { TerminalView } from '@/features/terminal/TerminalView'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'

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
  return (
    <Panel title="Terminal" subtitle={project.displayPath} focused={focused}>
      <TerminalView
        key={project.id}
        workspaceId={workspaceId}
        projectId={project.id}
        kind="shell"
      />
    </Panel>
  )
}
