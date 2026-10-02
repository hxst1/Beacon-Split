import { describe, expect, it } from 'vitest'

import { newlineKey } from './newlineKeys'

/** Shift+Enter as the browser reports it; `over` replaces what a case is about. */
function press(over: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return {
    type: 'keydown',
    key: 'Enter',
    ctrlKey: false,
    shiftKey: true,
    altKey: false,
    metaKey: false,
    ...over,
  } as KeyboardEvent
}

describe('Shift+Enter', () => {
  it('breaks the line for Claude with the sequence its own setup binds', () => {
    expect(newlineKey(press(), 'claude')).toBe('\x1b\r')
  })

  it('breaks the line for Codex with the line feed crossterm reads as Ctrl+J', () => {
    expect(newlineKey(press(), 'codex')).toBe('\n')
  })

  it('is left to the shell, where Enter is the whole point', () => {
    expect(newlineKey(press(), 'shell')).toBeNull()
  })

  it('leaves a plain Enter alone, so a finished prompt still sends', () => {
    expect(newlineKey(press({ shiftKey: false }), 'claude')).toBeNull()
  })

  it('does not claim Enter with another modifier on top', () => {
    expect(newlineKey(press({ ctrlKey: true }), 'claude')).toBeNull()
    expect(newlineKey(press({ altKey: true }), 'claude')).toBeNull()
    expect(newlineKey(press({ metaKey: true }), 'claude')).toBeNull()
  })

  it('acts once, on the way down', () => {
    expect(newlineKey(press({ type: 'keyup' }), 'claude')).toBeNull()
    expect(newlineKey(press({ type: 'keypress' }), 'claude')).toBeNull()
  })

  it('is only Enter', () => {
    expect(newlineKey(press({ key: 'Tab' }), 'claude')).toBeNull()
    expect(newlineKey(press({ key: 'j' }), 'claude')).toBeNull()
  })
})
