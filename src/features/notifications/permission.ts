import { useCallback, useEffect, useRef, useState } from 'react'

import { ipc } from '@/ipc'
import type { NotificationPermission } from '@/types/beacon'

/**
 * Beacon's side of the macOS notification permission.
 *
 * The state is not Beacon's to hold: it lives in System Settings, where it can
 * be changed while Beacon is running and without telling it. So this asks
 * rather than remembers, and asks again whenever the window comes back — which
 * is exactly when someone returns from having changed it.
 */

/** How often to look again while waiting for an answer to the system prompt. */
const POLL_MS = 500

/** How long to keep looking. The prompt is a window in front of a person. */
const POLL_LIMIT_MS = 120_000

export interface PermissionState {
  /** `null` until the first answer arrives. */
  permission: NotificationPermission | null
  /** True while the system prompt is expected to be on screen. */
  asking: boolean
  refresh: () => Promise<void>
  request: () => Promise<void>
  openSettings: () => Promise<void>
}

export function useNotificationPermission(): PermissionState {
  const [permission, setPermission] = useState<NotificationPermission | null>(null)
  const [asking, setAsking] = useState(false)
  const timer = useRef<number | null>(null)

  const refresh = useCallback(async () => {
    try {
      setPermission(await ipc.notificationPermission())
    } catch {
      // A window that cannot reach the backend has larger problems, and the
      // status bar is already saying so.
    }
  }, [])

  useEffect(() => {
    void refresh()
    // Coming back to the window is the one moment worth re-reading this: it is
    // what someone does after switching to System Settings to allow it.
    const onFocus = (): void => void refresh()
    window.addEventListener('focus', onFocus)
    return () => {
      window.removeEventListener('focus', onFocus)
      if (timer.current !== null) window.clearInterval(timer.current)
    }
  }, [refresh])

  const request = useCallback(async () => {
    setAsking(true)
    await ipc.requestNotificationPermission()

    // The command returns as soon as macOS has been asked, because the answer
    // is a person's, and no IPC call should wait on one. So the outcome is
    // read by looking, until it stops being `notDetermined` or long enough has
    // passed that nobody is coming back to it.
    const started = Date.now()
    if (timer.current !== null) window.clearInterval(timer.current)
    timer.current = window.setInterval(() => {
      void ipc
        .notificationPermission()
        .then((current) => {
          const settled = current !== 'notDetermined'
          if (settled || Date.now() - started > POLL_LIMIT_MS) {
            if (timer.current !== null) window.clearInterval(timer.current)
            timer.current = null
            setAsking(false)
          }
          setPermission(current)
        })
        .catch(() => {})
    }, POLL_MS)
  }, [])

  const openSettings = useCallback(async () => {
    await ipc.openNotificationSettings()
  }, [])

  return { permission, asking, refresh, request, openSettings }
}
