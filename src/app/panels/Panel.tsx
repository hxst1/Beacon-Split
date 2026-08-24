import styles from './Panel.module.css'

interface PanelProps {
  title: string
  subtitle?: string
  focused?: boolean
  actions?: React.ReactNode
  children: React.ReactNode
}

/** The shared frame every panel sits in: a quiet header and a scrolling body. */
export function Panel({
  title,
  subtitle,
  focused = false,
  actions,
  children,
}: PanelProps): React.ReactElement {
  return (
    <section className={styles['panel']} data-focused={focused}>
      <header className={styles['header']}>
        <span className={styles['title']}>{title}</span>
        {subtitle ? <span className={styles['subtitle']}>{subtitle}</span> : null}
        {actions}
      </header>
      <div className={styles['body']}>{children}</div>
    </section>
  )
}
