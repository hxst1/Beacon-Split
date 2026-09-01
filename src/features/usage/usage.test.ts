import { describe, expect, it } from 'vitest'

import type { UsageReport } from '@/types/beacon'
import {
  COLD_CACHE_TOKENS,
  adviceFor,
  cacheIsCold,
  contextHealth,
  healthLabel,
  levelOf,
  percent,
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
})
