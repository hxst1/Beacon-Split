import { useEffect } from 'react'

import { ClipDrawer } from '@/features/clips/ClipDrawer'
import { UnsavedOnQuit } from '@/features/editor/UnsavedOnQuit'
import { NotificationPrompt } from '@/features/notifications/NotificationPrompt'
import { CommandPalette } from '@/features/palette/CommandPalette'
import { QuickOpen } from '@/features/palette/QuickOpen'
import { SettingsScreen } from '@/features/settings/SettingsScreen'
import { WelcomeGuide } from '@/features/welcome/WelcomeGuide'
import { AccentFrame } from './AccentFrame'
import { StatusBar } from './StatusBar'
import { TitleBar } from './TitleBar'
import { Workbench } from './Workbench'
import { useBeacon } from './store'
import { useShortcuts } from './useShortcuts'
import styles from './AppShell.module.css'

/** The main window: title bar, workbench, status bar — plus the accent signal. */
export function AppShell(): React.ReactElement {
  useShortcuts()
  const overlay = useBeacon((s) => s.overlay)
  const setOverlay = useBeacon((s) => s.setOverlay)
  const close = (): void => setOverlay(null)

  // A fresh install opens on the welcome guide, once the first workspace
  // exists and there is a window to walk round.
  const welcomed = useBeacon((s) => s.snapshot?.welcomed ?? true)
  useEffect(() => {
    if (!welcomed) setOverlay('welcome')
  }, [welcomed, setOverlay])

  return (
    <div className={styles['shell']}>
      <AccentFrame />
      <TitleBar />
      <Workbench />
      <StatusBar />

      {/* Outside the workbench on purpose: it overlays the layout rather than
          taking a share of it, so opening it costs no terminal width. */}
      <ClipDrawer />

      {/* Asks for the macOS notification permission, once, and only while
          macOS has never been asked — after the welcome guide, on a first run,
          rather than on top of it. */}
      <NotificationPrompt held={overlay === 'welcome'} />

      {/* Quitting is the one action that can throw away work which exists
          nowhere else, so it is the one action Beacon asks about. */}
      <UnsavedOnQuit />

      {overlay === 'palette' ? <CommandPalette onClose={close} /> : null}
      {overlay === 'quickOpen' ? <QuickOpen onClose={close} /> : null}
      {overlay === 'settings' ? <SettingsScreen onClose={close} /> : null}
      {overlay === 'welcome' ? <WelcomeGuide onClose={close} /> : null}
    </div>
  )
}
