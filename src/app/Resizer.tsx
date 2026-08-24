import { useCallback, useState } from 'react'

import styles from './Resizer.module.css'

interface ResizerProps {
  orientation: 'vertical' | 'horizontal'
  /** Called continuously while dragging, with a 0..1 fraction of the window. */
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
export function Resizer({ orientation, onDrag, onCommit }: ResizerProps): React.ReactElement {
  const [dragging, setDragging] = useState(false)

  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging) return
      // Panels are measured from the opposite edge: the side panel from the
      // right, the terminal from the bottom.
      const fraction =
        orientation === 'vertical'
          ? (window.innerWidth - event.clientX) / window.innerWidth
          : (window.innerHeight - event.clientY) / window.innerHeight
      onDrag(fraction)
    },
    [dragging, onDrag, orientation],
  )

  return (
    <div
      className={`${styles['resizer']} ${styles[orientation]}`}
      data-dragging={dragging}
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
