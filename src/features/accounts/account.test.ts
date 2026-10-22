import { describe, expect, it } from 'vitest'

import { accountLabel } from './account'

describe('accountLabel', () => {
  it('names the account and the plan when Claude Code says both', () => {
    const label = accountLabel('claude', {
      signedIn: true,
      email: 'someone@example.com',
      method: 'claude.ai',
      plan: 'max',
    })

    expect(label).toEqual({
      name: 'someone@example.com',
      detail: 'Max',
      title: 'Claude Code is signed in as someone@example.com on the Max plan, through claude.ai',
    })
  })

  it('falls back to the method for an agent that will not name the account', () => {
    // Codex, every time: `codex login status` prints how it signed in and
    // nothing about who.
    const label = accountLabel('codex', { method: 'ChatGPT' })

    expect(label?.name).toBe('ChatGPT')
    expect(label?.detail).toBeNull()
    // And the hover says why there is no name, so the gap reads as Codex's
    // rather than as Beacon failing to look.
    expect(label?.title).toContain('does not say which account')
  })

  it('says so when nobody is signed in', () => {
    expect(accountLabel('claude', { signedIn: false })).toEqual({
      name: 'not signed in',
      detail: null,
      title: 'Nobody is signed in to Claude Code',
    })
  })

  it('shows nothing at all when nothing was said', () => {
    // The case to protect: an agent too old to answer, or an answer that
    // named nothing. A chip reading "unknown" is worse than no chip.
    expect(accountLabel('claude', undefined)).toBeNull()
    expect(accountLabel('claude', {})).toBeNull()
    expect(accountLabel('codex', { signedIn: true })).toBeNull()
  })

  it('leaves out a plan nobody mentioned', () => {
    const label = accountLabel('claude', { signedIn: true, email: 'someone@example.com' })

    expect(label?.detail).toBeNull()
    expect(label?.title).toBe('Claude Code is signed in as someone@example.com')
  })
})
