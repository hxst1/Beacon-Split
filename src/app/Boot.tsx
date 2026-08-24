import styles from './Boot.module.css'

/** The pre-UI state: either loading, or a load failure we cannot recover from. */
export function Boot({ error }: { error?: string | null }): React.ReactElement {
  return (
    <div className={styles['root']}>
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
