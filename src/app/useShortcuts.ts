import { useEffect } from 'react'

import { hasPrimaryModifier } from '@/lib/platform'
import { useBeacon } from './store'

/**
 * The keyboard layer.
 *
 * Bindings are expressed against "the primary modifier" rather than ⌘ or Ctrl,
 * so the same table is correct on macOS and Linux. This table will move into
 * user configuration in Milestone 6; the indirection is here from the start so
 * that change does not touch every call site.
 */
export function useShortcuts(): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (!hasPrimaryModifier(event)) return

      // Never steal keys from a field the user is typing in.
      const target = event.target as HTMLElement | null
      if (target?.tagName === 'INPUT' || target?.isContentEditable) return

      const store = useBeacon.getState()

      // ⌘1..⌘9 — jump straight to a project tab.
      if (/^[1-9]$/.test(event.key)) {
        event.preventDefault()
        void store.selectProjectAt(Number(event.key) - 1)
        return
      }

      switch (event.key.toLowerCase()) {
        // Files and Git are separate panels now, so they toggle separately.
        case 'e':
          event.preventDefault()
          void store.togglePanel('files')
          break
        case 'g':
          event.preventDefault()
          void store.togglePanel('git')
          break
        case 'j':
          event.preventDefault()
          void store.togglePanel('terminal')
          break
        case 'o':
          event.preventDefault()
          void store.togglePanel('editor')
          break
        case 'enter':
          event.preventDefault()
          store.toggleFullscreen(store.fullscreenPanel ?? 'claude')
          break
        default:
          break
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
}
