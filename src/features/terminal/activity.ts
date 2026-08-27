import { create } from 'zustand'

import { watchActivity } from './sessionBridge'
import type { Activity, ClaudeActivity } from '@/types/beacon'

/** How long after its last output a project stops looking busy. */
const SETTLE_MS = 700

/**
 * How long a "working" report is believed without anything backing it up.
 *
 * A report is a fact about a moment, not a standing claim. If Claude Code stops
 * reporting — it was logged out mid-turn, it crashed, the hooks were removed —
 * the last thing it said would otherwise stand forever, and a tab claiming to
 * be working on something for the rest of the afternoon is worse than one that
 * admits it does not know.
 *
 * `waiting` and `done` are deliberately not expired: both are states a session
 * can legitimately sit in for hours, and both are still true if Claude stopped
 * because it needs you. What takes them back is Claude saying so — a session
 * that starts, resumes or is cleared reports `idle`.
 */
const WORKING_EXPIRY_MS = 2 * 60 * 1000

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

  /**
   * What Claude Code said, where it has said anything.
   *
   * Takes precedence over the output heuristic: a report is a fact, and
   * "something printed recently" is an inference from it at best.
   */
  reported: Record<string, { activity: ClaudeActivity; detail: string | null; at: number }>

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
  reported: {},

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
      const reported = { ...state.reported }
      delete busy[project]
      delete reported[project]
      return { sessions, busy, reported }
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

/** Starts listening. Called once by the application, not on import. */
export function startActivityTracking(): () => void {
  return watchActivity({
    onOutput: markBusy,
    onExit: (_project, sessionId) => {
      useActivity.getState().sessionClosed(sessionId)
    },
    onClaudeActivity: ({ project, activity, detail }) => {
      useActivity.setState((state) => {
        const reported = { ...state.reported }
        // Both drop the claim rather than replace it: a session that ended has
        // nothing to say, and one that just started or was cleared is saying
        // only that it is there, which is what inferring from output already
        // assumes.
        if (activity === 'ended' || activity === 'idle') delete reported[project]
        else reported[project] = { activity, detail, at: Date.now() }
        return { reported }
      })
    },
  })
}

function derive(state: ActivityState, project: string): Activity {
  const running = Object.values(state.sessions).includes(project)
  if (!running) return 'stopped'

  // What Claude said beats what we inferred from bytes arriving — while it is
  // still plausibly true.
  const reported = state.reported[project]
  if (reported) {
    if (reported.activity === 'waiting') return 'waiting'
    if (reported.activity === 'done') return state.busy[project] ? 'working' : 'done'

    if (reported.activity === 'working') {
      // Output means it really is working, whatever the age of the report.
      if (state.busy[project]) return 'working'
      if (Date.now() - reported.at < WORKING_EXPIRY_MS) return 'working'
      // Nothing said, nothing printed: fall back to inferring rather than
      // repeating a claim that has gone unsupported.
    }
  }

  return state.busy[project] ? 'working' : 'idle'
}

export function useProjectActivity(project: string): Activity {
  return useActivity((state) => derive(state, project))
}

/** What Claude is doing, for a tooltip — `Bash`, `Edit`, and so on. */
export function useProjectDetail(project: string): string | null {
  return useActivity((state) => state.reported[project]?.detail ?? null)
}
