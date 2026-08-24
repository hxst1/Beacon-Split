import { useMemo, useState } from 'react'

import { buildCommands } from '@/app/commands'
import { rank } from '@/lib/fuzzy'
import { Overlay } from './Overlay'
import type { OverlayItem } from './Overlay'

/**
 * Everything Beacon can do, filtered as you type.
 *
 * The list is built when the palette opens rather than kept around, so it
 * always reflects the current project, workspace and panel state — "Hide Files"
 * or "Show Files" depending on which one is true right now.
 */
export function CommandPalette({ onClose }: { onClose: () => void }): React.ReactElement {
  const [query, setQuery] = useState('')
  const commands = useMemo(() => buildCommands(), [])

  const items: OverlayItem[] = useMemo(
    () =>
      rank(commands, query, (command) => command.title, 60).map(({ item, match }) => ({
        id: item.id,
        label: item.title,
        positions: match.positions,
        context: item.group,
        hint: item.hint,
      })),
    [commands, query],
  )

  return (
    <Overlay
      placeholder="Run a command"
      items={items}
      query={query}
      onQueryChange={setQuery}
      onClose={onClose}
      onChoose={(id) => {
        const command = commands.find((candidate) => candidate.id === id)
        onClose()
        void command?.run()
      }}
    />
  )
}
