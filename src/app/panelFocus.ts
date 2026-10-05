import { create } from 'zustand'

import { AGENT_PANELS } from '@/lib/layout'
import type { AgentKind, PanelId } from '@/types/beacon'

interface PanelFocusState {
  /** The panel the keyboard is in, or null when focus is elsewhere. */
  focused: PanelId | null
  /**
   * The agent panel the keyboard was in last.
   *
   * Kept after focus leaves, which is the whole point: clicking into the git
   * panel to read a diff should not stop that diff being the agent's. It is
   * the nearest thing to "the agent you are working in" that does not need
   * anybody to declare it.
   */
  lastAgent: AgentKind | null
  set: (panel: PanelId | null) => void
}

function agentOf(panel: PanelId | null): AgentKind | null {
  return panel !== null && (AGENT_PANELS as readonly PanelId[]).includes(panel)
    ? (panel as AgentKind)
    : null
}

/**
 * Which panel has the keyboard.
 *
 * Kept apart from the main store because focus changes on every click and
 * every tab, and nothing else should re-render for it. It is read from the DOM
 * rather than declared: whatever the user actually focused is the truth, so
 * clicking into the file tree, typing in a terminal and tabbing through the
 * commit form all say so without anything having to remember to.
 */
export const usePanelFocus = create<PanelFocusState>((set) => ({
  focused: null,
  lastAgent: null,
  // Guarded so a focus move inside the same panel is not a state change.
  set: (panel) =>
    set((state) => {
      if (state.focused === panel) return state
      const agent = agentOf(panel)
      return { focused: panel, lastAgent: agent ?? state.lastAgent }
    }),
}))

/**
 * The panel a step away from this one, wrapping at both ends.
 *
 * `order` is the panels as they are laid out, so moving on goes where the eye
 * would. With nothing focused yet, a step forward starts at the first panel and
 * a step back at the last, which is what makes the first press of the shortcut
 * do something sensible instead of nothing.
 */
export function panelAfter(
  order: readonly PanelId[],
  current: PanelId | null,
  step: number,
): PanelId | null {
  if (order.length === 0) return null

  const at = current === null ? -1 : order.indexOf(current)
  if (at === -1) return step > 0 ? (order[0] ?? null) : (order.at(-1) ?? null)

  const next = (at + step + order.length) % order.length
  return order[next] ?? null
}

/**
 * Puts the keyboard in a panel.
 *
 * Focus goes to what is useful inside it rather than to the frame: a terminal
 * wants the textarea xterm listens on, the editor wants its content, and a
 * panel with neither takes the focus itself so the border still moves and the
 * next Tab starts from the right place.
 */
export function focusPanel(panel: PanelId): void {
  const root = document.querySelector<HTMLElement>(`[data-panel="${panel}"]`)
  if (!root) return

  const target =
    root.querySelector<HTMLElement>('.xterm-helper-textarea') ??
    root.querySelector<HTMLElement>('.cm-content') ??
    root
  target.focus()
}
