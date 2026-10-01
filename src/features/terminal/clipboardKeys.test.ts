import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { setPlatform } from '@/lib/platform'
import { clipboardKey } from './clipboardKeys'

function press(key: string, modifiers: { shift?: boolean; meta?: boolean; alt?: boolean } = {}) {
  return {
    type: 'keydown',
    key,
    ctrlKey: !modifiers.meta,
    metaKey: modifiers.meta ?? false,
    shiftKey: modifiers.shift ?? false,
    altKey: modifiers.alt ?? false,
  }
}

beforeEach(() => {
  vi.stubGlobal('document', { documentElement: { dataset: {} } })
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('clipboard keys on Windows', () => {
  beforeEach(() => setPlatform('windows'))

  it('copies with Ctrl+C when something is selected', () => {
    expect(clipboardKey(press('c'), true)).toBe('copy')
  })

  it('leaves Ctrl+C to interrupt when nothing is selected', () => {
    expect(clipboardKey(press('c'), false)).toBe('terminal')
  })

  it('pastes with Ctrl+V and with Ctrl+Shift+V', () => {
    expect(clipboardKey(press('v'), false)).toBe('paste')
    expect(clipboardKey(press('V', { shift: true }), false)).toBe('paste')
  })

  it('always copies with Ctrl+Shift+C', () => {
    expect(clipboardKey(press('C', { shift: true }), false)).toBe('copy')
  })

  it('leaves every other control key to the terminal', () => {
    for (const key of ['a', 'd', 'l', 'r', 'z']) {
      expect(clipboardKey(press(key), true)).toBe('terminal')
    }
    expect(clipboardKey(press('c', { alt: true }), true)).toBe('terminal')
  })

  it('only acts on the key going down', () => {
    expect(clipboardKey({ ...press('c'), type: 'keyup' }, true)).toBe('terminal')
  })
})

describe('clipboard keys on Linux', () => {
  beforeEach(() => setPlatform('linux'))

  it('keeps Ctrl+C and Ctrl+V for the terminal, as Linux terminals do', () => {
    expect(clipboardKey(press('c'), true)).toBe('terminal')
    expect(clipboardKey(press('v'), false)).toBe('terminal')
  })

  it('copies and pastes with Ctrl+Shift', () => {
    expect(clipboardKey(press('C', { shift: true }), true)).toBe('copy')
    expect(clipboardKey(press('V', { shift: true }), false)).toBe('paste')
  })
})

describe('clipboard keys on macOS', () => {
  beforeEach(() => setPlatform('macos'))

  it('never interferes: ⌘C and ⌘V are the webview’s, Ctrl is the terminal’s', () => {
    expect(clipboardKey(press('c', { meta: true }), true)).toBe('terminal')
    expect(clipboardKey(press('c'), true)).toBe('terminal')
    expect(clipboardKey(press('V', { shift: true }), false)).toBe('terminal')
  })
})
