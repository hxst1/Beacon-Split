import { getCurrentWindow } from '@tauri-apps/api/window'

import type { Appearance, Theme } from '@/types/beacon'

/** What `system` resolves to right now. */
export function resolveTheme(theme: Theme): 'dark' | 'light' {
  if (theme !== 'system') return theme
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

/**
 * Puts the look on the document.
 *
 * Everything visual reads these: the palette from `data-theme` and translucency
 * from one variable. Changing either is a repaint, not a reload — which is what
 * makes dragging the slider feel like adjusting the window rather than
 * configuring it.
 *
 * Frosting is not here. It is a window effect the shell applies, because
 * nothing inside the page can blur what is behind the window.
 */
export function applyAppearance(appearance: Appearance): 'dark' | 'light' {
  const resolved = resolveTheme(appearance.theme)
  const root = document.documentElement

  root.dataset['theme'] = resolved
  root.style.setProperty('--window-alpha', String(appearance.windowOpacity))

  // The window frame is drawn by the system, not by us — traffic lights, and
  // the material behind a frosted window. Left alone it follows the operating
  // system, which is wrong the moment someone picks a theme that does not.
  void getCurrentWindow()
    .setTheme(resolved)
    .catch(() => {
      // Not worth an error in the way: the palette is already applied.
    })

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
