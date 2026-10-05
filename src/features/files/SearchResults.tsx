import { useEffect, useRef } from 'react'

import { FileIcon } from './FileIcon'
import type { FileHit } from './fileSearch'
import styles from './FileTree.module.css'

/**
 * What the file panel shows while something is typed in its search field.
 *
 * In place of the tree rather than over it: the panel is narrow, and a list of
 * matches is the tree's job for as long as the search lasts. The keyboard stays
 * in the field — arrows move through the matches, Return opens one — so this
 * draws the active row rather than taking focus.
 */
export function SearchResults({
  hits,
  active,
  status,
  onHover,
  onChoose,
}: {
  hits: FileHit[]
  active: number
  /** Why there are no hits, when there are none. */
  status: string
  onHover: (index: number) => void
  onChoose: (path: string) => void
}): React.ReactElement {
  const rows = useRef<Array<HTMLButtonElement | null>>([])

  useEffect(() => {
    rows.current[active]?.scrollIntoView({ block: 'nearest' })
  }, [active])

  if (hits.length === 0) return <div className={styles['status']}>{status}</div>

  return (
    <div role="listbox" id="file-search-results" aria-label="Matching files">
      {hits.map((hit, index) => (
        <button
          key={hit.path}
          ref={(node) => {
            rows.current[index] = node
          }}
          id={`file-search-${index}`}
          type="button"
          role="option"
          tabIndex={-1}
          aria-selected={index === active}
          className={`${styles['row']} ${styles['hitRow']}`}
          data-selected={index === active}
          title={hit.path}
          // The field keeps focus; a click only says which one.
          onMouseDown={(event) => event.preventDefault()}
          onMouseMove={() => onHover(index)}
          onClick={() => onChoose(hit.path)}
        >
          <FileIcon name={hit.name} isDirectory={false} className={styles['icon']} />
          <span className={styles['name']}>
            <Highlighted text={hit.name} positions={hit.positions} />
          </span>
          {hit.dir ? <span className={styles['hitDir']}>{hit.dir}</span> : null}
        </button>
      ))}
    </div>
  )
}

/** The name, with the characters the query matched marked. */
function Highlighted({ text, positions }: { text: string; positions: number[] }): React.ReactElement {
  const marked = new Set(positions)
  const parts: React.ReactNode[] = []
  let run = ''
  let runMarked = false

  const flush = (at: number): void => {
    if (!run) return
    parts.push(
      runMarked ? (
        <mark key={at} className={styles['hit']}>
          {run}
        </mark>
      ) : (
        run
      ),
    )
    run = ''
  }

  for (let index = 0; index < text.length; index += 1) {
    const isMarked = marked.has(index)
    if (isMarked !== runMarked) {
      flush(index)
      runMarked = isMarked
    }
    run += text[index]
  }
  flush(text.length)

  return <>{parts}</>
}
