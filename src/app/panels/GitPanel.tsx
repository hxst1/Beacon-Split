import { Panel } from './Panel'
import { Placeholder } from './Placeholder'

export function GitPanel(): React.ReactElement {
  return (
    <Panel title="Git">
      <Placeholder
        milestone="Milestone 5"
        text="Status, branch, diff, stage and commit — driven by the git CLI."
      />
    </Panel>
  )
}
