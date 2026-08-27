import type { PanelId } from '@/types/beacon'
import { usePanelFocus } from '../panelFocus'
import styles from './Panel.module.css'

interface PanelProps {
  /** Which panel this is. Also how focus finds it in the DOM. */
  id: PanelId
  title: string
  subtitle?: string
  actions?: React.ReactNode
  children: React.ReactNode
}

/**
 * The shared frame every panel sits in: a quiet header and a scrolling body.
 *
 * Whether it is focused is read from where the keyboard actually is, not
 * passed in. It used to be a prop, and what the prop said was that Claude was
 * focused always — so the marking was decoration rather than information, and
 * it could not have told you where your typing was going.
 *
 * `tabIndex={-1}` makes the frame focusable programmatically but keeps it out
 * of the Tab order, so moving between panels can land here without adding a
 * stop to every pass through the interface.
 */
export function Panel({ id, title, subtitle, actions, children }: PanelProps): React.ReactElement {
  const focused = usePanelFocus((state) => state.focused === id)

  return (
    <section className={styles['panel']} data-panel={id} data-focused={focused} tabIndex={-1}>
      <header className={styles['header']}>
        <span className={styles['title']}>{title}</span>
        {subtitle ? <span className={styles['subtitle']}>{subtitle}</span> : null}
        {actions}
      </header>
      <div className={styles['body']}>{children}</div>
    </section>
  )
}
