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
  editor: 'Editor',
  files: 'Files',
  git: 'Git',
  terminal: 'Terminal',
}
