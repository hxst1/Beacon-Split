import { describe, expect, it } from 'vitest'

import { describePermission } from './copy'

describe('describePermission', () => {
  it('offers the system prompt exactly while it is still available', () => {
    expect(describePermission('notDetermined').action).toBe('ask')

    // macOS gives that prompt once per application. Offering the button again
    // afterwards would be a button that does nothing.
    for (const settled of ['authorized', 'denied', 'provisional', 'unavailable'] as const) {
      expect(describePermission(settled).action).not.toBe('ask')
    }
  })

  it('routes a refusal to the only place that can undo it', () => {
    expect(describePermission('denied').action).toBe('openSettings')
  })

  it('treats quiet delivery as something to fix, not as success', () => {
    // Provisional means it lands in Notification Centre with no banner, which
    // is indistinguishable from not being notified at all.
    const quiet = describePermission('provisional')
    expect(quiet.action).toBe('openSettings')
    expect(quiet.hint).not.toBeNull()
  })

  it('says nothing needs doing when it is allowed', () => {
    expect(describePermission('authorized')).toEqual({
      label: 'Allowed',
      hint: null,
      action: null,
    })
  })

  it('does not send an unbundled build to System Settings', () => {
    // There is no row there to find: macOS never saw an application.
    const dev = describePermission('unavailable')
    expect(dev.action).toBeNull()
    expect(dev.hint).toContain('unbundled')
  })

  it('says nothing at all before the first answer arrives', () => {
    expect(describePermission(null).action).toBeNull()
  })
})
