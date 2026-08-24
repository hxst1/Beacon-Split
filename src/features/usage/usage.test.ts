import { describe, expect, it } from 'vitest'

import { STALE_AFTER_MS, howLongAgo, isStale, levelOf, percent, untilReset } from './usage'
import type { UsageReport } from '@/types/beacon'

const report: UsageReport = { project: 'pj_x' }
const at = (age: number): { report: UsageReport; at: number } => ({ report, at: Date.now() - age })

describe('staleness', () => {
  it('treats a fresh report as current', () => {
    expect(isStale(at(60_000), Date.now())).toBe(false)
  })

  it('stops believing a report that has gone quiet', () => {
    // Claude Code logged the session out, or the status line was removed. The
    // number it left behind is exactly what someone would plan around.
    expect(isStale(at(STALE_AFTER_MS + 1000), Date.now())).toBe(true)
  })

  it('treats nothing at all as stale rather than as zero', () => {
    expect(isStale(null, Date.now())).toBe(true)
  })

  it('says how old a number is in units a person reads', () => {
    const now = Date.now()
    expect(howLongAgo(now - 10_000, now)).toBe('just now')
    expect(howLongAgo(now - 4 * 60_000, now)).toBe('4m ago')
    expect(howLongAgo(now - 3 * 3600_000, now)).toBe('3h ago')
  })
})

describe('reset countdown', () => {
  const now = 1_800_000_000_000

  it('counts down to when the window comes back', () => {
    expect(untilReset(1_800_000_000 + 9600, now)).toBe('2h 40m')
    expect(untilReset(1_800_000_000 + 300, now)).toBe('5m')
  })

  it('says now once the window has come round', () => {
    expect(untilReset(1_800_000_000 - 10, now)).toBe('now')
  })

  it('has nothing to say when Claude Code did not', () => {
    expect(untilReset(undefined, now)).toBeNull()
  })
})

describe('presentation', () => {
  it('keeps unknown distinct from zero', () => {
    // Zero would read as "you have used none of it", which is the opposite of
    // what an absent number means.
    expect(percent(undefined)).toBeNull()
    expect(percent(0)).toBe(0)
  })

  it('never shows a gauge past its ends', () => {
    expect(percent(140)).toBe(100)
    expect(percent(-5)).toBe(0)
  })

  it('only raises the alarm once it means something', () => {
    expect(levelOf(40)).toBe('fine')
    expect(levelOf(74)).toBe('fine')
    expect(levelOf(80)).toBe('warn')
    expect(levelOf(95)).toBe('low')
  })
})
