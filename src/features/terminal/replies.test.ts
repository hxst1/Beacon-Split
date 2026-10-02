import { describe, expect, it } from 'vitest'

import { nextReplies } from './replies'
import type { ClaudeActivity, SessionActivity } from '@/types/beacon'

function report(activity: ClaudeActivity, reply?: string, project = 'a'): SessionActivity {
  return { project, activity, detail: null, reply: reply ?? null }
}

describe('nextReplies', () => {
  it('keeps the reply a turn ended with', () => {
    expect(nextReplies({}, report('done', 'All four pass.'))).toEqual({ a: 'All four pass.' })
  })

  it('replaces the last reply with the newest', () => {
    const replies = nextReplies({ a: 'First.' }, report('done', 'Second.'))
    expect(replies).toEqual({ a: 'Second.' })
  })

  it('drops the reply once the next turn starts working', () => {
    expect(nextReplies({ a: 'Done.' }, report('working'))).toEqual({})
  })

  it('drops it when the session is cleared or ends', () => {
    expect(nextReplies({ a: 'Done.' }, report('idle'))).toEqual({})
    expect(nextReplies({ a: 'Done.' }, report('ended'))).toEqual({})
  })

  it('leaves it through a permission prompt', () => {
    const replies = { a: 'Done.' }
    expect(nextReplies(replies, report('waiting'))).toBe(replies)
  })

  it('drops a stale reply when a turn ends without one', () => {
    expect(nextReplies({ a: 'Old.' }, report('done'))).toEqual({})
  })

  it('touches only the project that reported', () => {
    expect(nextReplies({ a: 'A.', b: 'B.' }, report('working', undefined, 'b'))).toEqual({
      a: 'A.',
    })
  })

  it('returns the same object when nothing changed, so nothing re-renders', () => {
    const replies = { a: 'Same.' }
    expect(nextReplies(replies, report('done', 'Same.'))).toBe(replies)
    expect(nextReplies(replies, report('working', undefined, 'b'))).toBe(replies)
  })
})
