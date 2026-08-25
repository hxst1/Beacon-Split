/**
 * Pushes the active workspace's accent into CSS.
 *
 * Only the base colour is set; every tint and glow in the stylesheet is derived
 * from it with `color-mix`, so adding a workspace never means adding CSS.
 */
export function applyAccent(accent: string): void {
  document.documentElement.style.setProperty('--accent', accent)
}

/** A small, deliberately muted set for new workspaces. */
export const ACCENT_PRESETS = [
  { name: 'Iris', value: '#6b7cff' },
  { name: 'Violet', value: '#a06bff' },
  { name: 'Azure', value: '#3f9bff' },
  { name: 'Teal', value: '#28b3a6' },
  { name: 'Lime', value: '#7bc043' },
  { name: 'Amber', value: '#e0a33d' },
  { name: 'Coral', value: '#ff6f61' },
  { name: 'Rose', value: '#ff5c8a' },
] as const

/**
 * Icons a workspace can wear.
 *
 * Emoji rather than an icon set: nothing to bundle, nothing to license, and
 * they carry meaning a glyph from a library would not — a workspace called
 * "Red Bull" with a car on it is recognisable before it is read, which is the
 * whole point of the accent too.
 */
export const ICON_PRESETS = [
  '🏎️', '⚡', '🎮', '🛰️', '🧪', '📦', '🔧', '🎨',
  '🚀', '🌱', '🏛️', '💾', '📡', '🧭', '🔒', '☕',
] as const
