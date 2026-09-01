import { useEditor } from '@/features/editor/openFiles'
import { panelsOf, prune } from '@/lib/layout'
import { hasPrimaryModifier, isMac } from '@/lib/platform'
import type { ActionBinding, PanelId } from '@/types/beacon'
import { focusPanel, panelAfter, usePanelFocus } from './panelFocus'
import { selectHidden, selectLayout, useBeacon } from './store'

/**
 * What each bindable action does.
 *
 * The catalogue of actions and their defaults lives in the backend, which is
 * what lets conflict checking have one source of truth. What they *do* lives
 * here, keyed by the same ids. An action in one and not the other is a bug, and
 * `missingHandlers` is how it gets noticed rather than silently doing nothing.
 */
const HANDLERS: Record<string, () => void> = {
  'palette.open': () => useBeacon.getState().setOverlay('palette'),
  'quickOpen.open': () => useBeacon.getState().setOverlay('quickOpen'),
  'settings.open': () => useBeacon.getState().setOverlay('settings'),

  'panel.toggle.files': () => void useBeacon.getState().togglePanel('files'),
  'panel.toggle.git': () => void useBeacon.getState().togglePanel('git'),
  'panel.toggle.editor': () => void useBeacon.getState().togglePanel('editor'),
  'panel.toggle.terminal': () => void useBeacon.getState().togglePanel('terminal'),

  'panel.fullscreen': () => {
    const store = useBeacon.getState()
    store.toggleFullscreen(store.fullscreenPanel ?? 'claude')
  },

  'panel.focusNext': () => shiftFocus(1),
  'panel.focusPrevious': () => shiftFocus(-1),

  // Bound at the application level, not only inside CodeMirror: the file with
  // unsaved changes is the one you were just editing, and Cmd+S doing nothing
  // because focus had moved to a tab or the tree is how work gets lost.
  'editor.save': () => {
    const store = useBeacon.getState()
    const workspace = store.snapshot?.activeWorkspace
    const project = activeProjectId(store)
    if (!workspace || !project) return

    const editor = useEditor.getState()
    const open = editor.byProject[project] ?? []
    const active = open.find((file) => file.path === editor.active[project]) ?? open.at(-1)
    if (active) void editor.save(workspace, project, active.path)
  },

  'session.restartClaude': () => {
    const store = useBeacon.getState()
    const project = activeProjectId(store)
    if (project) void store.restartSession(project, 'claude')
  },

  'project.next': () => shiftProject(1),
  'project.previous': () => shiftProject(-1),
}

/** Human-facing names, kept here because they are interface text. */
export const ACTION_TITLES: Record<string, string> = {
  'palette.open': 'Command palette',
  'quickOpen.open': 'Quick open',
  'settings.open': 'Settings',
  'panel.toggle.files': 'Toggle Files',
  'panel.toggle.git': 'Toggle Git',
  'panel.toggle.editor': 'Toggle the editor',
  'panel.toggle.terminal': 'Toggle the terminal',
  'panel.fullscreen': 'Fullscreen the focused panel',
  'panel.focusNext': 'Focus the next panel',
  'panel.focusPrevious': 'Focus the previous panel',
  'editor.save': 'Save the file',
  'session.restartClaude': 'Restart Claude',
  'project.next': 'Next project',
  'project.previous': 'Previous project',
}

/**
 * Moves the keyboard to the next panel along.
 *
 * The order is the layout's own, so it follows what you can see: a fullscreen
 * panel is the only one there is, and a hidden panel is not somewhere focus can
 * land.
 */
function shiftFocus(step: number): void {
  const store = useBeacon.getState()
  const layout = selectLayout(store)

  const order: PanelId[] = store.fullscreenPanel
    ? [store.fullscreenPanel]
    : layout
      ? panelsOf(prune(layout, selectHidden(store)) ?? layout)
      : []

  const next = panelAfter(order, usePanelFocus.getState().focused, step)
  if (next) focusPanel(next)
}

/** Actions the backend offers that nothing here implements. */
export function missingHandlers(bindings: ActionBinding[]): string[] {
  return bindings.map((b) => b.action).filter((action) => !(action in HANDLERS))
}

/**
 * The binding an event describes, in the same form the backend stores.
 *
 * Returns `null` for anything without the primary modifier, since every Beacon
 * shortcut has one — that is what stops them firing while you type.
 */
export function bindingOf(event: KeyboardEvent): string | null {
  if (!hasPrimaryModifier(event)) return null

  const key = event.key.toLowerCase()
  // A modifier on its own is not a shortcut.
  if (['shift', 'alt', 'control', 'meta', 'os'].includes(key)) return null

  const parts = ['mod']
  if (event.shiftKey) parts.push('shift')
  if (event.altKey) parts.push('alt')
  parts.push(key === ' ' ? 'space' : key)
  return parts.join('+')
}

/** Runs whatever is bound to this event, and says whether anything was. */
export function runBinding(event: KeyboardEvent, bindings: ActionBinding[]): boolean {
  const pressed = bindingOf(event)
  if (!pressed) return false

  const match = bindings.find((binding) => binding.binding === pressed)
  if (!match) return false

  const handler = HANDLERS[match.action]
  if (!handler) return false

  handler()
  return true
}

/** `mod+shift+p` as `⌘⇧P`, or `Ctrl+Shift+P`. */
export function describeBinding(binding: string): string {
  const parts = binding.split('+')
  const key = parts.at(-1) ?? ''
  const shift = parts.includes('shift')
  const alt = parts.includes('alt')

  const label = KEY_LABELS[key] ?? key.toUpperCase()

  return isMac()
    ? `${alt ? '⌥' : ''}${shift ? '⇧' : ''}⌘${label}`
    : `Ctrl+${shift ? 'Shift+' : ''}${alt ? 'Alt+' : ''}${label}`
}

const KEY_LABELS: Record<string, string> = {
  enter: '↩',
  space: '␣',
  escape: '⎋',
  arrowup: '↑',
  arrowdown: '↓',
  arrowleft: '←',
  arrowright: '→',
  ',': ',',
  '[': '[',
  ']': ']',
}

function activeProjectId(store: ReturnType<typeof useBeacon.getState>): string | undefined {
  const snapshot = store.snapshot
  const workspace = snapshot?.workspaces.find((w) => w.id === snapshot.activeWorkspace)
  if (!workspace) return undefined
  return snapshot?.activeProject[workspace.id] ?? workspace.projects[0]?.id
}

/** Moves to the next or previous tab, wrapping around. */
function shiftProject(step: number): void {
  const store = useBeacon.getState()
  const snapshot = store.snapshot
  const workspace = snapshot?.workspaces.find((w) => w.id === snapshot.activeWorkspace)
  const projects = workspace?.projects ?? []
  if (projects.length === 0) return

  const current = projects.findIndex((project) => project.id === activeProjectId(store))
  const next = (current + step + projects.length) % projects.length
  void store.selectProjectAt(next)
}

export { HANDLERS as BOUND_ACTIONS }
