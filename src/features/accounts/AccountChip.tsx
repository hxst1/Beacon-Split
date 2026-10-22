import { useBeacon } from '@/app/store'
import type { AgentKind } from '@/types/beacon'
import { accountLabel } from './account'
import styles from './AccountChip.module.css'

/**
 * Which account this agent is using, in its panel header.
 *
 * It is here because the mistake it prevents is expensive and silent: asking
 * the wrong account to do an hour of work, and finding out from a rate limit
 * or an invoice. The header is where somebody already looks to see which
 * project they are in, so the account costs no extra place to look.
 *
 * Quiet by design — the same grey as everything else in the header, no icon,
 * no colour. It is read constantly and acted on rarely.
 *
 * Renders nothing when the agent named nothing, which includes every agent too
 * old to be asked. Switching accounts comes next; this only tells the truth
 * about the one in use.
 */
export function AccountChip({ agent }: { agent: AgentKind }): React.ReactElement | null {
  const account = useBeacon((state) => state.requirements.find((entry) => entry.id === agent))
    ?.account
  const label = accountLabel(agent, account)

  if (!label) return null

  return (
    <span className={styles['chip']} title={label.title}>
      <span className={styles['name']}>{label.name}</span>
      {label.detail ? <span className={styles['detail']}>{label.detail}</span> : null}
    </span>
  )
}
