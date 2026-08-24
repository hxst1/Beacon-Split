import { describe, expect, it } from 'vitest'

import { selectBindings, selectHidden, selectWorkspaces } from './store'

/**
 * Selectors must return the same reference when nothing changed.
 *
 * Zustand compares with `Object.is`. A selector that builds a value — `?? []`,
 * `.filter(...)`, `.map(...)` — hands it a new one every call, so the store
 * concludes the state changed and re-renders, forever. That is not theoretical:
 * it took the whole application down, and the symptom was a blank window with
 * nothing pointing anywhere near the cause.
 */
const emptyState = { snapshot: null, missing: [] } as never

describe('selector stability', () => {
  it('returns the same empty workspaces every time', () => {
    expect(selectWorkspaces(emptyState)).toBe(selectWorkspaces(emptyState))
  })

  it('returns the same empty bindings every time', () => {
    expect(selectBindings(emptyState)).toBe(selectBindings(emptyState))
  })

  it('returns the same empty hidden panels every time', () => {
    expect(selectHidden(emptyState)).toBe(selectHidden(emptyState))
  })

  it('passes through what the snapshot holds, unchanged', () => {
    const workspaces = [{ id: 'ws_1' }]
    const bindings = [{ action: 'palette.open' }]
    const state = { snapshot: { workspaces, bindings } } as never

    // The same array the snapshot holds, not a copy of it.
    expect(selectWorkspaces(state)).toBe(workspaces)
    expect(selectBindings(state)).toBe(bindings)
  })
})
