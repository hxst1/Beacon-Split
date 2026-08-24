import { useEffect, useRef, useState } from 'react'

import styles from './Overlay.module.css'

export interface OverlayItem {
  id: string
  label: string
  /** Which characters of `label` matched, for highlighting. */
  positions?: number[] | undefined
  /** Secondary text, e.g. the directory a file is in. */
  context?: string | undefined
  /** Right-aligned text, e.g. a shortcut. */
  hint?: string | undefined
}

interface OverlayProps {
  placeholder: string
  items: OverlayItem[]
  query: string
  onQueryChange: (query: string) => void
  onChoose: (id: string) => void
  onClose: () => void
  emptyMessage?: string
}

/**
 * The shell both the command palette and quick open sit in.
 *
 * Fully keyboard-driven, which is the point: arrows move, Enter chooses, Escape
 * leaves, and the mouse is never required.
 */
export function Overlay({
  placeholder,
  items,
  query,
  onQueryChange,
  onChoose,
  onClose,
  emptyMessage = 'No matches',
}: OverlayProps): React.ReactElement {
  const [active, setActive] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)

  // A new query means a new list; the selection belongs at the top of it.
  useEffect(() => setActive(0), [query])

  useEffect(() => {
    const element = listRef.current?.children[active]
    element?.scrollIntoView({ block: 'nearest' })
  }, [active])

  return (
    <div className={styles['scrim']} onPointerDown={onClose}>
      <div className={styles['panel']} onPointerDown={(event) => event.stopPropagation()}>
        <input
          className={styles['input']}
          placeholder={placeholder}
          value={query}
          autoFocus
          spellCheck={false}
          onChange={(event) => onQueryChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown' || (event.key === 'n' && event.ctrlKey)) {
              event.preventDefault()
              setActive((index) => Math.min(index + 1, items.length - 1))
            } else if (event.key === 'ArrowUp' || (event.key === 'p' && event.ctrlKey)) {
              event.preventDefault()
              setActive((index) => Math.max(index - 1, 0))
            } else if (event.key === 'Enter') {
              event.preventDefault()
              const chosen = items[active]
              if (chosen) onChoose(chosen.id)
            } else if (event.key === 'Escape') {
              event.preventDefault()
              onClose()
            }
          }}
        />

        <div className={styles['results']} ref={listRef}>
          {items.length === 0 ? <div className={styles['empty']}>{emptyMessage}</div> : null}
          {items.map((item, index) => (
            <button
              key={item.id}
              type="button"
              className={styles['item']}
              data-active={index === active}
              onPointerEnter={() => setActive(index)}
              onClick={() => onChoose(item.id)}
            >
              <span className={styles['label']}>
                <Highlighted text={item.label} positions={item.positions} />
              </span>
              {item.context ? <span className={styles['context']}>{item.context}</span> : null}
              {item.hint ? <span className={styles['hint']}>{item.hint}</span> : null}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}

/** Marks the characters that matched, so a result explains itself. */
function Highlighted({
  text,
  positions,
}: {
  text: string
  positions?: number[] | undefined
}): React.ReactElement {
  if (!positions || positions.length === 0) return <>{text}</>

  const marked = new Set(positions)
  const parts: React.ReactNode[] = []
  let run = ''
  let runIsHit = marked.has(0)

  for (let index = 0; index < text.length; index += 1) {
    const isHit = marked.has(index)
    if (isHit !== runIsHit) {
      parts.push(
        runIsHit ? (
          <span key={index} className={styles['hit']}>
            {run}
          </span>
        ) : (
          run
        ),
      )
      run = ''
      runIsHit = isHit
    }
    run += text[index]
  }
  parts.push(runIsHit ? <span key="last" className={styles['hit']}>{run}</span> : run)

  return <>{parts}</>
}
