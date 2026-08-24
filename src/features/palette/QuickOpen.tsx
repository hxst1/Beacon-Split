import { useEffect, useMemo, useState } from 'react'

import { selectActiveProject, useBeacon } from '@/app/store'
import { useEditor } from '@/features/editor/openFiles'
import { errorMessage, ipc } from '@/ipc'
import { rank } from '@/lib/fuzzy'
import { Overlay } from './Overlay'
import type { OverlayItem } from './Overlay'

/**
 * Find a file in the current project and open it.
 *
 * The list is fetched when the palette opens rather than cached: a project's
 * files change constantly, and a stale list that cannot find the file you just
 * created is worse than a short wait.
 */
export function QuickOpen({ onClose }: { onClose: () => void }): React.ReactElement {
  const workspaceId = useBeacon((s) => s.snapshot?.activeWorkspace)
  const project = useBeacon(selectActiveProject)
  const openFile = useEditor((s) => s.open)
  const showPanel = useBeacon((s) => s.showPanel)

  const [query, setQuery] = useState('')
  const [files, setFiles] = useState<string[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!workspaceId || !project) return
    let cancelled = false

    ipc
      .listProjectFiles(workspaceId, project.id)
      .then((found) => {
        if (!cancelled) setFiles(found)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err))
      })

    return () => {
      cancelled = true
    }
  }, [workspaceId, project])

  const items: OverlayItem[] = useMemo(() => {
    if (!files) return []
    // The filename is what people search by; the directory is context.
    return rank(files, query, (path) => path, 80).map(({ item, match }) => {
      const cut = item.lastIndexOf('/')
      const name = cut === -1 ? item : item.slice(cut + 1)
      const shift = cut === -1 ? 0 : cut + 1
      return {
        id: item,
        label: name,
        positions: match.positions
          .filter((position) => position >= shift)
          .map((position) => position - shift),
        context: cut === -1 ? undefined : item.slice(0, cut),
      }
    })
  }, [files, query])

  return (
    <Overlay
      placeholder={project ? `Find a file in ${project.name}` : 'No project'}
      items={items}
      query={query}
      onQueryChange={setQuery}
      onClose={onClose}
      emptyMessage={error ?? (files === null ? 'Reading the project…' : 'No matches')}
      onChoose={(path) => {
        onClose()
        if (!workspaceId || !project) return
        void openFile(workspaceId, project.id, path)
        void showPanel('editor')
      }}
    />
  )
}
