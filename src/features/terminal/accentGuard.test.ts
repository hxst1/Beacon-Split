import { describe, expect, it } from 'vitest'

import { accentGuard, type AccentGuard } from './accentGuard'

type Step =
  | { keydown: number }
  | { compositionstart: true }
  | { compositionend: string }
  | { input: string | null; type: string }

/** Plays events into a guard; returns what reached the session and what reached xterm. */
function play(steps: Step[]): { sent: string; xterm: string[]; cleared: number } {
  let sent = ''
  let cleared = 0
  const xterm: string[] = []
  const guard: AccentGuard = accentGuard(
    (data) => {
      sent += data
    },
    () => {
      cleared += 1
    },
  )
  for (const step of steps) {
    if ('keydown' in step) {
      if (guard.keydown({ keyCode: step.keydown })) xterm.push(`keydown ${step.keydown}`)
    } else if ('compositionstart' in step) {
      guard.compositionstart()
      xterm.push('compositionstart')
    } else if ('compositionend' in step) {
      if (guard.compositionend({ data: step.compositionend })) xterm.push('compositionend')
    } else if (!guard.input({ inputType: step.type, data: step.input })) {
      xterm.push(`input ${step.type}`)
    }
  }
  return { sent, xterm, cleared }
}

/**
 * `´` then a letter, as WebKitGTK 2.52 delivers it on Wayland with a Spanish
 * layout and no input-method framework — recorded, not imagined.
 */
function deadKey(letter: string): Step[] {
  return [
    { keydown: 229 },
    { compositionstart: true },
    { input: '´', type: 'insertCompositionText' },
    { keydown: 229 },
    { input: null, type: 'deleteCompositionText' },
    { compositionend: '' },
    { keydown: 229 },
    { input: letter, type: 'insertText' },
    { compositionend: letter },
  ]
}

describe('accentGuard', () => {
  it('delivers a dead-key accent once', () => {
    expect(play(deadKey('á')).sent).toBe('á')
  })

  it('keeps the orphaned compositionend from xterm', () => {
    const { xterm } = play(deadKey('á'))
    expect(xterm.filter((event) => event === 'compositionend')).toHaveLength(1)
  })

  it('empties the textarea so nothing is read back later', () => {
    expect(play(deadKey('é')).cleared).toBeGreaterThan(0)
  })

  it('delivers a sentence of them, each once', () => {
    const steps = [...deadKey('ó'), { keydown: 78 }, ...deadKey('ü'), ...deadKey('é')]
    expect(play(steps).sent).toBe('óüé')
  })

  it('still delivers an orphaned commit whose input never came', () => {
    expect(play([{ keydown: 229 }, { compositionend: 'ñ' }]).sent).toBe('ñ')
  })

  // A composition that announces itself is how Japanese, Chinese and Korean
  // arrive, and xterm handles those itself.
  it('leaves a composition that started properly to xterm', () => {
    const { sent, xterm } = play([
      { compositionstart: true },
      { keydown: 229 },
      { input: 'に', type: 'insertCompositionText' },
      { keydown: 229 },
      { input: '日本', type: 'insertCompositionText' },
      { compositionend: '日本' },
    ])
    expect(sent).toBe('')
    expect(xterm).toEqual([
      'compositionstart',
      'keydown 229',
      'input insertCompositionText',
      'keydown 229',
      'input insertCompositionText',
      'compositionend',
    ])
  })

  it('leaves an ordinary key to xterm', () => {
    const { sent, xterm } = play([{ keydown: 65 }, { input: 'a', type: 'insertText' }])
    expect(sent).toBe('')
    expect(xterm).toEqual(['keydown 65', 'input insertText'])
  })

  // With an input method running, punctuation and Backspace also come as 229
  // with no composition; xterm sent those by diffing its textarea.
  it('sends what an input method types outside a composition', () => {
    const { sent } = play([
      { keydown: 229 },
      { input: '，', type: 'insertText' },
      { keydown: 229 },
      { input: null, type: 'deleteContentBackward' },
    ])
    expect(sent).toBe('，\x7f')
  })
})
