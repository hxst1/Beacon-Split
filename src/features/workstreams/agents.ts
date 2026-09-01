import { create } from 'zustand'

import { watchActivity } from '@/features/terminal/sessionBridge'
import type { AgentActivity } from '@/types/beacon'

/**
 * How long a finished agent stays on screen.
 *
 * Long enough to read what it found, short enough that the header goes back to
 * being about the session. Nothing here is persisted: an agent that ran for
 * twelve seconds is worth seeing while it runs and worth nothing afterwards.
 */
export const LINGER_MS = 6_000

export interface RunningAgent {
  agent: string
  /** Absent when Claude Code did not say which agent it was. */
  agentType?: string
  /** When this window heard it start, for the elapsed clock. */
  startedAt: number
  /** Set once it has stopped; absent while it runs. */
  finishedAt?: number
  summary?: string
}

interface AgentState {
  /** What is running, or has just finished, per project. */
  byProject: Record<string, RunningAgent[]>
}

export const useAgents = create<AgentState>(() => ({ byProject: {} }))

/**
 * Folds a start or a stop into what a project is showing.
 *
 * Pure, and separate from the store, because the interesting parts are the
 * pairing and the forgetting and both are worth testing without a window.
 */
export function reduceAgents(
  current: RunningAgent[],
  report: AgentActivity,
  now: number,
): RunningAgent[] {
  const rest = current.filter((held) => held.agent !== report.agent)

  if (report.running) {
    const started: RunningAgent = { agent: report.agent, startedAt: now }
    if (report.agentType !== undefined) started.agentType = report.agentType
    return [...rest, started]
  }

  // A stop for an agent this window never saw start — it was opened mid-run, or
  // the start was missed. Still worth showing, dated from now so the elapsed
  // time is honest about being unknown rather than wrong.
  const held = current.find((one) => one.agent === report.agent)
  const finished: RunningAgent = {
    agent: report.agent,
    startedAt: held?.startedAt ?? now,
    finishedAt: now,
  }
  const agentType = report.agentType ?? held?.agentType
  if (agentType !== undefined) finished.agentType = agentType
  if (report.summary !== undefined) finished.summary = report.summary

  return [...rest, finished]
}

/** Drops what has been finished long enough to have been read. */
export function forgetFinished(current: RunningAgent[], now: number): RunningAgent[] {
  return current.filter((one) => one.finishedAt === undefined || now - one.finishedAt < LINGER_MS)
}

/** Starts listening. Called once by the application, not on import. */
export function startAgentTracking(): () => void {
  const sweep = window.setInterval(() => {
    const now = Date.now()
    useAgents.setState((state) => {
      let changed = false
      const byProject: Record<string, RunningAgent[]> = {}

      for (const [project, held] of Object.entries(state.byProject)) {
        const kept = forgetFinished(held, now)
        if (kept.length !== held.length) changed = true
        if (kept.length > 0) byProject[project] = kept
      }
      // A new object every tick would re-render every header twice a second.
      return changed ? { byProject } : state
    })
  }, 1_000)

  const stop = watchActivity({
    onOutput: () => {},
    onExit: () => {},
    onAgent: (report) => {
      useAgents.setState((state) => ({
        byProject: {
          ...state.byProject,
          [report.project]: reduceAgents(state.byProject[report.project] ?? [], report, Date.now()),
        },
      }))
    },
  })

  return () => {
    window.clearInterval(sweep)
    stop()
  }
}

const NONE: RunningAgent[] = []

export function useProjectAgents(project: string): RunningAgent[] {
  return useAgents((state) => state.byProject[project] ?? NONE)
}

/** What to call an agent Claude Code did not name. */
export function agentLabel(agent: RunningAgent): string {
  return agent.agentType?.replace(/^beacon-/, '') ?? 'agent'
}

/** `8s`, `1m 20s` — how long it has been going, or how long it took. */
export function elapsed(agent: RunningAgent, now: number): string {
  const seconds = Math.max(0, Math.round(((agent.finishedAt ?? now) - agent.startedAt) / 1000))
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}
