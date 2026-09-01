import { describe, expect, it } from 'vitest'

import { workstreamLabel, type Workstream } from '@/types/beacon'
import { lastWorkedIn } from './workstreams'

const NOW = 1_800_000_000_000

function workstream(over: Partial<Workstream> = {}): Workstream {
  return {
    id: 'b57bf9d0-8020-4275-a060-a521d289beae',
    project: 'pj_x',
    createdAt: NOW / 1000,
    lastActiveAt: NOW / 1000,
    resumable: true,
    ...over,
  }
}

describe('workstreamLabel', () => {
  it('shows what you called it', () => {
    expect(workstreamLabel(workstream({ name: 'auth-refactor' }))).toBe('auth-refactor')
  })

  it('falls back to the front of the id rather than inventing a name', () => {
    // "untitled" on three rows tells you nothing about which is which; the
    // first block of the UUID at least tells them apart, and it is visibly not
    // a name so it invites one.
    expect(workstreamLabel(workstream())).toBe('b57bf9d0')
  })
})

describe('lastWorkedIn', () => {
  it('reads as a duration, not a timestamp', () => {
    const secondsAgo = (n: number): number => NOW / 1000 - n
    expect(lastWorkedIn(secondsAgo(20), NOW)).toBe('just now')
    expect(lastWorkedIn(secondsAgo(60 * 12), NOW)).toBe('12m')
    expect(lastWorkedIn(secondsAgo(3600 * 3), NOW)).toBe('3h')
    expect(lastWorkedIn(secondsAgo(3600 * 25), NOW)).toBe('1d')
    expect(lastWorkedIn(secondsAgo(86_400 * 9), NOW)).toBe('9d')
  })

  it('does not go backwards on a clock that has drifted', () => {
    expect(lastWorkedIn(NOW / 1000 + 30, NOW)).toBe('just now')
  })
})
