import { create } from 'zustand'

import { watchActivity } from './sessionBridge'
import type { Activity } from '@/types/beacon'

/** How long after its last output a project stops looking busy. */
const SETTLE_MS = 700

interface ActivityState {
  /**
   * Which project each live session belongs to.
   *
   * Keyed by session rather than counted per project: opening a session is
   * idempotent on the backend — a project that already has one gets it back —
   * so a counter would drift every time a panel remounted.
   */
  sessions: Record<string, string>
  /** Projects that produced output within the settle window. */
  busy: Record<string, true>

  sessionOpened: (sessionId: string, project: string) => void
  sessionClosed: (sessionId: string) => void
  projectStopped: (project: string) => void
}

/**
 * What each project appears to be doing.
 *
 * Derived from the session stream rather than guessed at: working if something
 * printed recently, idle if it has a live session but has gone quiet, stopped
 * if it has none. Nothing here inspects output — telling a dev server from an
 * error means understanding what was printed, which is honest work for a later
 * milestone rather than a regex now.
 */
export const useActivity = create<ActivityState>((set) => ({
  sessions: {},
  busy: {},

  sessionOpened: (sessionId, project) =>
    set((state) => ({ sessions: { ...state.sessions, [sessionId]: project } })),

  sessionClosed: (sessionId) =>
    set((state) => {
      const sessions = { ...state.sessions }
      delete sessions[sessionId]
      return { sessions }
    }),

  projectStopped: (project) =>
    set((state) => {
      const sessions = Object.fromEntries(
        Object.entries(state.sessions).filter(([, owner]) => owner !== project),
      )
      const busy = { ...state.busy }
      delete busy[project]
      return { sessions, busy }
    }),
}))

/**
 * Timers live outside the store: they are bookkeeping nobody renders, and
 * keeping them here means a burst of output does not churn the store.
 */
const settling = new Map<string, ReturnType<typeof setTimeout>>()

function markBusy(project: string): void {
  const existing = settling.get(project)
  if (existing) clearTimeout(existing)

  if (!useActivity.getState().busy[project]) {
    useActivity.setState((state) => ({ busy: { ...state.busy, [project]: true } }))
  }

  settling.set(
    project,
    setTimeout(() => {
      settling.delete(project)
      useActivity.setState((state) => {
        const busy = { ...state.busy }
        delete busy[project]
        return { busy }
      })
    }, SETTLE_MS),
  )
}

/** Subscribes for the lifetime of the window. */
watchActivity({
  onOutput: markBusy,
  onExit: (_project, sessionId) => {
    useActivity.getState().sessionClosed(sessionId)
  },
})

function derive(state: ActivityState, project: string): Activity {
  const running = Object.values(state.sessions).includes(project)
  if (!running) return 'stopped'
  return state.busy[project] ? 'working' : 'idle'
}

export function useProjectActivity(project: string): Activity {
  return useActivity((state) => derive(state, project))
}
