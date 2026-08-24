import type { LayoutNode } from '@/types/beacon'
import styles from './LayoutThumb.module.css'

/**
 * A miniature of a layout, drawn from the same tree the workbench renders.
 *
 * Building it from the real tree rather than from a hand-drawn icon means a
 * preview cannot end up describing an arrangement that no longer exists.
 */
export function LayoutThumb({ node }: { node: LayoutNode }): React.ReactElement {
  return (
    <div className={styles['thumb']}>
      <Region node={node} />
    </div>
  )
}

function Region({ node }: { node: LayoutNode }): React.ReactElement {
  if (node.type === 'panel') {
    return <div className={styles['region']} data-panel={node.panel} />
  }

  const percent = `${Math.round(node.fraction * 100)}%`
  const style =
    node.direction === 'row'
      ? { gridTemplateColumns: `${percent} 1fr` }
      : { gridTemplateRows: `${percent} 1fr` }

  return (
    <div className={styles['split']} style={style}>
      <Region node={node.first} />
      <Region node={node.second} />
    </div>
  )
}
