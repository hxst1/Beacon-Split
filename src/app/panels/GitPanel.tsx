import { GitPane } from '@/features/git/GitPane'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'

export function GitPanel({
  workspaceId,
  project,
}: {
  workspaceId: string
  project: Project
}): React.ReactElement {
  return (
    <Panel title="Git" subtitle={project.name}>
      <GitPane key={project.id} workspaceId={workspaceId} projectId={project.id} />
    </Panel>
  )
}
