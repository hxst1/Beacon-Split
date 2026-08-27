import { useState } from 'react'

import { TerminalView } from '@/features/terminal/TerminalView'
import { disposeFor } from '@/features/terminal/terminalHost'
import { useBeacon } from '@/app/store'
import { ipc } from '@/ipc'
import type { Project } from '@/types/beacon'
import { Panel } from './Panel'
import panelStyles from './Panel.module.css'
import styles from './TerminalPanel.module.css'

/**
 * A project's terminals.
 *
 * Several per project, because one is not enough the moment something is
 * running: a dev server holds a terminal, and needing to stop it to run a test
 * is the friction Beacon exists to remove. They are numbered rather than named
 * — a name is a thing to invent, and these are usually "the other one".
 *
 * Every terminal stays mounted while the panel is open, so switching between
 * them is instant and nothing loses its place. Only the visible one is shown.
 */
export function TerminalPanel({
  workspaceId,
  project,
}: {
  workspaceId: string
  project: Project
}): React.ReactElement {
  const restartSession = useBeacon((s) => s.restartSession)
  const attachEpoch = useBeacon((s) => s.attachEpoch)
  const epochs = useBeacon((s) => s.sessionEpoch)

  // Which slots this project has open, and which is showing. Not persisted:
  // the sessions themselves are, and reopening a project gives you one
  // terminal until you ask for another.
  const [slots, setSlots] = useState<number[]>([0])
  const [active, setActive] = useState(0)

  const add = (): void => {
    const next = Math.max(...slots) + 1
    setSlots([...slots, next])
    setActive(next)
  }

  const close = (slot: number): void => {
    const remaining = slots.filter((candidate) => candidate !== slot)
    setSlots(remaining)
    if (active === slot) setActive(remaining.at(-1) ?? 0)

    // The session ends with the tab: a terminal nobody can reach again is just
    // a process holding a directory open.
    void ipc.stopSessionSlot(project.id, slot)
    disposeFor(project.id, 'shell', slot)
  }

  return (
    <Panel
      id="terminal"
      title="Terminal"
      subtitle={project.displayPath}
      actions={
        <>
          <span className={styles['tabs']}>
            {slots.map((slot, index) => (
              <button
                key={slot}
                type="button"
                className={styles['tab']}
                data-active={slot === active}
                onClick={() => setActive(slot)}
              >
                {index + 1}
                {slots.length > 1 ? (
                  <span
                    className={styles['close']}
                    role="button"
                    aria-label={`Close terminal ${index + 1}`}
                    onClick={(event) => {
                      event.stopPropagation()
                      close(slot)
                    }}
                  >
                    ✕
                  </span>
                ) : null}
              </button>
            ))}
            <button type="button" className={styles['add']} title="New terminal" onClick={add}>
              +
            </button>
          </span>

          <button
            type="button"
            className={panelStyles['action']}
            title="Restart this terminal"
            onClick={() => void restartSession(project.id, 'shell', active)}
          >
            Restart
          </button>
        </>
      }
    >
      {slots.map((slot) => (
        <div
          key={slot}
          style={{ height: '100%', display: slot === active ? 'block' : 'none' }}
        >
          <TerminalView
            key={`${project.id}:${slot}:${epochs[`${project.id}:shell:${slot}`] ?? 0}:${attachEpoch}`}
            workspaceId={workspaceId}
            projectId={project.id}
            kind="shell"
            slot={slot}
          />
        </div>
      ))}
    </Panel>
  )
}
