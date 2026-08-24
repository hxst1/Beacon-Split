import styles from './Placeholder.module.css'

interface PlaceholderProps {
  /** What will live here, phrased as the feature not the absence of it. */
  text: string
  milestone: string
  detail?: string
  centered?: boolean
}

/**
 * Stands in for a panel that has not been built yet.
 *
 * It names the milestone rather than saying "coming soon", so an unfinished
 * panel reads as a known gap rather than a broken one.
 */
export function Placeholder({
  text,
  milestone,
  detail,
  centered = true,
}: PlaceholderProps): React.ReactElement {
  return (
    <div className={`${styles['root']} ${centered ? styles['centered'] : ''}`}>
      <span className={styles['milestone']}>{milestone}</span>
      <span>{text}</span>
      {detail ? <span className={styles['detail']}>{detail}</span> : null}
    </div>
  )
}
