import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

import { describe, expect, it } from 'vitest'

import { LIGHT_ANSI, MINIMUM_CONTRAST, terminalTheme } from './terminalTheme'

const TOKENS = readFileSync(fileURLToPath(new URL('../../styles/tokens.css', import.meta.url)), 'utf8')

/** `--window` as each palette declares it, read off the stylesheet. */
function windowOf(theme: 'light' | 'dark'): string {
  const selector = theme === 'light' ? String.raw`\[data-theme='light'\]` : String.raw`\[data-theme='dark'\]`
  const match = TOKENS.match(new RegExp(`${selector}\\s*\\{[^}]*?--window:\\s*([\\d ]+);`))
  if (!match?.[1]) throw new Error(`no --window for ${theme}`)
  return match[1]
}

type Rgb = [number, number, number]

/** The way xterm reads an `rgba()` (common/Color.ts): channels, then alpha. */
function xtermRgba(css: string): { rgb: Rgb; alpha: number } {
  const match = css.match(/rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(,\s*(0|1|\d?\.(\d+))\s*)?\)/)
  if (!match) throw new Error(`xterm would not parse ${css}`)
  return {
    rgb: [Number(match[1]), Number(match[2]), Number(match[3])],
    alpha: match[5] === undefined ? 1 : Number(match[5]),
  }
}

function hex(css: string): Rgb {
  const n = parseInt(css.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

function luminance([r, g, b]: Rgb): number {
  const lin = (c: number) => {
    const s = c / 255
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

function contrast(a: Rgb, b: Rgb): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number]
  return (hi + 0.05) / (lo + 0.05)
}

const light = terminalTheme({ light: true, window: windowOf('light'), accent: '#6b7cff' })
const dark = terminalTheme({ light: false, window: windowOf('dark'), accent: '#6b7cff' })

describe('the terminal theme', () => {
  it('stays transparent, but says which colour is behind it', () => {
    // xterm measures contrast against these channels and reports them to a
    // program that asks (OSC 11); the alpha it ignores for both.
    const back = xtermRgba(light.background!)
    expect(back.alpha).toBe(0)
    expect(back.rgb.join(' ')).toBe(windowOf('light'))
    expect(luminance(back.rgb)).toBeGreaterThan(0.5)

    const night = xtermRgba(dark.background!)
    expect(night.alpha).toBe(0)
    expect(night.rgb.join(' ')).toBe(windowOf('dark'))
    expect(luminance(night.rgb)).toBeLessThan(0.05)
  })

  it('falls back to the shipped window when the variable cannot be read', () => {
    expect(xtermRgba(terminalTheme({ light: true, window: '', accent: '#fff' }).background!).rgb).toEqual([
      246, 246, 248,
    ])
    expect(xtermRgba(terminalTheme({ light: false, window: 'nonsense', accent: '#fff' }).background!).rgb).toEqual([
      8, 8, 11,
    ])
  })

  it('gives the light ground all sixteen colours, each legible on it', () => {
    const ground = xtermRgba(light.background!).rgb
    const names = Object.keys(LIGHT_ANSI) as Array<keyof typeof LIGHT_ANSI>
    expect(names).toHaveLength(16)
    for (const name of names) {
      expect(light[name], name).toBe(LIGHT_ANSI[name])
      expect(contrast(hex(LIGHT_ANSI[name]), ground), name).toBeGreaterThanOrEqual(MINIMUM_CONTRAST)
    }
  })

  it('leaves the dark ground on the palette xterm made for it', () => {
    for (const name of Object.keys(LIGHT_ANSI)) {
      expect(dark).not.toHaveProperty(name)
    }
  })

  it('holds every glyph to AA, whatever colour a program picks', () => {
    expect(MINIMUM_CONTRAST).toBeGreaterThanOrEqual(4.5)
  })
})
