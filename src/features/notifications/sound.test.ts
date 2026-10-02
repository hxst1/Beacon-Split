import { describe, expect, it } from 'vitest'

import { CHIMES } from './sound'

describe('CHIMES', () => {
  it('rises when Claude is waiting and falls when it is done, so they are told apart by ear', () => {
    const [firstWait, lastWait] = [CHIMES.waiting[0]!, CHIMES.waiting.at(-1)!]
    const [firstDone, lastDone] = [CHIMES.done[0]!, CHIMES.done.at(-1)!]
    expect(lastWait.frequency).toBeGreaterThan(firstWait.frequency)
    expect(lastDone.frequency).toBeLessThan(firstDone.frequency)
  })

  it('stays short enough not to be a tune', () => {
    for (const notes of Object.values(CHIMES)) {
      const end = Math.max(...notes.map((note) => note.at + note.length))
      expect(end).toBeLessThan(0.6)
    }
  })

  it('plays its notes in order', () => {
    for (const notes of Object.values(CHIMES)) {
      const starts = notes.map((note) => note.at)
      expect(starts).toEqual([...starts].sort((a, b) => a - b))
    }
  })
})
