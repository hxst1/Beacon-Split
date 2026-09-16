import type { LayoutNode, PanelId } from '@/types/beacon'

/** Which child of a split to descend into. */
export type Step = 'first' | 'second'
/** A route from the root of the tree to one node. */
export type Path = Step[]

/**
 * Removes hidden panels, collapsing any split left with a single child.
 *
 * Hiding is a view concern: the stored tree keeps every panel in place, so
 * showing one again puts it back exactly where it was rather than somewhere
 * plausible.
 */
export function prune(node: LayoutNode, hidden: readonly PanelId[]): LayoutNode | null {
  if (node.type === 'panel') {
    return hidden.includes(node.panel) ? null : node
  }

  const first = prune(node.first, hidden)
  const second = prune(node.second, hidden)
  if (!first) return second
  if (!second) return first
  return { ...node, first, second }
}

/**
 * Translates a path in the pruned tree back into the tree it was pruned from.
 *
 * The two are not the same shape. `prune` collapses a split that lost a child,
 * so the moment any panel is hidden the splitters on screen sit at different
 * paths from the splits that hold their fractions. Dragging one used to write
 * into whatever the same path happened to name in the stored tree — a split
 * that was not on screen, or a panel, which is no split at all and swallowed
 * the change. Either way the panels did not move, and no amount of dragging
 * helped, because the thing being resized was never the thing being dragged.
 *
 * Returns `null` when the path names nothing, which is what a caller should
 * ignore rather than guess about.
 */
export function sourcePath(node: LayoutNode, hidden: readonly PanelId[], path: Path): Path | null {
  const taken: Path = []
  let current = node
  let step = 0

  for (;;) {
    if (current.type === 'panel') return null

    const first = prune(current.first, hidden)
    const second = prune(current.second, hidden)
    if (!first && !second) return null

    // A split with one surviving child is not in the pruned tree at all, so it
    // answers to no step: walk through it without spending one.
    if (!first || !second) {
      const surviving: Step = first ? 'first' : 'second'
      taken.push(surviving)
      current = surviving === 'first' ? current.first : current.second
      continue
    }

    // A split the user can see. If the path ends here, this is the one.
    if (step === path.length) return taken

    const next = path[step]
    if (!next) return null
    step += 1
    taken.push(next)
    current = next === 'first' ? current.first : current.second
  }
}

/** Returns a copy of the tree with one split's fraction replaced. */
export function withFraction(node: LayoutNode, path: Path, fraction: number): LayoutNode {
  if (node.type === 'panel') return node

  if (path.length === 0) {
    return { ...node, fraction: clamp(fraction, 0.1, 0.9) }
  }

  const [step, ...rest] = path
  return step === 'first'
    ? { ...node, first: withFraction(node.first, rest, fraction) }
    : { ...node, second: withFraction(node.second, rest, fraction) }
}

/** Every panel in the tree, in layout order. */
export function panelsOf(node: LayoutNode): PanelId[] {
  return node.type === 'panel' ? [node.panel] : [...panelsOf(node.first), ...panelsOf(node.second)]
}

export const clamp = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, value))

export const PANEL_LABELS: Record<PanelId, string> = {
  claude: 'Claude',
  codex: 'Codex',
  editor: 'Editor',
  files: 'Files',
  git: 'Git',
  terminal: 'Terminal',
}
