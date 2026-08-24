import type { LayoutNode, PanelId } from '@/types/beacon'
import type { Path } from '@/lib/layout'
import { Resizer } from './Resizer'
import styles from './LayoutView.module.css'

interface LayoutViewProps {
  node: LayoutNode
  /** Renders one panel. Kept out of here so the tree knows nothing about panels. */
  render: (panel: PanelId) => React.ReactNode
  /** Called while dragging a splitter, with the path to it. */
  onResize: (path: Path, fraction: number) => void
  onCommit: () => void
  path?: Path
}

/**
 * Renders a layout tree.
 *
 * Each split is a three-track grid — first child, splitter, rest — so the
 * splitter is the border rather than sitting next to one. Recursion means a
 * preset and a hand-arranged layout take exactly the same path through here.
 */
export function LayoutView({
  node,
  render,
  onResize,
  onCommit,
  path = [],
}: LayoutViewProps): React.ReactElement {
  if (node.type === 'panel') {
    return <>{render(node.panel)}</>
  }

  const style = { '--first': `${node.fraction * 100}%` } as React.CSSProperties

  return (
    <div className={`${styles['split']} ${styles[node.direction]}`} style={style}>
      <div className={styles['cell']}>
        <LayoutView
          node={node.first}
          render={render}
          onResize={onResize}
          onCommit={onCommit}
          path={[...path, 'first']}
        />
      </div>

      <Resizer
        orientation={node.direction === 'row' ? 'vertical' : 'horizontal'}
        from="start"
        within="parent"
        onDrag={(fraction) => onResize(path, fraction)}
        onCommit={onCommit}
      />

      <div className={styles['cell']}>
        <LayoutView
          node={node.second}
          render={render}
          onResize={onResize}
          onCommit={onCommit}
          path={[...path, 'second']}
        />
      </div>
    </div>
  )
}
