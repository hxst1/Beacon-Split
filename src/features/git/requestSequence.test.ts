import { describe, expect, it } from 'vitest'

import { createRequestSequence } from './requestSequence'

describe('request sequence', () => {
  it('only accepts the last-started request', () => {
    const sequence = createRequestSequence()
    const first = sequence.begin()
    const second = sequence.begin()

    expect(sequence.isCurrent(first)).toBe(false)
    expect(sequence.isCurrent(second)).toBe(true)
  })

  it('invalidates an in-flight request before a mutation', () => {
    const sequence = createRequestSequence()
    const request = sequence.begin()

    sequence.invalidate()

    expect(sequence.isCurrent(request)).toBe(false)
  })
})
