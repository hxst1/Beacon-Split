import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  drawsOwnWindowControls,
  hasPrimaryModifier,
  isMac,
  isWindows,
  modifierLabel,
  setPlatform,
  setWindowsBuild,
  shortcutLabel,
  windowsPty,
} from './platform'

const dataset: Record<string, string> = {}

beforeEach(() => {
  vi.stubGlobal('document', { documentElement: { dataset } })
  setWindowsBuild(null)
})

afterEach(() => {
  vi.unstubAllGlobals()
})

function key(modifiers: { ctrl?: boolean; meta?: boolean }) {
  return { ctrlKey: modifiers.ctrl ?? false, metaKey: modifiers.meta ?? false } as KeyboardEvent
}

describe('the platform', () => {
  it('is marked on the document, where the stylesheet reads it', () => {
    setPlatform('windows')
    expect(dataset['platform']).toBe('windows')
  })

  it('makes ⌘ the primary modifier on macOS', () => {
    setPlatform('macos')
    expect(isMac()).toBe(true)
    expect(hasPrimaryModifier(key({ meta: true }))).toBe(true)
    expect(hasPrimaryModifier(key({ ctrl: true }))).toBe(false)
    expect(shortcutLabel('K')).toBe('⌘K')
  })

  it('makes Ctrl the primary modifier on Windows, as on Linux', () => {
    for (const platform of ['windows', 'linux']) {
      setPlatform(platform)
      expect(isMac()).toBe(false)
      expect(hasPrimaryModifier(key({ ctrl: true }))).toBe(true)
      expect(hasPrimaryModifier(key({ meta: true }))).toBe(false)
      expect(modifierLabel()).toBe('Ctrl')
      expect(shortcutLabel('K')).toBe('Ctrl+K')
    }
    expect(isWindows()).toBe(false)
  })
})

describe('who draws the window controls', () => {
  it('is the system on macOS, and Beacon everywhere else', () => {
    // The capability that allows minimise, maximise and close names the same
    // two platforms; a window that cannot be closed is not a detail.
    setPlatform('macos')
    expect(drawsOwnWindowControls()).toBe(false)

    for (const platform of ['windows', 'linux']) {
      setPlatform(platform)
      expect(drawsOwnWindowControls()).toBe(true)
    }
  })
})

describe('what xterm is told about the pty', () => {
  it('is nothing outside Windows', () => {
    setPlatform('linux')
    setWindowsBuild(26200)
    expect(windowsPty()).toBeUndefined()
  })

  it('names the pseudo-console and the build on Windows', () => {
    setPlatform('windows')
    setWindowsBuild(26200)
    expect(windowsPty()).toEqual({ backend: 'conpty', buildNumber: 26200 })
  })

  it('still names the pseudo-console when the build is unknown', () => {
    setPlatform('windows')
    expect(windowsPty()).toEqual({ backend: 'conpty' })
  })
})
