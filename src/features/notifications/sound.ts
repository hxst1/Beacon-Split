/**
 * The two sounds Beacon makes: Claude is waiting for you, and a long turn has
 * finished.
 *
 * Synthesised rather than shipped as files — a few notes through one oscillator
 * each — so there is nothing to bundle and they sound the same on every
 * platform. Quiet on purpose: they are for someone who is not looking, and the
 * point is to be heard from across the room, not to make anyone jump.
 */

export type Chime = 'waiting' | 'done'

/** One note: when it starts after the chime does, its pitch, how long it rings. */
export interface Note {
  at: number
  frequency: number
  length: number
}

/**
 * What each chime plays.
 *
 * Waiting rises and asks — two notes, the second higher — because it wants an
 * answer. Done falls and settles, because it does not. Different enough in
 * shape to be told apart without looking.
 */
export const CHIMES: Record<Chime, Note[]> = {
  waiting: [
    { at: 0, frequency: 659.25, length: 0.16 },
    { at: 0.14, frequency: 880, length: 0.24 },
  ],
  done: [
    { at: 0, frequency: 783.99, length: 0.14 },
    { at: 0.12, frequency: 523.25, length: 0.3 },
  ],
}

/** Loud enough to hear in a quiet room, nowhere near a notification sound's. */
const PEAK = 0.08

/**
 * One context for the life of the window.
 *
 * Not one per sound: browsers cap how many can be open, and each one holds an
 * audio device until it is closed.
 */
let context: AudioContext | null = null

function audio(): AudioContext | null {
  if (context) return context
  try {
    context = new AudioContext()
  } catch {
    // No audio device, or none the webview will open. Sounds are a nicety.
    return null
  }
  return context
}

/**
 * Resumes a context, and never minds if it will not.
 *
 * Both call sites go through here because the failure is silent on the two
 * platforms where it does not happen: `resume` rejects on a webview that
 * cannot open an audio device, which on Linux is any WebKitGTK whose
 * GStreamer plugins are not installed — they are an optional dependency of
 * the package. A sound that cannot play is a sound not heard, which is what
 * the rest of this module already says; an unhandled rejection in the log is
 * something else.
 */
function resume(ctx: AudioContext | null): void {
  void ctx?.resume().catch(() => {})
}

/**
 * Lets the context make sound.
 *
 * WebView2 and WebKit both start a context suspended until the page has seen a
 * gesture, so a chime asked for by an activity report — which is never a
 * gesture — would play nothing. The first key or click anywhere in the window
 * resumes it, and from then on it plays whenever it is asked. Called once at
 * start; the listeners remove themselves.
 */
export function unlockSoundOnFirstGesture(): () => void {
  const unlock = (): void => {
    resume(audio())
    stop()
  }
  const stop = (): void => {
    window.removeEventListener('pointerdown', unlock, true)
    window.removeEventListener('keydown', unlock, true)
  }
  window.addEventListener('pointerdown', unlock, true)
  window.addEventListener('keydown', unlock, true)
  return stop
}

/** Plays a chime now. Never throws: a sound that fails is a sound not heard. */
export function playChime(chime: Chime): void {
  const ctx = audio()
  if (!ctx) return
  if (ctx.state === 'suspended') resume(ctx)

  try {
    const start = ctx.currentTime + 0.01
    for (const note of CHIMES[chime]) {
      const oscillator = ctx.createOscillator()
      const gain = ctx.createGain()
      oscillator.type = 'sine'
      oscillator.frequency.setValueAtTime(note.frequency, start + note.at)

      // A soft attack and an exponential fade, so the notes ring rather
      // than click on and off.
      gain.gain.setValueAtTime(0.0001, start + note.at)
      gain.gain.exponentialRampToValueAtTime(PEAK, start + note.at + 0.015)
      gain.gain.exponentialRampToValueAtTime(0.0001, start + note.at + note.length)

      oscillator.connect(gain)
      gain.connect(ctx.destination)
      oscillator.start(start + note.at)
      oscillator.stop(start + note.at + note.length + 0.02)
      oscillator.onended = () => {
        oscillator.disconnect()
        gain.disconnect()
      }
    }
  } catch {
    // Nothing worth reporting: the event still means what it means.
  }
}
