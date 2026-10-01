import { describe, expect, it } from 'vitest'

import { nextCheckout } from './GitPane'
import type { AgentKind } from '@/types/beacon'

describe('nextCheckout', () => {
  it('starts and comes back to the user own copy', () => {
    // Yours first, because that is where everything starts and what a panel
    // showing something else has to be an explicit choice away from.
    const seen: Array<AgentKind | undefined> = []
    let at: AgentKind | undefined = undefined
    for (let i = 0; i < 4; i += 1) {
      seen.push(at)
      at = nextCheckout(at)
    }
    expect(seen).toEqual([undefined, 'claude', 'codex', undefined])
  })
})
