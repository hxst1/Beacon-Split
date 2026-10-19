import type { Terminal } from '@xterm/xterm'

/**
 * Accents typed through WebKitGTK's input method, delivered once.
 *
 * A dead key on Linux (`´` then `a`) reaches the page as a composition that
 * starts, shows `´`, and ends empty — and then a second keydown with keyCode
 * 229, an `input` carrying `á`, and a `compositionend` that was never started.
 * xterm is built for compositions it was told about from the beginning. With
 * the start missing, three of its paths read the same hidden textarea at
 * positions left over from earlier keys: the 229 keydown diffs the textarea,
 * the orphaned `compositionend` slices it, and the previous composition's
 * delayed send reads it too. What arrives in the shell depends on which timer
 * runs first, and is usually the character plus a few older ones
 * (`pingüino` came out as `pingó ñ üino`).
 *
 * So on Linux the keys an input method answers outside a composition are
 * Beacon's: a 229 keydown is kept from xterm, the text comes from the `input`
 * event that carries it, and an orphaned `compositionend` never reaches xterm.
 * A composition that announces itself — Japanese, Chinese, Korean — still goes
 * to xterm untouched, and so does every key the input method does not touch.
 */
export interface AccentGuard {
  /** From xterm's custom key handler; `false` keeps xterm away from the key. */
  keydown(event: Pick<KeyboardEvent, 'keyCode'>): boolean
  compositionstart(): void
  /** Whether xterm should see this `compositionend`. */
  compositionend(event: Pick<CompositionEvent, 'data'>): boolean
  /** Whether this `input` was the guard's to deliver, and so not xterm's. */
  input(event: Pick<InputEvent, 'inputType' | 'data'>): boolean
}

/** The keyCode a browser reports for a key the input method is handling. */
const IME_KEY = 229

/**
 * The guard's state machine, apart from the DOM so it can be tested.
 *
 * `send` delivers text to the session; `clear` empties xterm's textarea, so
 * nothing left in it can be read back later.
 */
export function accentGuard(send: (data: string) => void, clear: () => void): AccentGuard {
  /** Inside a composition that began with `compositionstart`. */
  let composing = false
  /** The last keydown was one the input method took and xterm did not see. */
  let imeKey = false
  /** That key's text has already gone to the session. */
  let delivered = false

  return {
    keydown(event) {
      // Mid-composition xterm already leaves a 229 alone; outside one it is
      // the diffing path that sends stale text.
      if (event.keyCode === IME_KEY && !composing) {
        imeKey = true
        delivered = false
        return false
      }
      imeKey = false
      return true
    },

    compositionstart() {
      composing = true
      imeKey = false
    },

    compositionend(event) {
      if (composing) {
        composing = false
        return true
      }
      if (!delivered && event.data) send(event.data)
      delivered = false
      imeKey = false
      clear()
      return false
    },

    input(event) {
      if (composing || !imeKey) return false
      if (event.data && (event.inputType === 'insertText' || event.inputType === 'insertCompositionText')) {
        send(event.data)
        delivered = true
        // Before any delayed read of xterm's: what it would find is what was
        // just sent.
        clear()
        return true
      }
      if (event.inputType === 'deleteContentBackward') {
        // What xterm's own diff would have sent for a shrinking textarea.
        send('\x7f')
        return true
      }
      return false
    },
  }
}

/**
 * Puts the guard in front of xterm's own listeners for one terminal.
 *
 * Capture listeners on the terminal's element run before the ones xterm put on
 * its textarea, which is what lets a stopped `compositionend` never reach it.
 */
export function installAccentGuard(term: Terminal, element: HTMLElement): AccentGuard {
  const guard = accentGuard(
    (data) => term.input(data),
    () => {
      if (term.textarea) term.textarea.value = ''
    },
  )

  const fromTextarea = (event: Event): boolean => event.target === term.textarea

  element.addEventListener(
    'compositionstart',
    (event) => {
      if (fromTextarea(event)) guard.compositionstart()
    },
    true,
  )
  element.addEventListener(
    'compositionend',
    (event) => {
      if (fromTextarea(event) && !guard.compositionend(event)) event.stopPropagation()
    },
    true,
  )
  element.addEventListener(
    'input',
    (event) => {
      if (fromTextarea(event) && guard.input(event as InputEvent)) event.stopPropagation()
    },
    true,
  )

  return guard
}
