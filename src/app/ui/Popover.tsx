import { useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'

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
 * Rendered into `document.body` rather than where it is written. Menus are
 * opened from chrome that blurs what is behind it, and a `backdrop-filter`
 * makes an element the containing block for fixed-position descendants *and* a
 * stacking context — so a menu left in place was confined to a 42-pixel-tall
 * title bar and painted underneath the panels below it. It was visible, and
 * every click went to whatever was on top of it.
 *
 * No dependency for this: a portal is part of React, and what would have been
 * pulled in is positioning logic that fits in ten lines.
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

  return createPortal(
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
    </div>,
    document.body,
  )
}
