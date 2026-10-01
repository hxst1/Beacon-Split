import { isMac, isWindows } from '@/lib/platform'

/** What a key press in a terminal means for the clipboard. */
export type ClipboardKey = 'copy' | 'paste' | 'terminal'

type KeyPress = Pick<KeyboardEvent, 'type' | 'key' | 'ctrlKey' | 'shiftKey' | 'altKey' | 'metaKey'>

/**
 * Copy and paste where the primary modifier is Ctrl.
 *
 * On macOS ⌘C and ⌘V never reach the shell, so the webview's own copy and
 * paste just work and everything here is the terminal's. Elsewhere Ctrl+C is
 * the interrupt and Ctrl+V a character, so the keys are shared: Ctrl+Shift+C
 * and Ctrl+Shift+V always copy and paste, and on Windows — as in Windows
 * Terminal and VS Code — Ctrl+C copies when something is selected and
 * interrupts when nothing is, and Ctrl+V pastes.
 */
export function clipboardKey(event: KeyPress, hasSelection: boolean): ClipboardKey {
  if (isMac() || event.type !== 'keydown' || !event.ctrlKey || event.altKey || event.metaKey) {
    return 'terminal'
  }

  const key = event.key.toLowerCase()
  if (key === 'c' && (event.shiftKey || (isWindows() && hasSelection))) return 'copy'
  if (key === 'v' && (event.shiftKey || isWindows())) return 'paste'
  return 'terminal'
}
