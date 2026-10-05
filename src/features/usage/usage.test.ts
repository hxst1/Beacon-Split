import { describe, expect, it } from 'vitest'

import type { UsageReport } from '@/types/beacon'
import {
  COLD_CACHE_TOKENS,
  adviceFor,
  cacheIsCold,
  contextHealth,
  hasReset,
  healthLabel,
  isStale,
  leftInWindow,
  levelOf,
  newerLimits,
  percent,
  reported,
  staleness,
  untilReset,
} from './usage'

const NOW = 1_800_000_000_000

function report(over: Partial<UsageReport> = {}): UsageReport {
  return { project: 'pj_x', ...over }
}

describe('contextHealth', () => {
  it('names the bands by what you would do about them', () => {
    expect(contextHealth(0)).toBe('healthy')
    expect(contextHealth(49)).toBe('healthy')
    expect(contextHealth(50)).toBe('growing')
    expect(contextHealth(74)).toBe('growing')
    expect(contextHealth(75)).toBe('high')
    expect(contextHealth(89)).toBe('high')
    expect(contextHealth(90)).toBe('critical')
    expect(contextHealth(100)).toBe('critical')
  })

  it('says the band in words a reader can act on', () => {
    expect(healthLabel(contextHealth(20))).toBe('healthy')
    expect(healthLabel(contextHealth(60))).toBe('growing')
    expect(healthLabel(contextHealth(80))).toBe('getting full')
    expect(healthLabel(contextHealth(95))).toBe('almost full')
  })

  it('shares its upper boundaries with the allowance gauge', () => {
    // One vocabulary for "getting close" and "nearly gone", whichever meter is
    // being read. If these drift apart the same number means two things.
    expect(levelOf(75)).toBe('warn')
    expect(levelOf(90)).toBe('low')
  })
})

describe('cacheIsCold', () => {
  it('is not cold when nothing has been said about the cache', () => {
    // Claude Code leaves the block out until there has been an API response.
    // Reading absence as cold would advise a clean workstream on every session
    // before its first turn.
    expect(cacheIsCold(undefined, NOW)).toBe(false)
    expect(cacheIsCold({}, NOW)).toBe(false)
  })

  it('is cold when Claude Code says so', () => {
    expect(cacheIsCold({ warm: false }, NOW)).toBe(true)
  })

  it('is cold once a warm cache has passed its expiry', () => {
    expect(cacheIsCold({ warm: true, expiresAt: NOW / 1000 + 600 }, NOW)).toBe(false)
    expect(cacheIsCold({ warm: true, expiresAt: NOW / 1000 - 1 }, NOW)).toBe(true)
  })

  it('is not cold when a warm cache never said when it expires', () => {
    expect(cacheIsCold({ warm: true }, NOW)).toBe(false)
  })
})

describe('adviceFor', () => {
  it('says nothing about a session it knows nothing about', () => {
    expect(adviceFor(undefined, NOW)).toBeNull()
    expect(adviceFor(report(), NOW)).toBeNull()
  })

  it('says nothing while there is room and the cache is warm', () => {
    expect(
      adviceFor(
        report({ contextUsedPercentage: 38, promptCache: { warm: true } }),
        NOW,
      ),
    ).toBeNull()
  })

  it('offers both ways out when the window is nearly full', () => {
    // Both, not one: Beacon does not know whether the next thing is the same
    // piece of work, and that is what decides between them.
    const advice = adviceFor(report({ contextUsedPercentage: 93 }), NOW)
    expect(advice?.id).toBe('room-running-out')
    expect(advice?.detail).toContain('clean workstream')
    expect(advice?.detail).toContain('compact')
  })

  it('warns about a cold cache with the number that makes it matter', () => {
    const advice = adviceFor(
      report({
        contextUsedPercentage: 60,
        promptCache: { warm: false, recacheTokensIfCold: 45_000 },
      }),
      NOW,
    )
    expect(advice?.id).toBe('cold-context')
    expect(advice?.detail).toContain('45,000')
  })

  it('stays quiet about a cold cache that costs almost nothing to rebuild', () => {
    // True, and not worth the interruption. Showing it would be the failure
    // this surface exists to avoid.
    expect(
      adviceFor(
        report({
          contextUsedPercentage: 60,
          promptCache: { warm: false, recacheTokensIfCold: COLD_CACHE_TOKENS - 1 },
        }),
        NOW,
      ),
    ).toBeNull()
  })

  it('puts running out of room ahead of a cold cache', () => {
    const advice = adviceFor(
      report({
        contextUsedPercentage: 95,
        promptCache: { warm: false, recacheTokensIfCold: 200_000 },
      }),
      NOW,
    )
    expect(advice?.id).toBe('room-running-out')
  })

  it('suggests a clean workstream once the window is getting full', () => {
    const advice = adviceFor(report({ contextUsedPercentage: 80 }), NOW)
    expect(advice?.id).toBe('growing')
  })

  it('never suggests doing anything by itself', () => {
    // The whole surface is information. An application that acted on its own
    // advice would be making the decision this exists to inform.
    for (const used of [0, 50, 76, 91]) {
      const advice = adviceFor(
        report({
          contextUsedPercentage: used,
          promptCache: { warm: false, recacheTokensIfCold: 90_000 },
        }),
        NOW,
      )
      expect(advice?.detail ?? '').not.toMatch(/beacon (will|has)/i)
    }
  })
})

describe('the numbers a gauge is drawn from', () => {
  it('clamps a percentage rather than drawing past the ends', () => {
    expect(percent(-4)).toBe(0)
    expect(percent(140)).toBe(100)
    expect(percent(undefined)).toBeNull()
  })

  it('says when a window comes back, and says `now` once it has', () => {
    expect(untilReset(NOW / 1000 + 3600 * 2 + 60 * 40, NOW)).toBe('2h 40m')
    expect(untilReset(NOW / 1000 - 5, NOW)).toBe('now')
    expect(untilReset(undefined, NOW)).toBeNull()
  })

  it('shows what is left of a window, and nothing once it has come round', () => {
    expect(leftInWindow(67, NOW / 1000 + 600, NOW)).toBe(33)
    expect(leftInWindow(67, undefined, NOW)).toBe(33)
    // The old window's 67% says nothing about the new one.
    expect(leftInWindow(67, NOW / 1000 - 1, NOW)).toBeNull()
    expect(hasReset(NOW / 1000 - 1, NOW)).toBe(true)
    expect(leftInWindow(undefined, NOW / 1000 + 600, NOW)).toBeNull()
  })
})

describe('how old the numbers are', () => {
  const HOUR = 3_600_000

  it('dates a report by when the daemon heard it, not by when the window did', () => {
    // What the daemon held since the morning, replayed to a window opened now.
    const replayed = reported(report({ reportedAt: NOW - 3 * HOUR, limitsSeenAt: NOW - 3 * HOUR }), NOW)
    expect(replayed.at).toBe(NOW - 3 * HOUR)
    expect(isStale(replayed.limitsAt, NOW)).toBe(true)
  })

  it('keeps the limits as old as the response that brought them', () => {
    const repeated = reported(report({ reportedAt: NOW, limitsSeenAt: NOW - 2 * HOUR }), NOW)
    expect(isStale(repeated.at, NOW)).toBe(false)
    expect(isStale(repeated.limitsAt, NOW)).toBe(true)
  })

  it('falls back to the window’s own clock for an older daemon', () => {
    const old = reported(report(), NOW)
    expect(old.at).toBe(NOW)
    expect(old.limitsAt).toBe(NOW)
  })

  it('takes the allowance from the newest response, not from whoever spoke last', () => {
    // Two sessions in the same project: the idle one repeats after the
    // working one, and becomes the project's last report.
    const working = reported(
      report({ sessionId: 's1', fiveHourUsedPercentage: 92, reportedAt: NOW - 60_000, limitsSeenAt: NOW - 60_000 }),
      NOW,
    )
    const idle = reported(
      report({ sessionId: 's2', fiveHourUsedPercentage: 67, reportedAt: NOW, limitsSeenAt: NOW - 3 * HOUR }),
      NOW,
    )
    expect(newerLimits(newerLimits(null, working), idle)?.report.fiveHourUsedPercentage).toBe(92)
    expect(newerLimits(newerLimits(null, idle), working)?.report.fiveHourUsedPercentage).toBe(92)
  })

  it('after a restart, the window decides and not who spoke last', () => {
    // Both dated on arrival by a daemon that knows neither conversation, and
    // the idle one, repeating 67% from hours ago, arrives last.
    const working = reported(
      report({ sessionId: 's1', fiveHourUsedPercentage: 92, fiveHourResetsAt: 1_800_000_000, reportedAt: NOW - 60_000, limitsSeenAt: NOW - 60_000 }),
      NOW,
    )
    const idle = reported(
      report({ sessionId: 's2', fiveHourUsedPercentage: 67, fiveHourResetsAt: 1_800_000_000, reportedAt: NOW, limitsSeenAt: NOW }),
      NOW,
    )
    expect(newerLimits(working, idle)).toBe(working)
    expect(newerLimits(idle, working)).toBe(working)
  })

  it('takes a later window whatever it has used, and a few seconds off is the same one', () => {
    const old = reported(report({ fiveHourUsedPercentage: 92, fiveHourResetsAt: 1_800_000_000, limitsSeenAt: NOW }), NOW)
    const fresh = reported(
      report({ fiveHourUsedPercentage: 5, fiveHourResetsAt: 1_800_000_000 + 5 * 3600, limitsSeenAt: NOW - HOUR }),
      NOW,
    )
    expect(newerLimits(old, fresh)).toBe(fresh)
    expect(newerLimits(fresh, old)).toBe(fresh)

    const nearly = reported(report({ fiveHourUsedPercentage: 30, fiveHourResetsAt: 1_800_000_020, limitsSeenAt: NOW + 1 }), NOW)
    expect(newerLimits(old, nearly)).toBe(old)
  })

  it('dims the allowance and the context each on its own', () => {
    // The allowance came from another project hours ago; this project's
    // context is ten seconds old.
    const account = reported(report({ fiveHourUsedPercentage: 40, reportedAt: NOW - 3 * HOUR, limitsSeenAt: NOW - 3 * HOUR }), NOW)
    const project = reported(report({ reportedAt: NOW - 10_000, limitsSeenAt: NOW - 10_000 }), NOW)
    expect(staleness(account, project, NOW)).toEqual({ limits: true, context: false, all: false })

    // Everything old: the whole meter dims.
    const oldProject = reported(report({ reportedAt: NOW - HOUR, limitsSeenAt: NOW - HOUR }), NOW)
    expect(staleness(account, oldProject, NOW).all).toBe(true)
    // Nothing reported for one of them is not the same as fresh.
    expect(staleness(null, oldProject, NOW)).toEqual({ limits: false, context: true, all: true })
    expect(staleness(null, project, NOW).all).toBe(false)
  })

  it('never lets a report with no allowance in it take the place of one', () => {
    const none = reported(report({ reportedAt: NOW, limitsSeenAt: NOW }), NOW)
    expect(newerLimits(null, none)).toBeNull()

    const some = reported(report({ fiveHourUsedPercentage: 40, limitsSeenAt: NOW - HOUR }), NOW)
    expect(newerLimits(some, none)).toBe(some)
  })
})
