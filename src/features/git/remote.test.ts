import { describe, expect, it } from 'vitest'

import type { GitStatus } from '@/types/beacon'
import { remoteActions } from './remote'

const status = (over: Partial<GitStatus> = {}): GitStatus => ({
  branch: 'main',
  ahead: 0,
  behind: 0,
  unborn: false,
  entries: [],
  ...over,
})

describe('remoteActions', () => {
  it('offers push and pull on a branch that tracks one', () => {
    const actions = remoteActions(status({ upstream: 'origin/main' }))

    expect(actions.canPush).toBe(true)
    expect(actions.canPull).toBe(true)
    expect(actions.reason).toBeNull()
  })

  it('withholds them on a branch with no upstream, and says how to get one', () => {
    const actions = remoteActions(status({ branch: 'feature/thing' }))

    expect(actions.canPush).toBe(false)
    expect(actions.canPull).toBe(false)
    expect(actions.reason).toContain('git push -u origin feature/thing')
  })

  it('withholds them on a detached head', () => {
    const actions = remoteActions(status({ branch: null }))

    expect(actions.canPush).toBe(false)
    expect(actions.canPull).toBe(false)
    expect(actions.reason).toContain('detached')
  })

  it('still offers a push from a branch that is already up to date', () => {
    // Being level with the upstream is not a reason to take the button away;
    // whether there is anything to send is git's answer to give.
    expect(remoteActions(status({ upstream: 'origin/main', ahead: 0 })).canPush).toBe(true)
  })
})
