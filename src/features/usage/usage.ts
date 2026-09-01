import { create } from 'zustand'

import { watchActivity } from '@/features/terminal/sessionBridge'
import { ipc } from '@/ipc'
import type { PromptCache, UsageReport } from '@/types/beacon'

/**
 * How long a usage report is presented as current.
 *
 * Claude Code reports through its status line, which only renders while a
 * session is alive and working. If it stops — logged out, crashed, the status
 * line removed — the last numbers would otherwise be shown as if they were
 * still true, and "62% left" that is quietly two hours old is worse than
 * nothing: it is exactly the number someone would plan around.
 */
export const STALE_AFTER_MS = 15 * 60 * 1000

interface Reported {
  report: UsageReport
  /** When this window heard it. */
  at: number
}

interface UsageState {
  /** The last report from each project. */
  byProject: Record<string, Reported>
  /** Which project reported most recently, for the account-wide numbers. */
  latest: string | null
}

/**
 * What sessions are costing, as Claude Code reports it through its status line.
 *
 * Nothing here is computed or estimated. Claude Code is the only thing that
 * knows how much of the five-hour allowance is gone, and it either says so or
 * it does not.
 */
export const useUsage = create<UsageState>(() => ({ byProject: {}, latest: null }))

function accept(report: UsageReport): void {
  useUsage.setState((state) => ({
    byProject: { ...state.byProject, [report.project]: { report, at: Date.now() } },
    latest: report.project,
  }))
}

/** Drops what a project reported, e.g. when its sessions are stopped. */
export function forgetUsage(project: string): void {
  useUsage.setState((state) => {
    const byProject = { ...state.byProject }
    delete byProject[project]
    return {
      byProject,
      latest: state.latest === project ? null : state.latest,
    }
  })
}

/** Whatever the daemon already knew, so an attaching window is not blank. */
export function loadUsage(): void {
  ipc
    .sessionUsage()
    .then((reports) => reports.forEach(accept))
    .catch(() => {
      // Not knowing what a session costs is not worth an error in the way.
    })
}

/**
 * Starts listening. Called once by the application rather than run on import.
 *
 * Subscribing as a side effect of being imported makes a module impossible to
 * use without a Tauri runtime — including from a test that only wants the
 * arithmetic below.
 */
export function startUsageTracking(): () => void {
  return watchActivity({
    onOutput: () => {},
    onExit: () => {},
    onUsage: accept,
    onReattached: loadUsage,
  })
}

/**
 * The rate limits, which belong to the account rather than to a project.
 *
 * Taken from whichever session reported most recently: they all see the same
 * allowance, and the newest report is the least stale.
 */
export function useAccountUsage(): Reported | null {
  return useUsage((state) => {
    const latest = state.latest ? state.byProject[state.latest] : undefined
    if (latest?.report.fiveHourUsedPercentage !== undefined) return latest

    // The most recent one may be from a session that never saw a rate limit.
    return (
      Object.values(state.byProject).find(
        (entry) => entry.report.fiveHourUsedPercentage !== undefined,
      ) ?? null
    )
  })
}

export function useProjectUsage(project: string): Reported | null {
  return useUsage((state) => state.byProject[project] ?? null)
}

/** Whether a report is old enough that it should not be read as current. */
export function isStale(reported: Reported | null, now: number): boolean {
  return reported === null || now - reported.at > STALE_AFTER_MS
}

/** `4 minutes ago`, for saying how old a number is rather than hiding it. */
export function howLongAgo(at: number, now: number): string {
  const minutes = Math.floor((now - at) / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  return `${Math.floor(minutes / 60)}h ago`
}

/** `2h 40m`, or `now` once the window has come round. */
export function untilReset(resetsAt: number | undefined, now: number): string | null {
  if (resetsAt === undefined) return null

  const seconds = resetsAt - Math.floor(now / 1000)
  if (seconds <= 0) return 'now'

  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`
}

/** How alarming a proportion used is, for colouring a bar. */
export function levelOf(usedPercentage: number): 'fine' | 'warn' | 'low' {
  if (usedPercentage >= 90) return 'low'
  if (usedPercentage >= 75) return 'warn'
  return 'fine'
}

/** Clamped and rounded, since a gauge past its ends is a bug not a value. */
export function percent(value: number | undefined): number | null {
  if (value === undefined) return null
  return Math.max(0, Math.min(100, Math.round(value)))
}

/**
 * How full the context is, in terms of what you would do about it.
 *
 * Bands rather than a bare number because the number on its own asks the reader
 * to hold a threshold in their head. The upper two are the same 75 and 90 the
 * allowance gauge uses: one vocabulary for "getting close" and "nearly gone",
 * whichever meter is being read.
 */
export type ContextHealth = 'healthy' | 'growing' | 'high' | 'critical'

export function contextHealth(usedPercentage: number): ContextHealth {
  if (usedPercentage >= 90) return 'critical'
  if (usedPercentage >= 75) return 'high'
  if (usedPercentage >= 50) return 'growing'
  return 'healthy'
}

/** The band in words, for a reader rather than a stylesheet. */
export function healthLabel(health: ContextHealth): string {
  switch (health) {
    case 'healthy':
      return 'healthy'
    case 'growing':
      return 'growing'
    case 'high':
      return 'getting full'
    case 'critical':
      return 'almost full'
  }
}

/** Whether the cache is known to be cold, as opposed to not known at all. */
export function cacheIsCold(cache: PromptCache | undefined, now: number): boolean {
  if (cache?.warm !== true) return cache?.warm === false
  // Warm, but only until it expires — and Claude Code re-runs the status line
  // at that moment, so this is what the last report meant by the time it is
  // read rather than a guess about the future.
  return cache.expiresAt !== undefined && cache.expiresAt * 1000 <= now
}

/**
 * Something worth saying about a conversation, or nothing.
 *
 * At most one at a time, and never acted on. Beacon does not compact, does not
 * clear, and does not start a session on its own: the whole value here is
 * telling someone what a number means at the moment it starts to matter, and an
 * application that acts on its own advice would be making the decision that
 * this exists to inform.
 */
export interface Advice {
  /** Stable, so dismissing one does not dismiss the next. */
  id: 'room-running-out' | 'cold-context' | 'growing'
  title: string
  detail: string
}

/**
 * How much cache rebuilding has to be on the table before it is worth a word.
 *
 * Below this the advice would be true and not worth the interruption, which is
 * the failure mode this whole surface has to avoid.
 */
export const COLD_CACHE_TOKENS = 20_000

export function adviceFor(report: UsageReport | undefined, now: number): Advice | null {
  if (!report) return null
  const used = report.contextUsedPercentage

  if (used !== undefined && used >= 90) {
    return {
      id: 'room-running-out',
      title: 'Almost full',
      detail:
        'Start a clean workstream if you are moving on, or compact if you need this conversation to remember what it has done.',
    }
  }

  const rebuild = report.promptCache?.recacheTokensIfCold ?? 0
  if (cacheIsCold(report.promptCache, now) && rebuild >= COLD_CACHE_TOKENS) {
    return {
      id: 'cold-context',
      title: 'Large cold context',
      detail: `The next turn rebuilds about ${thousands(rebuild)} tokens of cache. A clean workstream would not.`,
    }
  }

  if (used !== undefined && used >= 75) {
    return {
      id: 'growing',
      title: 'Getting full',
      detail: 'If you are moving on to something else, a clean workstream starts with the room back.',
    }
  }

  return null
}

/** `45,000` — easier to size up at a glance than a bare number. */
export function thousands(value: number): string {
  return value.toLocaleString('en-US')
}
