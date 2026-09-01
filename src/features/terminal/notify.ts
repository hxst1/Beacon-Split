import { getCurrentWindow } from '@tauri-apps/api/window'

import { useBeacon } from '@/app/store'
import { ipc } from '@/ipc'
import type { ClaudeActivity } from '@/types/beacon'
import { watchActivity } from './sessionBridge'

/**
 * Tells you when a project needs an answer, or has finished a long turn, and
 * you are not looking at it.
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

/**
 * How long Claude must have been working for finishing to be worth a word.
 *
 * `Stop` fires at the end of every turn, and most turns are seconds long — you
 * were watching those. What is worth interrupting for is the turn you walked
 * away from, and length is the only honest proxy for that we have without
 * tracking where the user is looking.
 */
const WORTH_ANNOUNCING_MS = 30_000

/**
 * How long each project's current turn has been running.
 *
 * Timed from the first `working` report rather than from `SessionStart`, so an
 * idle session sitting open all morning does not count as a five-hour turn.
 * `waiting` deliberately does not stop the clock: a turn paused on a permission
 * prompt is still a turn you are waiting on.
 */
export function createTurnClock(now: () => number = Date.now) {
  const startedAt = new Map<string, number>()

  return {
    /**
     * Files an activity report.
     *
     * Returns how long the turn ran when it just ended and that is worth
     * announcing, and `null` in every other case — including a turn short
     * enough that you were plainly still at the keyboard for it.
     */
    saw(project: string, activity: ClaudeActivity): number | null {
      if (activity === 'working') {
        // Several tools in one turn means several reports; the turn began at
        // the first of them.
        if (!startedAt.has(project)) startedAt.set(project, now())
        return null
      }
      if (activity === 'waiting') return null

      const began = startedAt.get(project)
      startedAt.delete(project)
      if (activity !== 'done' || began === undefined) return null

      const ran = now() - began
      return ran >= WORTH_ANNOUNCING_MS ? ran : null
    },

    forget(project: string): void {
      startedAt.delete(project)
    },
  }
}

/** How long it ran, in the shortest form that is still precise enough. */
export function spellDuration(ms: number): string {
  const seconds = Math.round(ms / 1000)
  if (seconds < 60) return `${seconds}s`

  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) {
    const rest = seconds % 60
    return rest === 0 ? `${minutes}m` : `${minutes}m ${rest}s`
  }

  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`
}

/**
 * Whether macOS would actually deliver this.
 *
 * Asked rather than assumed, and never answered by raising the prompt: macOS
 * offers that prompt once per application, ever, so spending it silently on a
 * background event would burn the only chance the user has to say yes from
 * inside Beacon. Asking is the permission screen's job.
 */
async function allowed(): Promise<boolean> {
  const permission = await ipc.notificationPermission()
  return permission === 'authorized' || permission === 'provisional'
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

/**
 * Which project this is, said the way you hold it in your head.
 *
 * Workspace first, because with the same repository open in two workspaces the
 * project name alone does not tell you where to look.
 */
function nameOf(project: string): string {
  const snapshot = useBeacon.getState().snapshot
  for (const workspace of snapshot?.workspaces ?? []) {
    const found = workspace.projects.find((candidate) => candidate.id === project)
    if (found) return `${workspace.name} › ${found.name}`
  }
  return 'A project'
}

async function say(project: string, body: string): Promise<void> {
  if (!(await worthSaying(project)) || !(await allowed())) return
  // Clicking it activates Beacon, which macOS does for us because the
  // notification is filed under the app's own bundle identifier. Landing on
  // the right project would need a notification-response delegate, which is a
  // larger piece — see ADR-058.
  try {
    await ipc.sendNotification(nameOf(project), body)
  } catch (error) {
    // Never worth breaking a session over. The permission screen is where a
    // user finds out that notifications are not getting through.
    console.warn('could not post a notification', error)
  }
}

export function startNotifications(): () => void {
  const clock = createTurnClock()

  return watchActivity({
    onOutput: () => {},
    onExit: (project) => {
      announced.delete(project)
      clock.forget(project)
    },
    onClaudeActivity: ({ project, activity, detail }) => {
      const ran = clock.saw(project, activity)

      if (activity !== 'waiting') {
        // Anything else means it is no longer waiting, so the next wait is new.
        announced.delete(project)
        if (ran !== null) {
          void say(project, `Claude finished after ${spellDuration(ran)}`)
        }
        return
      }
      if (announced.has(project)) return
      announced.add(project)

      void say(
        project,
        detail ? `Claude is waiting to run ${detail}` : 'Claude is waiting for an answer',
      )
    },
  })
}
