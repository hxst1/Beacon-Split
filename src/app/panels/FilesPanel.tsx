import type { Project } from '@/types/beacon'
import { Panel } from './Panel'
import { Placeholder } from './Placeholder'

export function FilesPanel({ project }: { project: Project }): React.ReactElement {
  return (
    <Panel title="Files">
      <Placeholder
        milestone="Milestone 4"
        text="File tree, rename, duplicate, reveal, hidden files."
        detail={project.displayPath}
      />
    </Panel>
  )
}
