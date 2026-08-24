import styles from './Menu.module.css'

interface MenuItemProps {
  label: string
  onSelect: () => void
  hint?: string | undefined
  /** Renders a coloured dot — used for workspace accents. */
  dot?: string | undefined
  active?: boolean
  danger?: boolean
}

export function MenuItem({
  label,
  onSelect,
  hint,
  dot,
  active = false,
  danger = false,
}: MenuItemProps): React.ReactElement {
  return (
    <button
      type="button"
      role="menuitem"
      className={styles['item']}
      data-active={active}
      data-danger={danger}
      onClick={onSelect}
    >
      {dot ? <span className={styles['dot']} style={{ background: dot, color: dot }} /> : null}
      <span className={styles['label']}>{label}</span>
      {hint ? <span className={styles['hint']}>{hint}</span> : null}
    </button>
  )
}

export const MenuSeparator = (): React.ReactElement => <div className={styles['separator']} />

export const MenuHeading = ({ children }: { children: React.ReactNode }): React.ReactElement => (
  <div className={styles['heading']}>{children}</div>
)
