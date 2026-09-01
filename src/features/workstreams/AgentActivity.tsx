import { useEffect, useState } from 'react'

import { agentLabel, elapsed, useProjectAgents } from './agents'
import styles from './AgentActivity.module.css'

/**
 * What a session has delegated, while it is delegating it.
 *
 * From outside, a Claude that has handed a large search to a subagent looks
 * exactly like a Claude that has gone quiet — the panel shows nothing and the
 * turn does not end. This is the difference, and it is the only reason the row
 * exists, so it says what is running and for how long and then goes away.
 *
 * Nothing is kept. There is no history of agents and there should not be one:
 * the point of delegating is that the detail did not need to be here.
 */
export function AgentActivity({ projectId }: { projectId: string }): React.ReactElement | null {
  const agents = useProjectAgents(projectId)
  const [now, setNow] = useState(() => Date.now())

  const running = agents.some((agent) => agent.finishedAt === undefined)
  useEffect(() => {
    if (!running) return
    const timer = window.setInterval(() => setNow(Date.now()), 1_000)
    return () => window.clearInterval(timer)
  }, [running])

  if (agents.length === 0) return null

  // Newest first, and only ever a couple: more than that on one line stops
  // being readable, and the count says the rest.
  const shown = [...agents].reverse()
  const [first] = shown

  return (
    <span className={styles['activity']} title={first?.summary ?? undefined}>
      <span className={styles['dot']} data-done={first?.finishedAt !== undefined} />
      <span className={styles['label']}>{first ? agentLabel(first) : ''}</span>
      <span className={styles['time']}>{first ? elapsed(first, now) : ''}</span>
      {shown.length > 1 ? <span className={styles['more']}>+{shown.length - 1}</span> : null}
    </span>
  )
}
