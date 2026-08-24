import { create } from 'zustand'

import { watchActivity } from '@/features/terminal/sessionBridge'
import { ipc } from '@/ipc'
import type { UsageReport } from '@/types/beacon'

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
