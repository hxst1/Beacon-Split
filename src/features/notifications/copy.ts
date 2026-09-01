import type { NotificationPermission } from '@/types/beacon'

export interface PermissionCopy {
  /** Two or three words, for the value column of a settings row. */
  label: string
  /** What it means and what to do about it, or `null` when nothing is wrong. */
  hint: string | null
  /** The button that changes it, when there is one that can. */
  action: 'ask' | 'openSettings' | null
}

/**
 * What to say about a permission state.
 *
 * Kept apart from the components so the wording is testable, and because the
 * same four cases have to read correctly in a settings row and in a first-run
 * prompt.
 */
export function describePermission(permission: NotificationPermission | null): PermissionCopy {
  switch (permission) {
    case 'authorized':
      return { label: 'Allowed', hint: null, action: null }
    case 'provisional':
      return {
        label: 'Quiet',
        hint: 'macOS is delivering these to Notification Centre without showing a banner, which defeats the point of them.',
        action: 'openSettings',
      }
    case 'denied':
      return {
        label: 'Blocked',
        hint: 'macOS asks once and never again, so this can only be undone in System Settings.',
        action: 'openSettings',
      }
    case 'notDetermined':
      return {
        label: 'Not asked',
        hint: 'macOS has not been asked yet. It offers the prompt once per application.',
        action: 'ask',
      }
    case 'unavailable':
      return {
        label: 'Unavailable',
        hint: 'This build is running unbundled, so macOS has no application to attribute a notification to. An installed build can ask.',
        action: null,
      }
    default:
      return { label: '…', hint: null, action: null }
  }
}
