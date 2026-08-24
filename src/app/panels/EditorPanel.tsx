import { EditorPane } from '@/features/editor/EditorPane'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'

export function EditorPanel({
  workspaceId,
  project,
  focused,
}: {
  workspaceId: string
  project: Project
  focused: boolean
}): React.ReactElement {
  return (
    <Panel title="Editor" focused={focused}>
      <EditorPane workspaceId={workspaceId} projectId={project.id} />
    </Panel>
  )
}
