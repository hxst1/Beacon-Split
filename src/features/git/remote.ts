import type { GitStatus } from '@/types/beacon'

/** Whether push and pull can do anything, and what to say when they cannot. */
export interface RemoteActions {
  canPush: boolean
  canPull: boolean
  /** Why not, in the words the button's tooltip uses. */
  reason: string | null
}

/**
 * Reads what the branch header already told us about where this branch pushes
 * to.
 *
 * Beacon runs plain `git push` and `git pull --ff-only`, both of which need a
 * tracking branch and neither of which will invent one. Offering the buttons
 * anyway meant clicking them to be told `fatal: The current branch has no
 * upstream branch` — an answer the panel had, before the click.
 */
export function remoteActions(status: GitStatus): RemoteActions {
  if (!status.branch) {
    return {
      canPush: false,
      canPull: false,
      reason: 'A detached HEAD has no branch to push or pull.',
    }
  }

  if (!status.upstream) {
    return {
      canPush: false,
      canPull: false,
      reason: `${status.branch} is not tracking a branch yet. Run \`git push -u origin ${status.branch}\` in a terminal once, and these work from then on.`,
    }
  }

  return { canPush: true, canPull: true, reason: null }
}
