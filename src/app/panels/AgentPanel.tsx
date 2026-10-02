import { LastReply } from '@/features/terminal/LastReply'
import { TerminalView } from '@/features/terminal/TerminalView'
import { MissingTool } from '@/features/settings/MissingTool'
import { AgentActivity } from '@/features/workstreams/AgentActivity'
import { WorkstreamChip } from '@/features/workstreams/WorkstreamChip'
import { useWorkstreamsSupported } from '@/features/workstreams/capabilities'
import { useBeacon } from '@/app/store'
import type { AgentKind, Project, SessionKind } from '@/types/beacon'
import { Panel } from './Panel'
import styles from './Panel.module.css'

/**
 * What each agent is called, where it runs, and what to say when it is not
 * installed.
 *
 * One table rather than branches through the component: everything that
 * differs between the agents differs in the same three ways, and a third agent
 * should be a row here.
 */
const AGENTS: Record<
  AgentKind,
  { panel: 'claude' | 'codex'; kind: SessionKind; title: string; requirement: string }
> = {
  claude: { panel: 'claude', kind: 'claude', title: 'Claude', requirement: 'claude' },
  codex: { panel: 'codex', kind: 'codex', title: 'Codex', requirement: 'codex' },
}

/**
 * The centre of the app: a real agent CLI in a PTY, one session per project.
 *
 * Beacon does not reimplement the agents. Colours, prompts, permissions,
 * selection and scrolling are whatever the CLI does, because it is the CLI.
 */
export function AgentPanel({
  agent,
  workspaceId,
  project,
  autoFocus,
}: {
  agent: AgentKind
  workspaceId: string
  project: Project
  /** Take the keyboard when the window opens. */
  autoFocus: boolean
}): React.ReactElement {
  const spec = AGENTS[agent]
  // A mode nobody can see is a mode that bites. With separate checkouts on,
  // this agent's work does not land in the directory the subtitle names — it
  // lands on a branch of its own — and the header is the only place somebody
  // would look before wondering where their changes went.
  const subtitle = project.agentWorktrees
    ? `${project.displayPath} · beacon/${agent}`
    : project.displayPath
  const restartSession = useBeacon((s) => s.restartSession)
  const showOnlyAgent = useBeacon((s) => s.showOnlyAgent)
  // The other agent, when there is one installed and it is not already on
  // screen. Offered here rather than only as a shortcut because wanting to see
  // one of them properly is the ordinary thing to want, and a panel that could
  // be the other one should say so.
  const other: AgentKind = agent === 'claude' ? 'codex' : 'claude'
  const otherSpec = AGENTS[other]
  const otherInstalled = useBeacon(
    (s) => !s.missing.some((entry) => entry.id === otherSpec.requirement),
  )
  const otherShown = useBeacon((s) => s.snapshot?.hidden.includes(otherSpec.panel) === false)
  const canSwap = otherInstalled && !otherShown
  const missing = useBeacon((s) => s.missing.find((entry) => entry.id === spec.requirement))
  // On a Claude Code with the flags for it, restarting continues the
  // conversation instead of throwing it away — so the button says so. The old
  // word stays where the old behaviour does.
  //
  // Asked only of Claude Code so far. Codex can resume, but Beacon has to be
  // told the conversation's id by a hook before there is anything to resume,
  // and that is not wired up yet — so the honest word here is the old one.
  const claudeResumes = useWorkstreamsSupported()
  const resumes = agent === 'claude' && claudeResumes
  // One session per project per agent; the slot exists for terminals.
  const epoch = useBeacon((s) => s.sessionEpoch[`${project.id}:${spec.kind}:0`] ?? 0)
  const attachEpoch = useBeacon((s) => s.attachEpoch)

  return (
    <Panel
      id={spec.panel}
      title={spec.title}
      subtitle={subtitle}
      actions={
        <>
          {/* Both of these read what Claude Code's hooks and status line
              report, and nothing reports for Codex yet. Absent is better than
              a chip showing the wrong conversation. */}
          {agent === 'claude' ? (
            <>
              <AgentActivity projectId={project.id} />
              <WorkstreamChip workspaceId={workspaceId} projectId={project.id} />
            </>
          ) : null}
          {canSwap ? (
            <button
              type="button"
              className={styles['action']}
              title={`Give this space to ${otherSpec.title} instead`}
              onClick={() => void showOnlyAgent(otherSpec.panel)}
            >
              {otherSpec.title}
            </button>
          ) : null}
          <button
            type="button"
            className={styles['action']}
            title={
              resumes
                ? 'Start Claude again and carry on in the same workstream'
                : `Restart ${spec.title}`
            }
            onClick={() => void restartSession(project.id, spec.kind)}
          >
            {resumes ? 'Resume' : 'Restart'}
          </button>
        </>
      }
    >
      {missing ? (
        <MissingTool requirement={missing} />
      ) : (
        <div className={styles['agentBody']}>
          {/* Fed by Claude Code's Stop hook; Codex has nothing reporting yet. */}
          {agent === 'claude' ? <LastReply projectId={project.id} /> : null}
          <div className={styles['agentTerminal']}>
            <TerminalView
              key={`${project.id}:${spec.kind}:${epoch}:${attachEpoch}`}
              workspaceId={workspaceId}
              projectId={project.id}
              kind={spec.kind}
              slot={0}
              autoFocus={autoFocus}
            />
          </div>
        </div>
      )}
    </Panel>
  )
}
