import type { Account, AgentKind } from '@/types/beacon'

/** How an account reads in a panel header, and in the tooltip behind it. */
export interface AccountLabel {
  /** The account itself, as far as the agent names it. */
  name: string
  /** The plan, when the agent said one and it adds something the name does not. */
  detail: string | null
  /** The whole of what is known, for the hover. */
  title: string
}

/** What each agent is called when talking about its account. */
const AGENT_NAMES: Record<AgentKind, string> = {
  claude: 'Claude Code',
  codex: 'Codex',
}

/**
 * What to put in the header for an agent's account, or nothing.
 *
 * Returning null is the common case worth protecting: an agent that was never
 * asked, one too old to answer, or one whose answer named nothing. A chip that
 * can only say "unknown" costs a reader a glance every time they look at the
 * header and gives them nothing back, so there is no chip.
 *
 * The two agents are asymmetric and this does not hide it. Claude Code names
 * the account; Codex names only how it signed in, because that is all Codex
 * will say, and the tooltip says so rather than leaving somebody wondering
 * which of their two ChatGPT accounts is answering.
 */
export function accountLabel(agent: AgentKind, account: Account | undefined): AccountLabel | null {
  if (!account) return null
  const who = AGENT_NAMES[agent]

  // Said in so many words, and the only case worth stating plainly: the panel
  // behind this is showing the agent's own sign-in screen.
  if (account.signedIn === false) {
    return { name: 'not signed in', detail: null, title: `Nobody is signed in to ${who}` }
  }

  const plan = account.plan ? titleCase(account.plan) : null

  if (account.email) {
    return {
      name: account.email,
      detail: plan,
      title: [
        `${who} is signed in as ${account.email}`,
        plan ? ` on the ${plan} plan` : '',
        account.method ? `, through ${account.method}` : '',
      ].join(''),
    }
  }

  // No account name anywhere — Codex, every time. The method is the only true
  // thing there is to show, and the hover explains the gap instead of leaving
  // it as a mystery about Beacon.
  if (account.method) {
    return {
      name: account.method,
      detail: null,
      title: `${who} is signed in using ${account.method}. ${who} does not say which account, so the name is yours to give it.`,
    }
  }

  return null
}

/** `max` the way a person writes it. */
function titleCase(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1)
}
