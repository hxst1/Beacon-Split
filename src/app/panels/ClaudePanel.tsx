import { TerminalView } from '@/features/terminal/TerminalView'
import { MissingTool } from '@/features/settings/MissingTool'
import { AgentActivity } from '@/features/workstreams/AgentActivity'
import { WorkstreamChip } from '@/features/workstreams/WorkstreamChip'
import { useWorkstreamsSupported } from '@/features/workstreams/capabilities'
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
  autoFocus,
}: {
  workspaceId: string
  project: Project
  /** Take the keyboard when the window opens. */
  autoFocus: boolean
}): React.ReactElement {
  const restartSession = useBeacon((s) => s.restartSession)
  const missingClaude = useBeacon((s) => s.missing.find((entry) => entry.id === 'claude'))
  // On a Claude Code with the flags for it, restarting continues the
  // conversation instead of throwing it away — so the button says so. The old
  // word stays where the old behaviour does.
  const resumes = useWorkstreamsSupported()
  // Claude has one session per project; the slot exists for terminals.
  const epoch = useBeacon((s) => s.sessionEpoch[`${project.id}:claude:0`] ?? 0)
  const attachEpoch = useBeacon((s) => s.attachEpoch)

  return (
    <Panel
      id="claude"
      title="Claude"
      subtitle={project.displayPath}
      actions={
        <>
          <AgentActivity projectId={project.id} />
          <WorkstreamChip workspaceId={workspaceId} projectId={project.id} />
          <button
            type="button"
            className={styles['action']}
            title={
              resumes ? 'Start Claude again and carry on in the same workstream' : 'Restart Claude'
            }
            onClick={() => void restartSession(project.id, 'claude')}
          >
            {resumes ? 'Resume' : 'Restart'}
          </button>
        </>
      }
    >
      {missingClaude ? (
        <MissingTool requirement={missingClaude} />
      ) : (
        <TerminalView
          key={`${project.id}:${epoch}:${attachEpoch}`}
          workspaceId={workspaceId}
          projectId={project.id}
          kind="claude"
          slot={0}
          autoFocus={autoFocus}
        />
      )}
    </Panel>
  )
}
