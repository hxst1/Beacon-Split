import { create } from 'zustand'

import { watchActivity } from '@/features/terminal/sessionBridge'
import { ipc } from '@/ipc'
import type { Clip, ClipKind } from '@/types/beacon'

/** How long the tick stays on a clip after it has been copied. */
export const COPIED_FOR_MS = 1200

interface ClipState {
  /** Newest first, exactly as the daemon holds them. */
  clips: Clip[]
  open: boolean
  /**
   * Clips filed since the drawer was last looked at.
   *
   * The tab is the only thing on screen when the drawer is shut, so it is the
   * only place that can say something arrived. A count rather than a dot: "3
   * waiting" is worth interrupting a train of thought for, "something happened"
   * is not.
   */
  unseen: number
  /** The clip whose copy just succeeded, for the tick beside it. */
  copied: string | null
  /** Set when the clipboard refused, which is the one failure worth naming. */
  failed: string | null
}

export const useClips = create<ClipState>(() => ({
  clips: [],
  open: false,
  unseen: 0,
  copied: null,
  failed: null,
}))

/**
 * Files a clip that has just arrived.
 *
 * Guards against filing the same one twice: the window asks for the whole
 * drawer whenever it reattaches, and a clip that arrived during the reconnect
 * is in both that answer and the event that announced it.
 */
export function accept(clip: Clip): void {
  useClips.setState((state) => {
    if (state.clips.some((existing) => existing.id === clip.id)) return state
    return {
      clips: [clip, ...state.clips],
      unseen: state.open ? 0 : state.unseen + 1,
    }
  })
}

/** Replaces the drawer with what the daemon says is in it. */
export function replace(clips: Clip[]): void {
  useClips.setState((state) => ({
    clips,
    // Never more than there are: forgetting clips must not leave the tab
    // claiming things are waiting that have just been thrown away.
    unseen: Math.min(state.unseen, clips.length),
  }))
}

export function openDrawer(): void {
  useClips.setState({ open: true, unseen: 0 })
}

export function closeDrawer(): void {
  useClips.setState({ open: false })
}

export function toggleDrawer(): void {
  useClips.getState().open ? closeDrawer() : openDrawer()
}

/** Whatever the daemon already had, so an opening window is not blank. */
export function loadClips(): void {
  ipc
    .sessionClips()
    .then(replace)
    .catch(() => {
      // An unreachable daemon is already reported by the status bar; a second
      // complaint about the drawer adds nothing.
    })
}

export async function forgetClip(id: string): Promise<void> {
  try {
    replace(await ipc.forgetClips(id))
  } catch {
    // The daemon is the truth. If it did not hear, nothing was forgotten, and
    // the drawer is still showing what is actually there.
  }
}

export async function forgetEveryClip(): Promise<void> {
  try {
    replace(await ipc.forgetClips())
  } catch {
    // As above.
  }
}

/**
 * Puts a clip on the clipboard.
 *
 * Tries the async clipboard first and falls back to a hidden textarea, because
 * the async API needs the document to be focused and a click on a drawer that
 * has just opened is not always enough on macOS.
 */
export async function copyClip(clip: Clip): Promise<void> {
  const done = await writeToClipboard(clip.body)
  useClips.setState(done ? { copied: clip.id, failed: null } : { failed: clip.id, copied: null })

  window.setTimeout(() => {
    useClips.setState((state) =>
      state.copied === clip.id || state.failed === clip.id
        ? { copied: null, failed: null }
        : state,
    )
  }, COPIED_FOR_MS)
}

async function writeToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    return copyBySelection(text)
  }
}

/** The old way, which works without clipboard permission. */
function copyBySelection(text: string): boolean {
  const holder = document.createElement('textarea')
  holder.value = text
  // Off-screen rather than hidden: a `display: none` element cannot be selected.
  holder.setAttribute('readonly', '')
  holder.style.position = 'fixed'
  holder.style.top = '-1000px'
  holder.style.opacity = '0'
  document.body.appendChild(holder)

  try {
    holder.select()
    return document.execCommand('copy')
  } catch {
    return false
  } finally {
    document.body.removeChild(holder)
  }
}

/**
 * Starts listening. Called once by the application rather than on import, for
 * the same reason usage tracking is: a module that subscribes as a side effect
 * of being imported cannot be tested without a Tauri runtime.
 */
export function startClipTracking(): () => void {
  loadClips()
  return watchActivity({
    onOutput: () => {},
    onExit: () => {},
    onClip: accept,
    onClips: replace,
    // The daemon may have been replaced, and the drawer belongs to it.
    onReattached: loadClips,
  })
}

// ---- pure helpers, so the drawer renders no logic of its own --------------

/** What a kind is called in the corner of a clip. */
export function labelOf(kind: ClipKind): string {
  switch (kind) {
    case 'command':
      return 'command'
    case 'variable':
      return 'variable'
    case 'email':
      return 'email'
    default:
      return 'text'
  }
}

/** Whether the body must be shown monospaced and unwrapped. */
export function isLiteral(kind: ClipKind): boolean {
  return kind === 'command' || kind === 'variable'
}

/**
 * `4m`, `2h`, `3d` — how long a clip has been waiting.
 *
 * Short because it sits in a corner beside the kind, and the exact minute of a
 * thing you are about to paste has never mattered to anybody.
 */
export function age(createdAt: number, now: number): string {
  const seconds = Math.max(0, Math.floor(now / 1000) - createdAt)
  if (seconds < 60) return 'now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h`
  return `${Math.floor(hours / 24)}d`
}

/** How many lines of a clip the drawer shows before it is asked to expand. */
export const PREVIEW_LINES = 6

/**
 * The body trimmed to a preview, and whether anything was left out.
 *
 * Cut by lines rather than by characters: half of the last line of an `.env`
 * block reads as a value, and a truncated value that looks whole is exactly the
 * thing somebody pastes without checking.
 */
export function preview(body: string): { text: string; truncated: boolean } {
  const lines = body.split('\n')
  if (lines.length <= PREVIEW_LINES) return { text: body, truncated: false }
  return { text: lines.slice(0, PREVIEW_LINES).join('\n'), truncated: true }
}
