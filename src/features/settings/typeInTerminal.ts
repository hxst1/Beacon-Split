import { focusPanel } from '@/app/panelFocus'
import { selectActiveProject, useBeacon } from '@/app/store'
import { ipc } from '@/ipc'

/**
 * Puts a command on the terminal's command line, without running it.
 *
 * The gap this closes is the one that loses people: being shown a command and
 * having to find a terminal, paste it and come back. Beacon *is* the terminal
 * here, so the install command can go where it belongs with one press.
 *
 * It stops short of pressing Return, and that is the whole design. Beacon does
 * not run installers: an installer is a program somebody else wrote, fetched
 * over the network, and a Beacon that ran it would own every way it can go
 * wrong on a machine it cannot see — half-installed, a `sudo` prompt nobody
 * expected, a PATH that will not take effect until the next shell. What it can
 * do is take out the part that was tedious and leave the part that is a
 * decision. The command is sitting there, readable, and one key away.
 */
export function useTypeInTerminal(): {
  /** Null when there is nowhere to type: a terminal belongs to a project. */
  type: ((command: string) => void) | null
} {
  const workspaceId = useBeacon((s) => s.snapshot?.activeWorkspace)
  const project = useBeacon(selectActiveProject)
  const showPanel = useBeacon((s) => s.showPanel)
  const setOverlay = useBeacon((s) => s.setOverlay)

  if (!workspaceId || !project) return { type: null }

  const type = (command: string): void => {
    void (async () => {
      // Out of the way first, so what happens next is visible. A command typed
      // behind a settings screen is a command nobody saw arrive.
      setOverlay(null)
      await showPanel('terminal')

      // `openSession` is an ensure: the project's shell if it has one, a new
      // one if it does not. The size is only used when there was none, and the
      // panel corrects it the moment it draws.
      const session = await ipc.openSession(workspaceId, project.id, 'shell', 0, 80, 24)
      await ipc.writeSession(session.id, command)

      // The keyboard goes where the command is, because the next thing to
      // happen is somebody reading it and pressing Return.
      focusPanel('terminal')
    })()
  }

  return { type }
}
