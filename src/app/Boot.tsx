import { WindowControls } from './WindowControls'
import styles from './Boot.module.css'

/** The pre-UI state: either loading, or a load failure we cannot recover from. */
export function Boot({ error }: { error?: string | null }): React.ReactElement {
  return (
    <div className={styles['root']}>
      {/* Where the window has no system title bar, this is the only way to
          move it or close it before the real one exists — or when it never
          will, because loading failed. */}
      <div className={styles['chrome']} data-tauri-drag-region>
        <WindowControls />
      </div>
      {error ? (
        <div className={styles['error']}>
          <div className={styles['errorTitle']}>Beacon could not start</div>
          {error}
        </div>
      ) : (
        <div className={styles['mark']} />
      )}
    </div>
  )
}
