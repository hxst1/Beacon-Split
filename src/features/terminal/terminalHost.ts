import { FitAddon } from '@xterm/addon-fit'
import { Terminal, type ITheme } from '@xterm/xterm'

import { cssValue } from '@/lib/appearance'
import { isLinux, windowsPty } from '@/lib/platform'
import { ipc } from '@/ipc'
import type { SessionKind, TerminalFont } from '@/types/beacon'
import { installAccentGuard, type AccentGuard } from './accentGuard'
import { clipboardKey } from './clipboardKeys'
import { newlineKey } from './newlineKeys'
import { attach, replayed } from './sessionBridge'
import { fontStack } from './terminalFont'
import { MINIMUM_CONTRAST, terminalTheme } from './terminalTheme'

export interface HostedTerminal {
  term: Terminal
  fit: FitAddon
  /** The element the terminal renders into, reparented as panels come and go. */
  element: HTMLDivElement
  /** Which project this belongs to, so it can be torn down with it. */
  project: string
  kind: SessionKind
  slot: number
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

/** The font new terminals are built with; follows the setting. */
let font: TerminalFont = { size: 12 }

/** Called when the font changes, so each view can refit its grid. */
const fontListeners = new Set<() => void>()

/** The terminal's theme for whichever palette and accent are showing now. */
function currentTheme(): ITheme {
  const light = document.documentElement.dataset['theme'] === 'light'
  return terminalTheme({
    light,
    window: cssValue('--window', light ? '246 246 248' : '8 8 11'),
    accent: cssValue('--accent', '#6b7cff'),
  })
}

function create(
  project: string,
  kind: SessionKind,
  slot: number,
  grid: { cols: number; rows: number },
): HostedTerminal {
  const element = document.createElement('div')
  element.style.width = '100%'
  element.style.height = '100%'
  const pty = windowsPty()

  const term = new Terminal({
    // The grid the session's output was written for, rather than xterm's own
    // default of 80x24. Replaying a snapshot into the wrong width re-wraps
    // every line that was already wrapped, and what comes out is the shredded
    // text you only get rid of by restarting the session. The real size
    // follows a moment later, and reflowing from a correct state is something
    // xterm does properly.
    cols: grid.cols,
    rows: grid.rows,
    // Transparent, so the panel's blurred surface shows through instead of a
    // flat black rectangle.
    allowTransparency: true,
    theme: currentTheme(),
    minimumContrastRatio: MINIMUM_CONTRAST,
    fontFamily: fontStack(font),
    fontSize: font.size,
    lineHeight: 1.35,
    letterSpacing: 0,
    cursorBlink: true,
    cursorStyle: 'bar',
    scrollback: 10_000,
    // xterm's own scrollbar would fight the app's; the panel handles overflow.
    scrollOnUserInput: true,
    // Option is how most non-US Mac layouts type @, #, [, ], {, }, \, | and ~,
    // so it has to stay a character key. Option with an arrow, Backspace or
    // Enter still reaches the session with its modifier; xterm only consults
    // this for keys that produce a character.
    macOptionIsMeta: false,
    // Only set on Windows, where the session runs in a pseudo-console that
    // xterm has to know about to resize it without losing lines.
    ...(pty ? { windowsPty: pty } : {}),
  })
  const fit = new FitAddon()
  term.loadAddon(fit)
  term.open(element)

  // Linux only: the shape it fixes is WebKitGTK's, and macOS's input method
  // already announces every composition xterm is handed.
  const accents = isLinux() ? installAccentGuard(term, element) : null
  term.attachCustomKeyEventHandler((event) => beaconKeys(term, event, kind, accents))

  return { term, fit, element, project, kind, slot }
}

/**
 * The keys Beacon reads before the session does.
 *
 * Returning `false` keeps xterm from sending the key on as it would have.
 */
function beaconKeys(
  term: Terminal,
  event: KeyboardEvent,
  kind: SessionKind,
  accents: AccentGuard | null,
): boolean {
  if (event.type === 'keydown' && accents && !accents.keydown(event)) return false
  const newline = newlineKey(event, kind)
  if (newline !== null) {
    // xterm would send a carriage return here and the agent would take the
    // prompt as finished, so the newline goes in its place. It is still
    // typing, so it scrolls and clears the selection as a key press should.
    term.input(newline)
    event.preventDefault()
    return false
  }
  return clipboardKeys(term, event)
}

/**
 * Copy and paste where the primary modifier is Ctrl; see {@link clipboardKey}.
 *
 * Returning `false` keeps xterm from sending the key; for a paste that leaves
 * the browser's own paste to happen, which xterm then delivers as a paste.
 */
function clipboardKeys(term: Terminal, event: KeyboardEvent): boolean {
  const meaning = clipboardKey(event, term.hasSelection())

  if (meaning === 'copy') {
    if (term.hasSelection()) {
      void navigator.clipboard.writeText(term.getSelection())
      term.clearSelection()
    }
    event.preventDefault()
    return false
  }
  return meaning !== 'paste'
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
  slot: number,
  grid: { cols: number; rows: number },
): Promise<HostedTerminal> {
  const existing = hosted.get(sessionId)
  if (existing) return existing

  const terminal = create(project, kind, slot, grid)
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
export function disposeFor(project: string, kind: SessionKind, slot?: number): void {
  for (const [sessionId, terminal] of hosted) {
    if (terminal.project !== project || terminal.kind !== kind) continue
    if (slot !== undefined && terminal.slot !== slot) continue
    dispose(sessionId)
  }
}

/** Tears down every terminal belonging to a project. */
export function disposeProject(project: string): void {
  for (const [sessionId, terminal] of hosted) {
    if (terminal.project === project) dispose(sessionId)
  }
}

/**
 * Tears down every terminal.
 *
 * Used when the daemon connection is rebuilt: the new daemon may not be the old
 * one, so every session id these were attached to has to be treated as gone.
 */
export function disposeAll(): void {
  for (const sessionId of [...hosted.keys()]) dispose(sessionId)
}

/**
 * Draws every terminal in a new font, and has each view refit its grid.
 *
 * The panel has not changed size, so nothing else would tell it to: a bigger
 * font in the same grid runs off the edge, a smaller one leaves a margin.
 */
export function setTerminalFont(next: TerminalFont): void {
  if (next.family === font.family && next.size === font.size) return
  font = next
  const family = fontStack(font)
  for (const { term } of hosted.values()) {
    term.options.fontFamily = family
    term.options.fontSize = font.size
  }
  for (const listener of fontListeners) listener()
}

/** Calls back after the terminal font changes; returns the unsubscribe. */
export function onFontChange(listener: () => void): () => void {
  fontListeners.add(listener)
  return () => fontListeners.delete(listener)
}

/** Repaints every terminal after the palette or the accent changes. */
export function refreshAccent(): void {
  const theme = currentTheme()
  for (const { term } of hosted.values()) {
    term.options.theme = theme
  }
}
