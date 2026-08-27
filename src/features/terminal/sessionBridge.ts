import { listen } from '@tauri-apps/api/event'

import type {
  Clip,
  SessionActivity,
  SessionExit,
  SessionOutput,
  UsageReport,
} from '@/types/beacon'

/** Something that can receive PTY bytes — in practice, an xterm instance. */
export interface OutputSink {
  write: (bytes: Uint8Array) => void
  /** Called when the session's process ends. */
  onExit?: (code: number | null) => void
}

interface Attachment {
  sink: OutputSink
  /**
   * Stream offset the sink has consumed up to. Chunks ending at or before this
   * were already covered by the replayed snapshot and must not be written twice.
   */
  consumed: number
  /** Chunks that arrived before the snapshot was replayed. */
  queued: SessionOutput[]
  replaying: boolean
}

const attachments = new Map<string, Attachment>()
let listening: Promise<void> | null = null

/**
 * Anything that wants to know a session did something, whether or not it is
 * showing that session.
 *
 * Activity indicators need every session's events, including projects whose
 * panels are not mounted, so they listen here rather than through a terminal.
 */
export interface ActivityWatcher {
  onOutput: (project: string) => void
  onExit: (project: string, sessionId: string) => void
  /** The daemon connection dropped. Sessions are still running. */
  onDetached?: () => void
  /** A connection is live again, possibly to a different daemon. */
  onReattached?: () => void
  /** Claude Code said what a project's session is doing. */
  onClaudeActivity?: (report: SessionActivity) => void
  /** Claude Code said what a project's session is costing. */
  onUsage?: (report: UsageReport) => void
  /** Claude filed something for the user to copy. */
  onClip?: (clip: Clip) => void
  /** The drawer changed wholesale — something was forgotten, or all of it. */
  onClips?: (clips: Clip[]) => void
}

const watchers = new Set<ActivityWatcher>()

export function watchActivity(watcher: ActivityWatcher): () => void {
  watchers.add(watcher)
  void ensureListening()
  return () => {
    watchers.delete(watcher)
  }
}

function decode(base64: string): Uint8Array {
  const binary = atob(base64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i)
  return bytes
}

/**
 * Writes a chunk, trimming any part the sink has already seen.
 *
 * A chunk can straddle the snapshot boundary — part already replayed, part new —
 * so it is sliced rather than accepted or dropped whole.
 */
function deliver(attachment: Attachment, chunk: SessionOutput): void {
  const bytes = decode(chunk.data)
  const end = chunk.offset + bytes.length
  if (end <= attachment.consumed) return

  const skip = Math.max(0, attachment.consumed - chunk.offset)
  attachment.sink.write(skip > 0 ? bytes.subarray(skip) : bytes)
  attachment.consumed = end
}

/**
 * Subscribes to session events once, for the lifetime of the window.
 *
 * A single listener fans out to every attached terminal, so opening a project
 * does not add another IPC subscription.
 */
function ensureListening(): Promise<void> {
  listening ??= Promise.all([
    listen<SessionOutput>('session:output', ({ payload }) => {
      for (const watcher of watchers) watcher.onOutput(payload.project)

      const attachment = attachments.get(payload.id)
      if (!attachment) return
      if (attachment.replaying) {
        attachment.queued.push(payload)
        return
      }
      deliver(attachment, payload)
    }),
    listen<SessionExit>('session:exit', ({ payload }) => {
      for (const watcher of watchers) watcher.onExit(payload.project, payload.id)
      attachments.get(payload.id)?.sink.onExit?.(payload.code)
    }),
    listen('session:detached', () => {
      for (const watcher of watchers) watcher.onDetached?.()
    }),
    listen('session:reattached', () => {
      for (const watcher of watchers) watcher.onReattached?.()
    }),
    listen<SessionActivity>('session:activity', ({ payload }) => {
      for (const watcher of watchers) watcher.onClaudeActivity?.(payload)
    }),
    listen<UsageReport>('session:usage', ({ payload }) => {
      for (const watcher of watchers) watcher.onUsage?.(payload)
    }),
    listen<Clip>('clips:added', ({ payload }) => {
      for (const watcher of watchers) watcher.onClip?.(payload)
    }),
    listen<{ clips: Clip[] }>('clips:replaced', ({ payload }) => {
      for (const watcher of watchers) watcher.onClips?.(payload.clips)
    }),
  ]).then(() => undefined)

  return listening
}

/**
 * Routes a session's output into a sink.
 *
 * Live chunks are queued until {@link replayed} reports where the snapshot
 * ended, which is what makes reattaching lossless in both directions.
 */
export async function attach(id: string, sink: OutputSink): Promise<void> {
  attachments.set(id, { sink, consumed: 0, queued: [], replaying: true })
  await ensureListening()
}

/** Called once the snapshot has been written, with the offset just past it. */
export function replayed(id: string, endOffset: number): void {
  const attachment = attachments.get(id)
  if (!attachment) return

  attachment.consumed = endOffset
  attachment.replaying = false
  for (const chunk of attachment.queued) deliver(attachment, chunk)
  attachment.queued = []
}

export function detach(id: string): void {
  attachments.delete(id)
}
