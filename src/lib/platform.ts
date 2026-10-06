import type { HostPlatform } from '@/types/beacon'

/**
 * Shortcut handling is abstracted behind "the primary modifier" so the same
 * binding table works on macOS (⌘) and on Linux and Windows (Ctrl).
 *
 * The backend's answer is authoritative and replaces this guess as soon as it
 * arrives. The guess exists for the moments before that — and for a boot that
 * fails before it — because on Windows the window draws its own close button,
 * and a window that cannot be closed is not a detail.
 */
let platform: HostPlatform = guessPlatform()

function guessPlatform(): HostPlatform {
  const agent = typeof navigator === 'undefined' ? '' : navigator.userAgent
  if (agent.includes('Windows')) return 'windows'
  if (agent.includes('Linux')) return 'linux'
  return 'macos'
}

export function setPlatform(value: HostPlatform): void {
  platform = value
  document.documentElement.dataset['platform'] = value
}

export function isMac(): boolean {
  return platform === 'macos'
}

/** Windows, which differs from Linux in the terminal and in the pseudo-console. */
export function isWindows(): boolean {
  return platform === 'windows'
}

/**
 * Whether Beacon draws its own minimise, maximise and close buttons.
 *
 * Everywhere but macOS, which keeps its traffic lights under an overlay title
 * bar. Windows has no such overlay, and on Linux a system title bar would sit
 * above Beacon's own as a second row of chrome — so both windows are
 * undecorated and the buttons are Beacon's to draw. The capability that allows
 * them names the same two platforms.
 */
export function drawsOwnWindowControls(): boolean {
  return !isMac()
}

let windowsBuild: number | null = null

export function setWindowsBuild(value: number | null): void {
  windowsBuild = value
}

/**
 * What xterm needs to know about a Windows pseudo-console, or nothing at all
 * elsewhere. With the build it can tell whether the console reflows lines on
 * resize itself, which it does from build 21376.
 */
export function windowsPty(): { backend: 'conpty'; buildNumber?: number } | undefined {
  if (!isWindows()) return undefined
  return windowsBuild === null ? { backend: 'conpty' } : { backend: 'conpty', buildNumber: windowsBuild }
}

/** True when the event carries the platform's primary modifier. */
export function hasPrimaryModifier(event: KeyboardEvent): boolean {
  return isMac() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
}

export const modifierLabel = (): string => (isMac() ? '⌘' : 'Ctrl')

export function shortcutLabel(key: string): string {
  return isMac() ? `⌘${key}` : `Ctrl+${key}`
}
