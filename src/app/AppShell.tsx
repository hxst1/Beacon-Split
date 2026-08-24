import { AccentFrame } from './AccentFrame'
import { StatusBar } from './StatusBar'
import { TitleBar } from './TitleBar'
import { Workbench } from './Workbench'
import { useShortcuts } from './useShortcuts'
import styles from './AppShell.module.css'

/** The main window: title bar, workbench, status bar — plus the accent signal. */
export function AppShell(): React.ReactElement {
  useShortcuts()

  return (
    <div className={styles['shell']}>
      <AccentFrame />
      <TitleBar />
      <Workbench />
      <StatusBar />
    </div>
  )
}
