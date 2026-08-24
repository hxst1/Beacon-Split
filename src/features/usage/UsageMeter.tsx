import { useEffect, useState } from 'react'

import { selectActiveProject, useBeacon } from '@/app/store'
import {
  howLongAgo,
  isStale,
  levelOf,
  percent,
  untilReset,
  useAccountUsage,
  useProjectUsage,
} from './usage'
import styles from './UsageMeter.module.css'

/**
 * How much of the session allowance is left, in the title bar.
 *
 * The number that changes what you do: with several projects competing for one
 * allowance, watching it run down is what tells you to spend the rest of it on
 * the thing that matters.
 *
 * Shows nothing until Claude Code has said something — an empty gauge would
 * read as an empty allowance — and dims once what it said is old enough that it
 * should not be taken as current.
 */
export function UsageMeter(): React.ReactElement | null {
  const account = useAccountUsage()
  const project = useBeacon(selectActiveProject)
  const projectUsage = useProjectUsage(project?.id ?? '')

  // Only to keep the countdown and the staleness honest; twice a minute is as
  // precise as either needs to be.
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [])

  const sessionUsed = percent(account?.report.fiveHourUsedPercentage)
  const contextUsed = percent(projectUsage?.report.contextUsedPercentage)

  if (sessionUsed === null && contextUsed === null) return null

  const stale = isStale(account ?? projectUsage, now)
  const resets = untilReset(account?.report.fiveHourResetsAt, now)
  const weekUsed = percent(account?.report.sevenDayUsedPercentage)
  const reportedAt = account?.at ?? projectUsage?.at

  const title = [
    sessionUsed !== null ? `${100 - sessionUsed}% of the 5-hour allowance left` : null,
    resets && !stale ? `resets in ${resets}` : null,
    weekUsed !== null ? `${100 - weekUsed}% of the week left` : null,
    contextUsed !== null && project
      ? `${project.name}: ${contextUsed}% of the context used${
          projectUsage?.report.contextUsedTokens
            ? ` (${thousands(projectUsage.report.contextUsedTokens)} tokens)`
            : ''
        }`
      : null,
    reportedAt
      ? stale
        ? `Last reported ${howLongAgo(reportedAt, now)} — Claude Code has not said anything since, so this may be out of date`
        : `Reported ${howLongAgo(reportedAt, now)}`
      : null,
  ]
    .filter(Boolean)
    .join('\n')

  return (
    <div className={styles['meter']} title={title} data-stale={stale}>
      {sessionUsed !== null ? (
        <>
          <span className={styles['bar']}>
            <span
              className={styles['fill']}
              data-level={stale ? 'unknown' : levelOf(sessionUsed)}
              style={{ width: `${100 - sessionUsed}%` }}
            />
          </span>
          <span>{100 - sessionUsed}%</span>
          {resets && !stale ? <span className={styles['muted']}>· {resets}</span> : null}
        </>
      ) : null}

      {contextUsed !== null ? (
        <span className={styles['muted']}>
          {sessionUsed !== null ? '· ' : ''}
          {contextUsed}% ctx
        </span>
      ) : null}

      {stale ? <span className={styles['muted']}>· stale</span> : null}
    </div>
  )
}

/** `128,000` — easier to size up at a glance than a bare number. */
function thousands(value: number): string {
  return value.toLocaleString('en-US')
}
