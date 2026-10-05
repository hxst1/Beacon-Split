import type { ITheme } from '@xterm/xterm'

/**
 * The least contrast xterm lets a glyph have against what it is drawn on —
 * WCAG AA for body text, the floor the rest of the palette is held to.
 *
 * A palette only covers the sixteen colours a program names by number. Claude
 * Code and most TUIs pick their own colours in RGB for a ground they assume is
 * dark, so in the light theme they drew white on white. xterm can lift or
 * darken any glyph that falls below this, whatever colour it asked for.
 */
export const MINIMUM_CONTRAST = 4.5

/**
 * The sixteen colours on the light ground: GitHub Primer's light terminal,
 * with the brights and white taken down until every one clears AA against the
 * window. White and bright white are greys on purpose — programs use them for
 * text, assuming a dark terminal, far more often than as a background. Dark
 * keeps xterm's own palette, which was made for it.
 */
export const LIGHT_ANSI = {
  black: '#24292f',
  red: '#cf222e',
  green: '#116329',
  yellow: '#7d4e00',
  blue: '#0969da',
  magenta: '#8250df',
  cyan: '#1b7c83',
  white: '#656d76',
  brightBlack: '#57606a',
  brightRed: '#a40e26',
  brightGreen: '#1a7f37',
  brightYellow: '#633c01',
  brightBlue: '#0550ae',
  brightMagenta: '#6639ba',
  brightCyan: '#136061',
  brightWhite: '#424a53',
} as const satisfies ITheme

export interface TerminalLook {
  light: boolean
  /** `--window`, the three channels the window is painted with: `246 246 248`. */
  window: string
  accent: string
}

/**
 * xterm draws to a canvas and cannot read CSS, so its theme is built from the
 * same variables everything else uses and rebuilt when they change.
 */
export function terminalTheme({ light, window, accent }: TerminalLook): ITheme {
  return {
    // Transparent, so the panel's surface shows through instead of a flat
    // rectangle in the wrong colour for the palette. The channels still say
    // which colour that is: xterm ignores the alpha both when it measures
    // contrast and when a program asks for the background (OSC 11), so a zero
    // here would read as black to both — the light theme taken for a dark one.
    background: `rgba(${channels(window, light)}, 0)`,
    foreground: light ? 'rgba(20, 20, 26, 0.92)' : 'rgba(255, 255, 255, 0.88)',
    cursor: accent,
    cursorAccent: light ? '#ffffff' : '#08080b',
    selectionBackground: light ? 'rgba(0, 0, 0, 0.14)' : 'rgba(255, 255, 255, 0.16)',
    ...(light ? LIGHT_ANSI : {}),
  }
}

/** `246 246 248` as `246, 246, 248`, the only form xterm parses an alpha in. */
function channels(window: string, light: boolean): string {
  const parts = window.trim().split(/[\s,]+/).map(Number)
  if (parts.length === 3 && parts.every((n) => Number.isInteger(n) && n >= 0 && n <= 255)) {
    return parts.join(', ')
  }
  return light ? '246, 246, 248' : '8, 8, 11'
}
