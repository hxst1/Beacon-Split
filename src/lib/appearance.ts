import type { Appearance, Theme } from '@/types/beacon'

/** What `system` resolves to right now. */
export function resolveTheme(theme: Theme): 'dark' | 'light' {
  if (theme !== 'system') return theme
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

/**
 * Puts the look on the document.
 *
 * Everything visual reads these: the palette from `data-theme`, translucency
 * and blur from two variables. Changing any of them is a repaint, not a reload
 * — which is what makes dragging a slider feel like adjusting the window rather
 * than configuring it.
 */
export function applyAppearance(appearance: Appearance): 'dark' | 'light' {
  const resolved = resolveTheme(appearance.theme)
  const root = document.documentElement

  root.dataset['theme'] = resolved
  root.style.setProperty('--window-alpha', String(appearance.windowOpacity))
  root.style.setProperty('--blur-px', `${appearance.blur}px`)

  return resolved
}

/**
 * Calls back when the system palette changes.
 *
 * Only matters while the theme is `system`, but subscribing unconditionally is
 * simpler than subscribing and unsubscribing as the setting moves.
 */
export function watchSystemTheme(onChange: () => void): () => void {
  const query = window.matchMedia('(prefers-color-scheme: light)')
  query.addEventListener('change', onChange)
  return () => query.removeEventListener('change', onChange)
}

/** Reads a CSS custom property, for the two components that draw their own. */
export function cssValue(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
  return value || fallback
}
