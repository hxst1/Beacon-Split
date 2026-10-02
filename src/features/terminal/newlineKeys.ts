import type { SessionKind } from '@/types/beacon'

type KeyPress = Pick<KeyboardEvent, 'type' | 'key' | 'ctrlKey' | 'shiftKey' | 'altKey' | 'metaKey'>

/**
 * What Shift+Enter should put into a session, or `null` to leave the key alone.
 *
 * A terminal cannot tell Shift+Enter from Enter — both arrive as a carriage
 * return — so an agent waiting on one sends the half-written prompt. Every
 * chat window in the world breaks the line instead, which is what people's
 * hands expect, so the key is translated here into whatever the agent on the
 * other end reads as a newline.
 *
 * Each sequence is the agent's own, not a guess at one:
 *
 * - Claude Code's `/terminal-setup` makes this key work in iTerm2 and VS Code
 *   by binding it to send `ESC CR`, so that is the sequence it reads as a
 *   newline. Beacon sends it directly and the setup is never needed here.
 * - Codex is a crossterm program, and crossterm reports a bare line feed in
 *   raw mode as Ctrl+J rather than as Enter — the newline its composer takes.
 *
 * A shell is left alone, because there is no half-written anything to protect:
 * Enter runs the command, and Shift+Enter running it too is what every
 * terminal already does.
 */
export function newlineKey(event: KeyPress, kind: SessionKind): string | null {
  if (event.type !== 'keydown' || event.key !== 'Enter') return null
  // Shift and nothing else: Ctrl+Enter and friends belong to whoever bound them.
  if (!event.shiftKey || event.ctrlKey || event.altKey || event.metaKey) return null

  // Exhaustive on purpose: a new kind of agent should not silently inherit one
  // of these, since being wrong here means sending a prompt that was not ready.
  switch (kind) {
    case 'claude':
      return '\x1b\r'
    case 'codex':
      return '\n'
    case 'shell':
      return null
  }
}
