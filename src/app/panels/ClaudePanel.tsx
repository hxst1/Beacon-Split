import type { Project } from '@/types/beacon'
import { Panel } from './Panel'
import { Placeholder } from './Placeholder'

/**
 * The centre of the app. From Milestone 3 this hosts a real `claude` process in
 * a PTY, one per project, rendered with xterm.js — Beacon never reimplements
 * Claude Code, it runs it.
 */
export function ClaudePanel({
  project,
  focused,
}: {
  project: Project
  focused: boolean
}): React.ReactElement {
  return (
    <Panel title="Claude" subtitle={project.displayPath} focused={focused}>
      <Placeholder
        milestone="Milestone 3"
        text="A live claude session for this project, in its own PTY."
        detail={`$ cd ${project.absolutePath} && claude`}
      />
    </Panel>
  )
}
