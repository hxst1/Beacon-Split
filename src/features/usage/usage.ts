import { create } from 'zustand'

import { watchActivity } from '@/features/terminal/sessionBridge'
import { ipc } from '@/ipc'
import type { AgentKind, PromptCache, UsageReport } from '@/types/beacon'

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

export interface Reported {
  report: UsageReport
  /** When the daemon heard it. */
  at: number
  /** When its rate limits were last new, which a repeat leaves behind `at`. */
  limitsAt: number
}

/**
 * A project's report about one agent.
 *
 * Keyed by both because two agents can be working in one project and their
 * numbers are not comparable: a context window belongs to a conversation, and
 * there is a conversation per agent.
 */
function keyOf(project: string, agent: AgentKind): string {
  return `${project}:${agent}`
}

/** Claude, for a report from a daemon that predates Codex having one. */
function agentOf(report: UsageReport): AgentKind {
  return report.agent ?? 'claude'
}

interface UsageState {
  /** The last report from each project, per agent. */
  byAgent: Record<string, Reported>
  /**
   * The report with the newest rate limits, per agent.
   *
   * Per agent because the accounts are different accounts: Claude's five-hour
   * window is Anthropic's and Codex's is OpenAI's. Letting them share a slot
   * would show whichever reported last as if it were the allowance being
   * spent.
   */
  limits: Partial<Record<AgentKind, Reported>>
}

/**
 * What sessions are costing, as Claude Code reports it through its status line.
 *
 * Nothing here is computed or estimated. Claude Code is the only thing that
 * knows how much of the five-hour allowance is gone, and it either says so or
 * it does not.
 */
export const useUsage = create<UsageState>(() => ({ byAgent: {}, limits: {} }))

/**
 * Dates a report by when the daemon heard it, not by when this window did.
 *
 * The daemon outlives the window and replays what it holds to one that
 * attaches, so "when this window heard it" made a number from the morning look
 * new every time Beacon was opened. Only a daemon from before the times were
 * sent leaves the window to guess, and then it guesses as it always did.
 */
export function reported(report: UsageReport, heardAt: number): Reported {
  const at = report.reportedAt ?? heardAt
  return { report, at, limitsAt: report.limitsSeenAt ?? at }
}

function accept(report: UsageReport): void {
  const entry = reported(report, Date.now())
  const agent = agentOf(report)
  useUsage.setState((state) => ({
    byAgent: { ...state.byAgent, [keyOf(report.project, agent)]: entry },
    limits: { ...state.limits, [agent]: newerLimits(state.limits[agent] ?? null, entry) },
  }))
}

/**
 * `accept`, for a test that wants to put a report in without a daemon.
 *
 * Exported rather than exercised through the event stream: what is worth
 * pinning here is how two agents' reports are filed, and that is this function
 * and not the transport under it.
 */
export const acceptForTests = accept

/**
 * Drops what a project reported, e.g. when its sessions are stopped.
 *
 * The account's limits stay: they are still the newest word on the allowance,
 * dated, whichever project brought them.
 */
export function forgetUsage(project: string): void {
  useUsage.setState((state) => {
    const byAgent = { ...state.byAgent }
    // Every agent's report for this project, since the key carries both.
    for (const key of Object.keys(byAgent)) {
      if (key.startsWith(`${project}:`)) delete byAgent[key]
    }
    return { byAgent }
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
 * How far apart two reset times, in seconds, can be and still be the same
 * five-hour window. Claude Code reports a fixed time, but nothing promises it
 * to the second.
 */
const SAME_WINDOW_S = 60

/**
 * The rate limits, which belong to the account rather than to a project.
 *
 * Every session sees the same allowance, so the one to believe is whichever
 * heard about it last — not whichever session spoke last. An idle session
 * repeating its old numbers speaks often, and in a project with two sessions it
 * is that project's last report as often as not; so the limits are kept apart,
 * and only newer ones replace them.
 *
 * The numbers are asked before the dates, because they cannot be wrong about
 * their own order the way a date can: a window that resets later is a later
 * window, and within one window the share used only goes up, so the lower
 * figure is the older one. Only when both say the same is it down to when they
 * were new. `has_newer_limits_than` in beacon-core decides the same way.
 */
export function newerLimits(current: Reported | null, incoming: Reported): Reported | null {
  const used = incoming.report.fiveHourUsedPercentage
  if (used === undefined) return current
  if (current === null) return incoming
  const theirs = current.report.fiveHourUsedPercentage
  if (theirs === undefined) return incoming

  const mine = incoming.report.fiveHourResetsAt
  const theirReset = current.report.fiveHourResetsAt
  if (mine !== undefined && theirReset !== undefined) {
    if (Math.abs(mine - theirReset) > SAME_WINDOW_S) return mine > theirReset ? incoming : current
    if (used !== theirs) return used > theirs ? incoming : current
  }
  return incoming.limitsAt >= current.limitsAt ? incoming : current
}

export function useAccountUsage(agent: AgentKind): Reported | null {
  return useUsage((state) => state.limits[agent] ?? null)
}

export function useProjectUsage(project: string, agent: AgentKind): Reported | null {
  return useUsage((state) => state.byAgent[keyOf(project, agent)] ?? null)
}

/**
 * Which of the meter's two numbers are too old to read as current.
 *
 * The allowance is as old as the response that brought it — whichever project
 * that was — and the context is the project's own, so one being old says
 * nothing about the other. `all` is when everything the meter shows is old,
 * which is when the meter as a whole dims.
 */
export function staleness(
  account: Reported | null,
  project: Reported | null,
  now: number,
): { limits: boolean; context: boolean; all: boolean } {
  const limits = account !== null && isStale(account.limitsAt, now)
  const context = project !== null && isStale(project.at, now)
  return { limits, context, all: (account === null || limits) && (project === null || context) }
}

/** Whether something said at `at` is old enough not to be read as current. */
export function isStale(at: number | undefined, now: number): boolean {
  return at === undefined || now - at > STALE_AFTER_MS
}

/**
 * How much of a rate-limit window is left, or nothing once it has come round.
 *
 * Past `resetsAt` the percentage describes a window that is over: the new one
 * started empty, and Claude Code says how much of it is gone only with its
 * next response. Showing the old number there is the same mistake as showing
 * an old number as new — so it is not shown at all.
 */
export function leftInWindow(
  used: number | undefined,
  resetsAt: number | undefined,
  now: number,
): number | null {
  if (hasReset(resetsAt, now)) return null
  const clamped = percent(used)
  return clamped === null ? null : 100 - clamped
}

/** Whether a window's reset time, in Unix seconds, has passed. */
export function hasReset(resetsAt: number | undefined, now: number): boolean {
  return resetsAt !== undefined && resetsAt * 1000 <= now
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
