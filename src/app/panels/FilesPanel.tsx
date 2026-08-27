import { FileTree } from '@/features/files/FileTree'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'

export function FilesPanel({
  workspaceId,
  project,
}: {
  workspaceId: string
  project: Project
}): React.ReactElement {
  return (
    <Panel id="files" title="Files" subtitle={project.name}>
      <FileTree key={project.id} workspaceId={workspaceId} projectId={project.id} />
    </Panel>
  )
}
