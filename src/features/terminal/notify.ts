import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { useBeacon } from '@/app/store'
import { watchActivity } from './sessionBridge'

/**
 * Tells you when a project needs an answer and you are not looking at it.
 *
 * This is the payoff of knowing what Claude is doing rather than guessing. With
 * three projects open, the expensive thing was never switching tabs — it was a
 * permission prompt in one of them going unseen for twenty minutes because
 * nothing said so.
 *
 * Deliberately narrow. A notification is an interruption, and an interruption
 * that was not worth it teaches people to dismiss the next one without reading.
 */

/** Projects already announced, so one wait is one notification. */
const announced = new Set<string>()

async function allowed(): Promise<boolean> {
  if (await isPermissionGranted()) return true
  // Asked only when there is something to say, not on startup: a permission
  // prompt before the application has done anything is a prompt about nothing.
  return (await requestPermission()) === 'granted'
}

/**
 * Whether this is worth interrupting for.
 *
 * Not if you are already looking at it: the tab is right there, pulsing. Not
 * if notifications are off.
 */
async function worthSaying(project: string): Promise<boolean> {
  const state = useBeacon.getState()
  if (!state.snapshot?.notifications) return false

  const workspace = state.snapshot.workspaces.find((w) => w.id === state.snapshot?.activeWorkspace)
  const showing = workspace ? state.snapshot.activeProject[workspace.id] : undefined

  if (showing !== project) return true
  // It is the project on screen — only worth saying if the window is not.
  return !(await getCurrentWindow().isFocused())
}

function nameOf(project: string): string {
  const snapshot = useBeacon.getState().snapshot
  for (const workspace of snapshot?.workspaces ?? []) {
    const found = workspace.projects.find((candidate) => candidate.id === project)
    if (found) return found.name
  }
  return 'A project'
}

export function startNotifications(): () => void {
  return watchActivity({
    onOutput: () => {},
    onExit: (project) => announced.delete(project),
    onClaudeActivity: ({ project, activity, detail }) => {
      if (activity !== 'waiting') {
        // Anything else means it is no longer waiting, so the next wait is new.
        announced.delete(project)
        return
      }
      if (announced.has(project)) return
      announced.add(project)

      void worthSaying(project).then(async (should) => {
        if (!should || !(await allowed())) return
        sendNotification({
          title: `${nameOf(project)} needs you`,
          body: detail ? `Claude is waiting to run ${detail}` : 'Claude is waiting for an answer',
        })
      })
    },
  })
}
