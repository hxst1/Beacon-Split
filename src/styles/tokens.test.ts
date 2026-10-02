import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'

import { describe, expect, it } from 'vitest'

/**
 * The dark palette's contrast, measured rather than eyeballed.
 *
 * Every text token is white at a low alpha over a near-black window that is
 * itself translucent, so what a label actually contrasts against depends on how
 * many surfaces it sits on and on what is behind the window. Reading the values
 * off the stylesheet and compositing them the way the browser does is the only
 * way to know whether a level is legible — and the reason this is a test and
 * not a comment is that the alphas look arbitrary, so nothing else would stop
 * one being nudged back down.
 *
 * The ground used here is the shipped default opacity. Taking the slider lower
 * trades legibility for the desktop on purpose, which the settings copy says.
 */
const TOKENS = readFileSync(fileURLToPath(new URL('./tokens.css', import.meta.url)), 'utf8')

/** WCAG 2.1 AA for body text. The levels below are all body text somewhere. */
const AA = 4.5

const dark = darkBlock(TOKENS)

const WINDOW = rgbTriplet(declaration(dark, '--window'))
const WINDOW_ALPHA = Number(declaration(dark, '--window-alpha'))
const SURFACES = (['--surface-1', '--surface-2', '--surface-3'] as const).map((token) =>
  whiteAlpha(declaration(dark, token)),
)
const FIELD = blackAlpha(declaration(dark, '--field'))

const TEXT = ['--fg-1', '--fg-2', '--fg-3', '--fg-4'] as const
const SEMANTIC = ['--ok', '--warn', '--danger', '--info', '--purple'] as const

/**
 * Every ground a label can land on: the window over a black and over a white
 * desktop, bare, inset as a field, and under each surface layer. A white
 * desktop behind the window is the worst of them, and the case that prompted
 * this — glare adds light to everything and flattens whatever starts close
 * together.
 */
function grounds(): Array<[string, Rgb]> {
  const all: Array<[string, Rgb]> = []
  for (const [desktop, label] of [
    [[0, 0, 0] as Rgb, 'black desktop'],
    [[255, 255, 255] as Rgb, 'white desktop'],
  ] as const) {
    const base = over(WINDOW, WINDOW_ALPHA, desktop)
    all.push([`${label}, bare`, base])
    all.push([`${label}, field`, over([0, 0, 0], FIELD, base)])
    SURFACES.forEach((alpha, i) => {
      all.push([`${label}, surface-${i + 1}`, over([255, 255, 255], alpha, base)])
    })
  }
  return all
}

describe('the dark palette', () => {
  it('carries every level of text at AA on every ground', () => {
    const failing: string[] = []
    for (const token of TEXT) {
      const alpha = whiteAlpha(declaration(dark, token))
      for (const [where, ground] of grounds()) {
        const ratio = contrast(over([255, 255, 255], alpha, ground), ground)
        if (ratio < AA) failing.push(`${token} on ${where}: ${ratio.toFixed(2)}`)
      }
    }
    expect(failing).toEqual([])
  })

  it('keeps the four levels in order and apart, so brighter is not flatter', () => {
    const alphas = TEXT.map((token) => whiteAlpha(declaration(dark, token)))
    for (let i = 1; i < alphas.length; i += 1) {
      // Each step down is a visible step, not a rounding difference.
      expect(alphas[i - 1]! - alphas[i]!).toBeGreaterThanOrEqual(0.08)
    }
  })

  it('carries the colours that mean something at AA on the worst ground', () => {
    const worst = over([255, 255, 255], SURFACES[2]!, over(WINDOW, WINDOW_ALPHA, [255, 255, 255]))
    const failing: string[] = []
    for (const token of SEMANTIC) {
      const ratio = contrast(hex(declaration(dark, token)), worst)
      if (ratio < AA) failing.push(`${token}: ${ratio.toFixed(2)}`)
    }
    expect(failing).toEqual([])
  })
})

/**
 * The light palette, on the same terms. Its text is dark ink at an alpha over a
 * light window, its surfaces darken rather than lighten, and its worst ground
 * is the opposite desktop: a black one behind the window dims the material
 * and pulls the ground toward the ink.
 */
const light = lightBlock(TOKENS)
const LIGHT_WINDOW = rgbTriplet(declaration(light, '--window'))
const INK: Rgb = [20, 20, 26]
const LIGHT_SURFACES = (['--surface-1', '--surface-2', '--surface-3'] as const).map((token) =>
  blackAlpha(declaration(light, token)),
)
const LIGHT_FIELD = blackAlpha(declaration(light, '--field'))

function lightGrounds(): Array<[string, Rgb]> {
  const all: Array<[string, Rgb]> = []
  for (const [desktop, label] of [
    [[0, 0, 0] as Rgb, 'black desktop'],
    [[255, 255, 255] as Rgb, 'white desktop'],
  ] as const) {
    const base = over(LIGHT_WINDOW, WINDOW_ALPHA, desktop)
    all.push([`${label}, bare`, base])
    all.push([`${label}, field`, over([0, 0, 0], LIGHT_FIELD, base)])
    LIGHT_SURFACES.forEach((alpha, i) => {
      all.push([`${label}, surface-${i + 1}`, over([0, 0, 0], alpha, base)])
    })
  }
  return all
}

describe('the light palette', () => {
  it('carries every level of text at AA on every ground', () => {
    const failing: string[] = []
    for (const token of TEXT) {
      const alpha = inkAlpha(declaration(light, token))
      for (const [where, ground] of lightGrounds()) {
        const ratio = contrast(over(INK, alpha, ground), ground)
        if (ratio < AA) failing.push(`${token} on ${where}: ${ratio.toFixed(2)}`)
      }
    }
    expect(failing).toEqual([])
  })

  it('keeps the four levels in order and apart', () => {
    const alphas = TEXT.map((token) => inkAlpha(declaration(light, token)))
    for (let i = 1; i < alphas.length; i += 1) {
      expect(alphas[i - 1]! - alphas[i]!).toBeGreaterThanOrEqual(0.08)
    }
  })

  it('carries the colours that mean something at AA on the worst ground', () => {
    const worst = over([0, 0, 0], LIGHT_SURFACES[2]!, over(LIGHT_WINDOW, WINDOW_ALPHA, [0, 0, 0]))
    const failing: string[] = []
    for (const token of SEMANTIC) {
      const ratio = contrast(hex(declaration(light, token)), worst)
      if (ratio < AA) failing.push(`${token}: ${ratio.toFixed(2)}`)
    }
    expect(failing).toEqual([])
  })
})

/* ---- the stylesheet ------------------------------------------------------- */

/**
 * The dark block, which is also `:root` — so it is cut at the next selector
 * rather than read to the end of the file, or the light values would be found
 * first by a later lookup.
 */
function darkBlock(css: string): string {
  const start = css.indexOf(":root[data-theme='dark'] {")
  if (start === -1) throw new Error('no dark block in tokens.css')
  const end = css.indexOf('\n}', start)
  const block = css.slice(start, end)
  // `--window-alpha` and the shape tokens live in the shared `:root` above it.
  const shared = css.slice(0, start)
  return `${shared}\n${block}`
}

/** The first light block: the palette, not the accent tints that follow it. */
function lightBlock(css: string): string {
  const start = css.indexOf(":root[data-theme='light'] {")
  if (start === -1) throw new Error('no light block in tokens.css')
  const end = css.indexOf('\n}', start)
  // `--window-alpha` lives in the shared `:root` before the dark block; the
  // dark block itself is left out, or its values would be read instead.
  const shared = css.slice(0, css.indexOf(":root[data-theme='dark'] {"))
  return `${shared}\n${css.slice(start, end)}`
}

/** `rgb(20 20 26 / 0.66)` → `0.66`. */
function inkAlpha(value: string): number {
  const match = /^rgb\(\s*20\s+20\s+26\s*\/\s*([\d.]+)\s*\)$/.exec(value)
  if (!match) throw new Error(`not the ink colour with an alpha: ${value}`)
  return Number(match[1])
}

function declaration(block: string, token: string): string {
  // The last one wins, as it would in the browser: the dark block is appended
  // to the shared one above.
  const matches = [...block.matchAll(new RegExp(`${token}:\\s*([^;]+);`, 'g'))]
  const last = matches.at(-1)
  if (!last) throw new Error(`no ${token} in tokens.css`)
  return last[1]!.trim()
}

type Rgb = [number, number, number]

/** `rgb(255 255 255 / 0.54)` → `0.54`. */
function whiteAlpha(value: string): number {
  const match = /^rgb\(\s*255\s+255\s+255\s*\/\s*([\d.]+)\s*\)$/.exec(value)
  if (!match) throw new Error(`not white with an alpha: ${value}`)
  return Number(match[1])
}

/** `rgb(0 0 0 / 0.28)` → `0.28`. */
function blackAlpha(value: string): number {
  const match = /^rgb\(\s*0\s+0\s+0\s*\/\s*([\d.]+)\s*\)$/.exec(value)
  if (!match) throw new Error(`not black with an alpha: ${value}`)
  return Number(match[1])
}

/** `8 8 11` → `[8, 8, 11]`, the bare triplet `--window` is declared as. */
function rgbTriplet(value: string): Rgb {
  const parts = value.split(/\s+/).map(Number)
  if (parts.length !== 3 || parts.some(Number.isNaN)) throw new Error(`not a triplet: ${value}`)
  return parts as Rgb
}

function hex(value: string): Rgb {
  const match = /^#([0-9a-f]{6})$/i.exec(value)
  if (!match) throw new Error(`not a six-digit hex colour: ${value}`)
  const digits = match[1]!
  return [0, 2, 4].map((i) => parseInt(digits.slice(i, i + 2), 16)) as Rgb
}

/* ---- the maths ------------------------------------------------------------ */

/** `fg` at `alpha` composited over `bg`, which is what the browser draws. */
function over(fg: Rgb, alpha: number, bg: Rgb): Rgb {
  return bg.map((channel, i) => alpha * fg[i]! + (1 - alpha) * channel) as Rgb
}

function contrast(a: Rgb, b: Rgb): number {
  const [lighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number]
  return (lighter + 0.05) / (darker + 0.05)
}

function luminance([r, g, b]: Rgb): number {
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

function channel(value: number): number {
  const c = value / 255
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
}
