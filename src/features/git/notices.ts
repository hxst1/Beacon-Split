/**
 * What the Git panel has to say, kept in two slots that cannot overwrite each
 * other.
 *
 * The panel re-reads the status every two seconds. With a single message,
 * every one of those reads deleted the reason a push or a commit had just
 * failed — the message was gone before it could be finished. They are separate
 * kinds of news: one is about the repository not being readable, the other is
 * the answer to something the user asked for, and each has to survive the
 * other happening.
 */
export interface Notices {
  /** The outcome of the last thing the user asked for. */
  action: { tone: 'error' | 'report'; text: string } | null
  /** Why the background status read is not working, while it is not. */
  poll: string | null
}

export type NoticeEvent =
  /** The user asked for something, which supersedes the last answer. */
  | { type: 'actionStarted' }
  /** It worked; `text` is whatever git had to say about it, if anything. */
  | { type: 'actionSucceeded'; text?: string }
  | { type: 'actionFailed'; text: string }
  | { type: 'pollSucceeded' }
  | { type: 'pollFailed'; text: string }

export const noNotices: Notices = { action: null, poll: null }

export function reduceNotices(current: Notices, event: NoticeEvent): Notices {
  switch (event.type) {
    case 'actionStarted':
      return current.action === null ? current : { ...current, action: null }

    case 'actionSucceeded': {
      const said = event.text?.trim()
      return { ...current, action: said ? { tone: 'report', text: said } : null }
    }

    case 'actionFailed':
      return { ...current, action: { tone: 'error', text: event.text } }

    // A status read that works again says nothing about whether the push the
    // user is still reading about worked.
    case 'pollSucceeded':
      return current.poll === null ? current : { ...current, poll: null }

    // A read that keeps failing the same way every two seconds is one piece of
    // news, not one every two seconds.
    case 'pollFailed':
      return current.poll === event.text ? current : { ...current, poll: event.text }
  }
}
