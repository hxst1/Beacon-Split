import { describe, expect, it } from 'vitest'

import { panelAfter } from './panelFocus'

const ORDER = ['files', 'claude', 'terminal'] as const

describe('panelAfter', () => {
  it('walks the panels in the order they are laid out', () => {
    expect(panelAfter(ORDER, 'files', 1)).toBe('claude')
    expect(panelAfter(ORDER, 'claude', 1)).toBe('terminal')
  })

  it('wraps at both ends rather than stopping', () => {
    expect(panelAfter(ORDER, 'terminal', 1)).toBe('files')
    expect(panelAfter(ORDER, 'files', -1)).toBe('terminal')
  })

  it('goes backwards', () => {
    expect(panelAfter(ORDER, 'terminal', -1)).toBe('claude')
  })

  /** The first press has to do something, and the obvious something is to enter. */
  it('enters from the near end when nothing is focused', () => {
    expect(panelAfter(ORDER, null, 1)).toBe('files')
    expect(panelAfter(ORDER, null, -1)).toBe('terminal')
  })

  /** A panel can be hidden while the keyboard is in it. */
  it('enters from the near end when the focused panel is no longer laid out', () => {
    expect(panelAfter(ORDER, 'git', 1)).toBe('files')
    expect(panelAfter(ORDER, 'git', -1)).toBe('terminal')
  })

  it('has nowhere to go when nothing is visible', () => {
    expect(panelAfter([], 'files', 1)).toBeNull()
    expect(panelAfter([], null, 1)).toBeNull()
  })

  it('stays put when only one panel is visible', () => {
    expect(panelAfter(['claude'], 'claude', 1)).toBe('claude')
    expect(panelAfter(['claude'], 'claude', -1)).toBe('claude')
  })
})
