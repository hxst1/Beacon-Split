import { useEffect, useRef } from 'react'

import styles from './Popover.module.css'

interface PopoverProps {
  anchor: DOMRect
  onClose: () => void
  children: React.ReactNode
  /** Which edge of the anchor the panel lines up with. */
  align?: 'start' | 'end'
}

/**
 * A floating panel positioned against an element's bounding box.
 *
 * Deliberately not a portal library: Beacon has one window and a handful of
 * menus, and a full popover dependency would be more surface than the whole
 * feature.
 */
export function Popover({
  anchor,
  onClose,
  children,
  align = 'start',
}: PopoverProps): React.ReactElement {
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [onClose])

  // Keep the panel inside the window when the anchor sits near an edge.
  const style: React.CSSProperties =
    align === 'end'
      ? { top: anchor.bottom + 6, right: Math.max(8, window.innerWidth - anchor.right) }
      : { top: anchor.bottom + 6, left: Math.min(anchor.left, window.innerWidth - 240) }

  return (
    <div className={styles['layer']} onPointerDown={onClose}>
      <div
        ref={panelRef}
        className={styles['panel']}
        style={style}
        role="menu"
        onPointerDown={(event) => event.stopPropagation()}
      >
        {children}
      </div>
    </div>
  )
}
