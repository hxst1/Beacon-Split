import { create } from 'zustand'

import { watchActivity } from './sessionBridge'
import type { SessionActivity } from '@/types/beacon'

/**
 * The last thing Claude said at the end of a turn, per project.
 *
 * Claude Code draws on the terminal's alternate screen, so its replies never
 * reach xterm's scrollback and there is nothing in the buffer to mark. The
 * Stop hook hands the reply over as text instead, and this keeps it until the
 * next turn starts — so a reply that scrolled away under a long run of tool
 * output can still be read without hunting for it.
 */
export type Replies = Record<string, string>

/**
 * What a report does to the replies.
 *
 * A turn that ends with something to say replaces what was there. Anything
 * that means the conversation moved on — a new turn starting, the session
 * being cleared or ending — takes it away, because a reply shown beside the
 * wrong turn is worse than none. Waiting leaves it alone: a permission prompt
 * mid-turn says nothing about the last reply.
 */
export function nextReplies(replies: Replies, report: SessionActivity): Replies {
  const { project, activity, reply } = report
  if (activity === 'done') {
    if (!reply) return without(replies, project)
    return replies[project] === reply ? replies : { ...replies, [project]: reply }
  }
  if (activity === 'waiting') return replies
  return without(replies, project)
}

function without(replies: Replies, project: string): Replies {
  if (!(project in replies)) return replies
  const rest = { ...replies }
  delete rest[project]
  return rest
}

interface RepliesState {
  replies: Replies
  /** Put away by hand; comes back with the next turn's reply. */
  dismiss: (project: string) => void
}

export const useReplies = create<RepliesState>((set) => ({
  replies: {},
  dismiss: (project) => set((state) => ({ replies: without(state.replies, project) })),
}))

/** Starts listening. Called once by the application, not on import. */
export function startReplyTracking(): () => void {
  return watchActivity({
    onClaudeActivity: (report) => {
      useReplies.setState((state) => {
        const replies = nextReplies(state.replies, report)
        return replies === state.replies ? state : { replies }
      })
    },
  })
}
