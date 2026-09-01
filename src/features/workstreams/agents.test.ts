import { describe, expect, it } from 'vitest'

import type { AgentActivity } from '@/types/beacon'
import {
  LINGER_MS,
  agentLabel,
  elapsed,
  forgetFinished,
  reduceAgents,
  type RunningAgent,
} from './agents'

const NOW = 1_800_000_000_000

function report(over: Partial<AgentActivity> = {}): AgentActivity {
  return { project: 'pj_x', agent: 'a071', running: true, ...over }
}

describe('reduceAgents', () => {
  it('shows an agent from the moment it starts', () => {
    const held = reduceAgents([], report({ agentType: 'beacon-explorer' }), NOW)
    expect(held).toEqual([{ agent: 'a071', agentType: 'beacon-explorer', startedAt: NOW }])
  })

  it('pairs a stop with its start and keeps how long it took', () => {
    const started = reduceAgents([], report({ agentType: 'beacon-explorer' }), NOW)
    const done = reduceAgents(
      started,
      report({ running: false, summary: 'Found 4 relevant files' }),
      NOW + 12_000,
    )

    expect(done).toHaveLength(1)
    expect(done[0]).toMatchObject({
      agent: 'a071',
      agentType: 'beacon-explorer',
      startedAt: NOW,
      finishedAt: NOW + 12_000,
      summary: 'Found 4 relevant files',
    })
    expect(elapsed(done[0]!, NOW + 99_999)).toBe('12s')
  })

  it('keeps the type from the start when the stop does not repeat it', () => {
    // Claude Code reports `agent_type` empty on a real SubagentStop, and the
    // hook turns that into absent rather than an empty name.
    const started = reduceAgents([], report({ agentType: 'beacon-tester' }), NOW)
    const done = reduceAgents(started, report({ running: false }), NOW + 2_000)
    expect(done[0]?.agentType).toBe('beacon-tester')
  })

  it('still shows a stop for an agent it never saw start', () => {
    // The window was opened mid-run. Dated from now, so the elapsed time is
    // honest about being unknown rather than wrong.
    const done = reduceAgents([], report({ running: false }), NOW)
    expect(done).toHaveLength(1)
    expect(elapsed(done[0]!, NOW)).toBe('0s')
  })

  it('holds several at once and never doubles one up', () => {
    let held: RunningAgent[] = []
    held = reduceAgents(held, report({ agent: 'a1', agentType: 'beacon-explorer' }), NOW)
    held = reduceAgents(held, report({ agent: 'a2', agentType: 'beacon-tester' }), NOW)
    held = reduceAgents(held, report({ agent: 'a1', agentType: 'beacon-explorer' }), NOW)

    expect(held.map((one) => one.agent)).toEqual(['a2', 'a1'])
  })
})

describe('forgetFinished', () => {
  it('keeps what is still running, however long it runs', () => {
    const running: RunningAgent = { agent: 'a1', startedAt: NOW }
    expect(forgetFinished([running], NOW + 60 * 60_000)).toEqual([running])
  })

  it('drops what finished long enough ago to have been read', () => {
    const done: RunningAgent = { agent: 'a1', startedAt: NOW, finishedAt: NOW }
    expect(forgetFinished([done], NOW + LINGER_MS - 1)).toHaveLength(1)
    expect(forgetFinished([done], NOW + LINGER_MS)).toHaveLength(0)
  })
})

describe('agentLabel', () => {
  it('drops the prefix, because every one of ours has it', () => {
    expect(agentLabel({ agent: 'a1', startedAt: NOW, agentType: 'beacon-explorer' })).toBe(
      'explorer',
    )
  })

  it('leaves a name that is not ours alone', () => {
    expect(agentLabel({ agent: 'a1', startedAt: NOW, agentType: 'Explore' })).toBe('Explore')
  })

  it('says something rather than nothing when Claude Code did not name it', () => {
    expect(agentLabel({ agent: 'a1', startedAt: NOW })).toBe('agent')
  })
})

describe('elapsed', () => {
  it('counts up while it runs', () => {
    const running: RunningAgent = { agent: 'a1', startedAt: NOW }
    expect(elapsed(running, NOW + 8_400)).toBe('8s')
    expect(elapsed(running, NOW + 80_000)).toBe('1m 20s')
  })

  it('does not go backwards on a clock that has drifted', () => {
    expect(elapsed({ agent: 'a1', startedAt: NOW }, NOW - 5_000)).toBe('0s')
  })
})
