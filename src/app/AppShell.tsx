import { ClipDrawer } from '@/features/clips/ClipDrawer'
import { CommandPalette } from '@/features/palette/CommandPalette'
import { QuickOpen } from '@/features/palette/QuickOpen'
import { SettingsScreen } from '@/features/settings/SettingsScreen'
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

  return (
    <div className={styles['shell']}>
      <AccentFrame />
      <TitleBar />
      <Workbench />
      <StatusBar />

      {/* Outside the workbench on purpose: it overlays the layout rather than
          taking a share of it, so opening it costs no terminal width. */}
      <ClipDrawer />

      {overlay === 'palette' ? <CommandPalette onClose={close} /> : null}
      {overlay === 'quickOpen' ? <QuickOpen onClose={close} /> : null}
      {overlay === 'settings' ? <SettingsScreen onClose={close} /> : null}
    </div>
  )
}
