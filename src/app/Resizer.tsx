import { useCallback, useState } from 'react'

import styles from './Resizer.module.css'

interface ResizerProps {
  orientation: 'vertical' | 'horizontal'
  /**
   * Which edge the fraction is measured from. `end` suits panels docked to the
   * right or bottom; `start` suits a panel docked to the top of its container.
   */
  from?: 'start' | 'end'
  /**
   * What the fraction is relative to. `window` for the top-level layout,
   * `parent` for a split inside a column.
   */
  within?: 'window' | 'parent'
  /** Grid area to place the splitter in, when its container is a grid. */
  area?: string
  /** Called continuously while dragging, with a 0..1 fraction. */
  onDrag: (fraction: number) => void
  /** Called once, on release — the only point at which we persist. */
  onCommit: () => void
}

/**
 * A splitter driven by pointer capture.
 *
 * Dragging updates layout locally at pointer rate; the state is written to disk
 * only on release, so a drag is never a burst of file writes.
 */
export function Resizer({
  orientation,
  from = 'end',
  within = 'window',
  area,
  onDrag,
  onCommit,
}: ResizerProps): React.ReactElement {
  const [dragging, setDragging] = useState(false)

  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging) return

      const parent = event.currentTarget.parentElement
      const box =
        within === 'parent' && parent
          ? parent.getBoundingClientRect()
          : new DOMRect(0, 0, window.innerWidth, window.innerHeight)

      const position =
        orientation === 'vertical'
          ? (event.clientX - box.left) / box.width
          : (event.clientY - box.top) / box.height

      onDrag(from === 'start' ? position : 1 - position)
    },
    [dragging, from, onDrag, orientation, within],
  )

  return (
    <div
      className={`${styles['resizer']} ${styles[orientation]}`}
      data-dragging={dragging}
      style={area ? { gridArea: area } : undefined}
      role="separator"
      aria-orientation={orientation}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId)
        setDragging(true)
      }}
      onPointerMove={onPointerMove}
      onPointerUp={(event) => {
        event.currentTarget.releasePointerCapture(event.pointerId)
        setDragging(false)
        onCommit()
      }}
    />
  )
}
