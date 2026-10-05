import { beforeEach, describe, expect, it } from 'vitest'

import { panelAfter, usePanelFocus } from './panelFocus'

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

describe('the agent panel the keyboard was in last', () => {
  beforeEach(() => {
    usePanelFocus.setState({ focused: null, lastAgent: null })
  })

  it('is nothing until the keyboard has been in one', () => {
    expect(usePanelFocus.getState().lastAgent).toBeNull()

    usePanelFocus.getState().set('files')
    expect(usePanelFocus.getState().lastAgent).toBeNull()
  })

  it('is remembered after focus moves away', () => {
    // The point of keeping it: clicking into the git panel to read a diff must
    // not stop that diff being the agent's.
    usePanelFocus.getState().set('codex')
    usePanelFocus.getState().set('git')

    expect(usePanelFocus.getState().focused).toBe('git')
    expect(usePanelFocus.getState().lastAgent).toBe('codex')
  })

  it('moves with the agent, and only with an agent', () => {
    usePanelFocus.getState().set('claude')
    expect(usePanelFocus.getState().lastAgent).toBe('claude')

    usePanelFocus.getState().set('codex')
    expect(usePanelFocus.getState().lastAgent).toBe('codex')

    usePanelFocus.getState().set('terminal')
    usePanelFocus.getState().set(null)
    expect(usePanelFocus.getState().lastAgent).toBe('codex')
  })
})
