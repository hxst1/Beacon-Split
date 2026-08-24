import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'

import { ipc } from '@/ipc'
import type { SessionKind } from '@/types/beacon'
import { attach, replayed } from './sessionBridge'

export interface HostedTerminal {
  term: Terminal
  fit: FitAddon
  /** The element the terminal renders into, reparented as panels come and go. */
  element: HTMLDivElement
  /** Which project this belongs to, so it can be torn down with it. */
  project: string
  kind: SessionKind
}

/**
 * Live terminals, keyed by session id.
 *
 * Instances outlive the React components that show them. Switching projects
 * reparents an existing element instead of rebuilding a terminal, which is what
 * makes the switch feel instant — and means a session keeps rendering output
 * while you are looking at something else.
 */
const hosted = new Map<string, HostedTerminal>()

function readAccent(): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue('--accent')
  return value.trim() || '#6b7cff'
}

function create(project: string, kind: SessionKind): HostedTerminal {
  const element = document.createElement('div')
  element.style.width = '100%'
  element.style.height = '100%'

  const term = new Terminal({
    // Transparent, so the panel's blurred surface shows through instead of a
    // flat black rectangle.
    allowTransparency: true,
    theme: {
      background: 'rgba(0, 0, 0, 0)',
      foreground: 'rgba(255, 255, 255, 0.88)',
      cursor: readAccent(),
      cursorAccent: '#08080b',
      selectionBackground: 'rgba(255, 255, 255, 0.16)',
    },
    fontFamily: "'SF Mono', 'JetBrains Mono', Menlo, 'DejaVu Sans Mono', monospace",
    fontSize: 12,
    lineHeight: 1.35,
    letterSpacing: 0,
    cursorBlink: true,
    cursorStyle: 'bar',
    scrollback: 10_000,
    // xterm's own scrollbar would fight the app's; the panel handles overflow.
    scrollOnUserInput: true,
    macOptionIsMeta: true,
  })

  const fit = new FitAddon()
  term.loadAddon(fit)
  term.open(element)

  return { term, fit, element, project, kind }
}

/**
 * Returns the terminal for a session, building and back-filling it on first use.
 *
 * The snapshot is replayed before live output is released, so a terminal opened
 * for an existing session shows exactly what that session has printed.
 */
export async function acquire(
  sessionId: string,
  project: string,
  kind: SessionKind,
): Promise<HostedTerminal> {
  const existing = hosted.get(sessionId)
  if (existing) return existing

  const terminal = create(project, kind)
  hosted.set(sessionId, terminal)

  await attach(sessionId, {
    write: (bytes) => terminal.term.write(bytes),
    onExit: () => {
      terminal.term.write('\r\n\x1b[2m[process exited]\x1b[0m\r\n')
    },
  })

  const snapshot = await ipc.sessionScrollback(sessionId)
  if (snapshot.data) {
    const binary = atob(snapshot.data)
    const bytes = new Uint8Array(binary.length)
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i)
    terminal.term.write(bytes)
  }
  replayed(sessionId, snapshot.endOffset)

  terminal.term.onData((data) => {
    void ipc.writeSession(sessionId, data)
  })

  return terminal
}

/** Tears a terminal down for good — used when its session is closed. */
export function dispose(sessionId: string): void {
  const terminal = hosted.get(sessionId)
  if (!terminal) return
  terminal.term.dispose()
  terminal.element.remove()
  hosted.delete(sessionId)
}

/** Tears down one project's terminal of a given kind, e.g. before a restart. */
export function disposeFor(project: string, kind: SessionKind): void {
  for (const [sessionId, terminal] of hosted) {
    if (terminal.project === project && terminal.kind === kind) dispose(sessionId)
  }
}

/** Tears down every terminal belonging to a project. */
export function disposeProject(project: string): void {
  for (const [sessionId, terminal] of hosted) {
    if (terminal.project === project) dispose(sessionId)
  }
}

/** Repaints cursors after a workspace accent change. */
export function refreshAccent(): void {
  const cursor = readAccent()
  for (const { term } of hosted.values()) {
    term.options.theme = { ...term.options.theme, cursor }
  }
}
