import { EditorPane } from '@/features/editor/EditorPane'
import { useBeacon } from '@/app/store'
import { shortcutLabel } from '@/lib/platform'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'
import styles from './Panel.module.css'

export function EditorPanel({
  workspaceId,
  project,
  focused,
}: {
  workspaceId: string
  project: Project
  focused: boolean
}): React.ReactElement {
  const togglePanel = useBeacon((s) => s.togglePanel)

  return (
    <Panel
      title="Editor"
      focused={focused}
      actions={
        <button
          type="button"
          className={styles['close']}
          title={`Close the editor (${shortcutLabel('O')})`}
          aria-label="Close the editor"
          onClick={() => void togglePanel('editor')}
        >
          ✕
        </button>
      }
    >
      <EditorPane workspaceId={workspaceId} projectId={project.id} />
    </Panel>
  )
}
