import { beforeEach, describe, expect, it } from 'vitest'

import { PREVIEW_LINES, accept, age, isLiteral, labelOf, preview, replace, useClips } from './clips'
import type { Clip } from '@/types/beacon'

const clip = (over: Partial<Clip> = {}): Clip => ({
  id: 'cl_1',
  project: 'pj_x',
  title: 'Staging keys',
  body: 'API_KEY=abc',
  kind: 'variable',
  createdAt: 1_800_000_000,
  ...over,
})

beforeEach(() => {
  useClips.setState({ clips: [], open: false, unseen: 0, copied: null, failed: null })
})

describe('filing what arrives', () => {
  it('puts the newest clip first', () => {
    accept(clip({ id: 'cl_1' }))
    accept(clip({ id: 'cl_2' }))
    expect(useClips.getState().clips.map((entry) => entry.id)).toEqual(['cl_2', 'cl_1'])
  })

  it('does not file the same clip twice', () => {
    // The window asks for the whole drawer when it reattaches, so a clip that
    // arrived during the reconnect is in that answer *and* in the event that
    // announced it. Without this it would be listed twice.
    accept(clip())
    accept(clip())
    expect(useClips.getState().clips).toHaveLength(1)
  })

  it('counts what arrived while the drawer was shut', () => {
    accept(clip({ id: 'cl_1' }))
    accept(clip({ id: 'cl_2' }))
    expect(useClips.getState().unseen).toBe(2)
  })

  it('counts nothing while the drawer is open', () => {
    useClips.setState({ open: true })
    accept(clip())
    expect(useClips.getState().unseen).toBe(0)
  })

  it('never claims more is waiting than there is', () => {
    // Clips arrive, then the drawer is emptied without being opened. The tab
    // would otherwise keep advertising things that have been thrown away.
    accept(clip({ id: 'cl_1' }))
    accept(clip({ id: 'cl_2' }))
    replace([])
    expect(useClips.getState().unseen).toBe(0)
  })
})

describe('how a clip is shown', () => {
  it('shows commands and variables literally', () => {
    // A wrapped command pastes its line break into a shell as a return.
    expect(isLiteral('command')).toBe(true)
    expect(isLiteral('variable')).toBe(true)
    expect(isLiteral('email')).toBe(false)
    expect(isLiteral('text')).toBe(false)
  })

  it('names every kind', () => {
    expect(labelOf('command')).toBe('command')
    expect(labelOf('email')).toBe('email')
    expect(labelOf('text')).toBe('text')
  })

  it('leaves a short body whole', () => {
    expect(preview('one\ntwo')).toEqual({ text: 'one\ntwo', truncated: false })
  })

  it('cuts a long body on a line boundary', () => {
    // By lines, not characters: half of the last line of an .env block still
    // reads as a value, and a truncated value that looks whole is the thing
    // somebody pastes without checking.
    const body = Array.from({ length: PREVIEW_LINES + 4 }, (_, i) => `LINE_${i}=x`).join('\n')
    const shown = preview(body)

    expect(shown.truncated).toBe(true)
    expect(shown.text.split('\n')).toHaveLength(PREVIEW_LINES)
    expect(shown.text.endsWith('=x')).toBe(true)
  })
})

describe('how old a clip is', () => {
  const now = (seconds: number): number => (1_800_000_000 + seconds) * 1000

  it('says now for something that just arrived', () => {
    expect(age(1_800_000_000, now(5))).toBe('now')
  })

  it('counts minutes, then hours, then days', () => {
    expect(age(1_800_000_000, now(180))).toBe('3m')
    expect(age(1_800_000_000, now(7200))).toBe('2h')
    expect(age(1_800_000_000, now(3 * 86_400))).toBe('3d')
  })

  it('never reads as being from the future', () => {
    // Clocks disagree: the daemon stamps the clip, the window renders it.
    expect(age(1_800_000_000, now(-30))).toBe('now')
  })
})
