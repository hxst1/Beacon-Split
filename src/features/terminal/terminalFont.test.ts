import { describe, expect, it } from 'vitest'

import { DEFAULT_FONT_STACK, fontStack } from './terminalFont'

describe('fontStack', () => {
  it('is the default list when nothing is chosen', () => {
    expect(fontStack({ size: 12 })).toBe(DEFAULT_FONT_STACK)
    expect(fontStack({ family: '  ', size: 12 })).toBe(DEFAULT_FONT_STACK)
  })

  it('puts a chosen family in front of the defaults, quoted', () => {
    expect(fontStack({ family: 'Iosevka Term', size: 13 })).toBe(
      `"Iosevka Term", ${DEFAULT_FONT_STACK}`,
    )
  })

  // A Nerd Font installs under its own family name. Listed behind the plain
  // names, the plain name's fallback wins and the icons come from elsewhere.
  it('tries a Nerd Font before any font without the icons', () => {
    const families = DEFAULT_FONT_STACK.split(', ')
    expect(families[0]).toBe("'JetBrainsMono Nerd Font Mono'")
    expect(families.indexOf("'Symbols Nerd Font Mono'")).toBe(families.length - 2)
    expect(families.at(-1)).toBe('monospace')
  })
})
