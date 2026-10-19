import type { TerminalFont } from '@/types/beacon'

/**
 * The families a terminal tries when nobody has chosen one, in order.
 *
 * Nerd Fonts first, because a prompt drawn with their icons — Powerlevel10k,
 * starship, eza — is only right when the font that has the icons is the one
 * being drawn, and somebody with one installed chose it for exactly that. The
 * `Mono` variants, whose icons are one cell wide, which is what the grid
 * assumes. A Nerd Font installs under its own name (`JetBrainsMono Nerd Font
 * Mono`, not `JetBrains Mono`), so the plain names further down never find it.
 *
 * `Symbols Nerd Font Mono` goes last before the generic: it has nothing but the
 * icons, so whatever base font won still gets them from one consistent source
 * instead of glyph by glyph from whatever else has them.
 */
export const DEFAULT_FONT_STACK = [
  "'JetBrainsMono Nerd Font Mono'",
  "'JetBrainsMono NFM'",
  "'MesloLGS NF'",
  "'SF Mono'",
  "'JetBrains Mono'",
  'Menlo',
  "'Cascadia Mono'",
  'Consolas',
  "'DejaVu Sans Mono'",
  "'Symbols Nerd Font Mono'",
  'monospace',
].join(', ')

/**
 * The CSS font list for a terminal.
 *
 * A chosen family goes in front of the defaults rather than in place of them,
 * so a name this machine does not have still lands on something monospaced.
 * The backend refuses a name with a quote in it, which is what makes quoting
 * it here safe.
 */
export function fontStack(font: TerminalFont): string {
  const family = font.family?.trim()
  return family ? `"${family}", ${DEFAULT_FONT_STACK}` : DEFAULT_FONT_STACK
}
