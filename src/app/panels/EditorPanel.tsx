import { EditorPane } from '@/features/editor/EditorPane'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'

export function EditorPanel({
  workspaceId,
  project,
}: {
  workspaceId: string
  project: Project
}): React.ReactElement {
  return (
    <Panel id="editor" title="Editor">
      <EditorPane workspaceId={workspaceId} projectId={project.id} />
    </Panel>
  )
}
