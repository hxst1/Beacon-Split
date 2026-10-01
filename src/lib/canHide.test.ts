import { describe, expect, it } from 'vitest'

import { canHide } from './layout'

describe('canHide', () => {
  it('lets any ordinary panel be put away', () => {
    expect(canHide('files', [])).toBe(true)
    expect(canHide('terminal', ['editor'])).toBe(true)
  })

  it('keeps the last agent on screen', () => {
    // Codex away, as it starts: closing Claude would leave nothing to work in,
    // so the button is not offered rather than offered and refused.
    expect(canHide('claude', ['codex'])).toBe(false)
    expect(canHide('codex', ['claude'])).toBe(false)
  })

  it('lets either go while the other is there', () => {
    expect(canHide('claude', [])).toBe(true)
    expect(canHide('codex', [])).toBe(true)
  })
})
