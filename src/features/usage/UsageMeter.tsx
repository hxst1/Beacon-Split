import { useEffect, useState } from 'react'

import { Popover } from '@/app/ui/Popover'

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
  const [anchor, setAnchor] = useState<DOMRect | null>(null)
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

  // Everything worth knowing now lives in the panel, where it can be read at
  // leisure rather than raced against a tooltip.

  return (
    <>
      <button
        type="button"
        className={styles['meter']}
        data-stale={stale}
        title="What this session is costing"
        onClick={(event) => setAnchor(event.currentTarget.getBoundingClientRect())}
      >
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
      </button>

      {anchor ? (
        <Popover anchor={anchor} align="end" onClose={() => setAnchor(null)}>
          <div className={styles['details']}>
            <div className={styles['heading']}>Session</div>
            {sessionUsed !== null ? (
              <>
                <div className={styles['line']}>
                  <span className={styles['lineLabel']}>Five-hour allowance left</span>
                  <span className={styles['lineValue']}>{100 - sessionUsed}%</span>
                </div>
                <div className={styles['track']}>
                  <span
                    className={styles['fill']}
                    data-level={stale ? 'unknown' : levelOf(sessionUsed)}
                    style={{ width: `${100 - sessionUsed}%` }}
                  />
                </div>
                {resets ? (
                  <div className={styles['line']}>
                    <span className={styles['lineLabel']}>Comes back in</span>
                    <span className={styles['lineValue']}>{stale ? 'unknown' : resets}</span>
                  </div>
                ) : null}
              </>
            ) : (
              <div className={styles['note']}>
                Claude Code has not reported an allowance. Plans without rate limits do not have
                one.
              </div>
            )}
            {weekUsed !== null ? (
              <div className={styles['line']}>
                <span className={styles['lineLabel']}>Week left</span>
                <span className={styles['lineValue']}>{100 - weekUsed}%</span>
              </div>
            ) : null}

            <div className={styles['heading']}>Context</div>
            {contextUsed !== null ? (
              <>
                <div className={styles['line']}>
                  <span className={styles['lineLabel']}>{project?.name ?? 'This project'}</span>
                  <span className={styles['lineValue']}>{contextUsed}% used</span>
                </div>
                <div className={styles['track']}>
                  <span
                    className={styles['fill']}
                    data-level={levelOf(contextUsed)}
                    style={{ width: `${contextUsed}%` }}
                  />
                </div>
                {projectUsage?.report.contextUsedTokens ? (
                  <div className={styles['line']}>
                    <span className={styles['lineLabel']}>Tokens</span>
                    <span className={styles['lineValue']}>
                      {thousands(projectUsage.report.contextUsedTokens)}
                      {projectUsage.report.contextSize
                        ? ` / ${thousands(projectUsage.report.contextSize)}`
                        : ''}
                    </span>
                  </div>
                ) : null}
                {contextUsed >= 75 ? (
                  <div className={styles['note']}>
                    Getting full. A `/clear` starts the conversation over and gives the room back.
                  </div>
                ) : null}
              </>
            ) : (
              <div className={styles['note']}>Nothing reported for this project yet.</div>
            )}

            {reportedAt ? (
              <div className={styles['note']}>
                {stale
                  ? `Last reported ${howLongAgo(reportedAt, now)}. Claude Code has said nothing since, so these may be out of date.`
                  : `Reported ${howLongAgo(reportedAt, now)}.`}
              </div>
            ) : null}
          </div>
        </Popover>
      ) : null}
    </>
  )
}

/** `128,000` — easier to size up at a glance than a bare number. */
function thousands(value: number): string {
  return value.toLocaleString('en-US')
}
