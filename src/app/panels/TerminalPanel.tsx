import type { Project } from '@/types/beacon'
import { Panel } from './Panel'
import { Placeholder } from './Placeholder'

export function TerminalPanel({
  project,
  focused,
}: {
  project: Project
  focused: boolean
}): React.ReactElement {
  return (
    <Panel title="Terminal" subtitle={project.displayPath} focused={focused}>
      <Placeholder
        milestone="Milestone 2"
        text="A real shell for this project, opened at its root."
        detail={`$ cd ${project.absolutePath}`}
      />
    </Panel>
  )
}
