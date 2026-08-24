import type { HostPlatform } from '@/types/beacon'

/**
 * Shortcut handling is abstracted behind "the primary modifier" so the same
 * binding table works on macOS (⌘) and Linux (Ctrl).
 */
let platform: HostPlatform = 'macos'

export function setPlatform(value: HostPlatform): void {
  platform = value
  document.documentElement.dataset['platform'] = value
}

export function isMac(): boolean {
  return platform === 'macos'
}

/** True when the event carries the platform's primary modifier. */
export function hasPrimaryModifier(event: KeyboardEvent): boolean {
  return isMac() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
}

export const modifierLabel = (): string => (isMac() ? '⌘' : 'Ctrl')

export function shortcutLabel(key: string): string {
  return isMac() ? `⌘${key}` : `Ctrl+${key}`
}
