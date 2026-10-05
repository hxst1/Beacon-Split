import { useEffect, useState } from 'react'

import { Popover } from '@/app/ui/Popover'

import { useAgentInFront } from '@/app/agentInFront'
import { selectActiveProject, useBeacon } from '@/app/store'
import type { AgentKind, UsageReport } from '@/types/beacon'
import {
  adviceFor,
  cacheIsCold,
  contextHealth,
  healthLabel,
  hasReset,
  howLongAgo,
  leftInWindow,
  levelOf,
  percent,
  staleness,
  thousands,
  untilReset,
  useAccountUsage,
  useProjectUsage,
} from './usage'
import styles from './UsageMeter.module.css'

/** What each agent is called where the meter has room to say it. */
const AGENT_NAMES: Record<AgentKind, string> = { claude: 'Claude', codex: 'Codex' }

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
  // Whose numbers these are. Two agents in one window spend two allowances
  // against two accounts, and a meter that quietly showed one of them while
  // you worked in the other would be the wrong number at the moment it
  // mattered — so it follows the agent you are in.
  const inFront = useAgentInFront()
  const project = useBeacon(selectActiveProject)

  const theirs = useAccountUsage(inFront)
  const theirContext = useProjectUsage(project?.id ?? '', inFront)
  // The other one, for the first seconds of a Codex nobody has read yet: a
  // meter that blinks out when you move between panels looks broken. It is
  // only ever shown with its agent named, so it cannot be mistaken for yours.
  const other: AgentKind = inFront === 'claude' ? 'codex' : 'claude'
  const others = useAccountUsage(other)
  const otherContext = useProjectUsage(project?.id ?? '', other)

  const standIn = theirs === null && theirContext === null
  const agent = standIn ? other : inFront
  const account = standIn ? others : theirs
  const projectUsage = standIn ? otherContext : theirContext

  // Only to keep the countdown and the staleness honest; twice a minute is as
  // precise as either needs to be.
  const [anchor, setAnchor] = useState<DOMRect | null>(null)
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [])

  const fiveHourResetsAt = account?.report.fiveHourResetsAt
  const sessionLeft = leftInWindow(account?.report.fiveHourUsedPercentage, fiveHourResetsAt, now)
  const sessionReset = account !== null && hasReset(fiveHourResetsAt, now)
  const contextUsed = percent(projectUsage?.report.contextUsedPercentage)

  if (sessionLeft === null && !sessionReset && contextUsed === null) return null

  // Named only when there is another agent it could have been. On the window
  // most people have — one agent — the meter says what it always said.
  const named = standIn || (others !== null || otherContext !== null) ? AGENT_NAMES[agent] : null

  // Two numbers, dated apart: the allowance is as old as the response that
  // brought it — whichever project that was, and an idle session repeating
  // itself does not make it newer — while the context is this project's own.
  // One being old says nothing about the other, so each dims on its own, and
  // the whole meter only when everything on it is old.
  const {
    limits: limitsStale,
    context: contextStale,
    all: allStale,
  } = staleness(account, projectUsage, now)
  const advice = adviceFor(projectUsage?.report, now)
  const resets = untilReset(fiveHourResetsAt, now)
  const weekLeft = leftInWindow(
    account?.report.sevenDayUsedPercentage,
    account?.report.sevenDayResetsAt,
    now,
  )

  // Everything worth knowing now lives in the panel, where it can be read at
  // leisure rather than raced against a tooltip.

  return (
    <>
      <button
        type="button"
        className={styles['meter']}
        data-stale={allStale}
        title={`What ${AGENT_NAMES[agent]} is costing`}
        onClick={(event) => setAnchor(event.currentTarget.getBoundingClientRect())}
      >
      {named ? <span className={styles['agent']}>{named}</span> : null}
      {sessionLeft !== null ? (
        <span className={styles['part']} data-stale={limitsStale}>
          <span className={styles['bar']}>
            <span
              className={styles['fill']}
              data-level={limitsStale ? 'unknown' : levelOf(100 - sessionLeft)}
              style={{ width: `${sessionLeft}%` }}
            />
          </span>
          <span>{sessionLeft}%</span>
          {limitsStale ? (
            <span className={styles['muted']}>· stale</span>
          ) : resets ? (
            <span className={styles['muted']}>· {resets}</span>
          ) : null}
        </span>
      ) : null}

      {sessionReset ? <span className={styles['muted']}>5h reset</span> : null}

      {contextUsed !== null ? (
        <span className={styles['muted']} data-stale={contextStale}>
          {sessionLeft !== null || sessionReset ? '· ' : ''}
          {contextUsed}% ctx
          {contextStale ? ' · stale' : ''}
        </span>
      ) : null}
      </button>

      {anchor ? (
        <Popover anchor={anchor} align="end" onClose={() => setAnchor(null)}>
          <div className={styles['details']}>
            <div className={styles['heading']}>{AGENT_NAMES[agent]}</div>
            {sessionReset && fiveHourResetsAt !== undefined ? (
              <div className={styles['note']}>
                The five-hour window came round {howLongAgo(fiveHourResetsAt * 1000, now)}.
                Claude Code says how much of the new one is gone with its next reply.
              </div>
            ) : sessionLeft !== null ? (
              <>
                <div className={styles['line']}>
                  <span className={styles['lineLabel']}>Five-hour allowance left</span>
                  <span className={styles['lineValue']}>{sessionLeft}%</span>
                </div>
                <div className={styles['track']}>
                  <span
                    className={styles['fill']}
                    data-level={limitsStale ? 'unknown' : levelOf(100 - sessionLeft)}
                    style={{ width: `${sessionLeft}%` }}
                  />
                </div>
                {resets ? (
                  <div className={styles['line']}>
                    <span className={styles['lineLabel']}>Comes back in</span>
                    <span className={styles['lineValue']}>{limitsStale ? 'unknown' : resets}</span>
                  </div>
                ) : null}
              </>
            ) : (
              <div className={styles['note']}>
                Claude Code has not reported an allowance. Plans without rate limits do not have
                one.
              </div>
            )}
            {weekLeft !== null ? (
              <div className={styles['line']}>
                <span className={styles['lineLabel']}>Week left</span>
                <span className={styles['lineValue']}>{weekLeft}%</span>
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
                <div className={styles['line']}>
                  <span className={styles['lineLabel']}>Health</span>
                  <span className={styles['lineValue']}>
                    {healthLabel(contextHealth(contextUsed))}
                  </span>
                </div>
                <Cache report={projectUsage?.report} now={now} />
                {advice ? (
                  <div className={styles['note']}>
                    <strong>{advice.title}.</strong> {advice.detail}
                  </div>
                ) : null}
              </>
            ) : (
              <div className={styles['note']}>Nothing reported for this project yet.</div>
            )}

            {/* Each number dated on its own line: they come from different
                sessions, and can be different ages. */}
            {account ? (
              <div className={styles['note']}>
                {limitsStale
                  ? `Allowance last reported ${howLongAgo(account.limitsAt, now)}. No session here has had a reply since, so anything used after that — on claude.ai or another machine too — is not in it.`
                  : account.report.limitsAgeUnknown
                    ? `Allowance first heard ${howLongAgo(account.limitsAt, now)}, from a session Beacon had not heard from before — it may be older than that. The next reply settles it.`
                    : `Allowance reported ${howLongAgo(account.limitsAt, now)}.`}
              </div>
            ) : null}
            {projectUsage ? (
              <div className={styles['note']}>
                {contextStale
                  ? `Context last reported ${howLongAgo(projectUsage.at, now)}. Claude Code has said nothing since, so it may be out of date.`
                  : `Context reported ${howLongAgo(projectUsage.at, now)}.`}
              </div>
            ) : null}
          </div>
        </Popover>
      ) : null}
    </>
  )
}

/**
 * What the prompt cache is doing, when Claude Code has said anything about it.
 *
 * Absent until the first API response, and absence is shown as absence: a cache
 * reported as cold before there has been a request to cache would read as a
 * warning about something that has not happened yet.
 */
function Cache({
  report,
  now,
}: {
  report: UsageReport | undefined
  now: number
}): React.ReactElement | null {
  const cache = report?.promptCache
  if (!cache) return null

  const cold = cacheIsCold(cache, now)
  const expires = untilReset(cache.expiresAt, now)
  const ratio = cache.hitRatio === undefined ? null : Math.round(cache.hitRatio * 100)

  return (
    <>
      <div className={styles['line']}>
        <span className={styles['lineLabel']}>Cache</span>
        <span className={styles['lineValue']}>
          {cold ? 'cold' : 'warm'}
          {ratio !== null ? ` · ${ratio}% hits` : ''}
        </span>
      </div>
      {!cold && expires ? (
        <div className={styles['line']}>
          <span className={styles['lineLabel']}>Goes cold in</span>
          <span className={styles['lineValue']}>{expires}</span>
        </div>
      ) : null}
      {cold && cache.recacheTokensIfCold ? (
        <div className={styles['line']}>
          <span className={styles['lineLabel']}>Rebuilds</span>
          <span className={styles['lineValue']}>
            {thousands(cache.recacheTokensIfCold)} tokens
          </span>
        </div>
      ) : null}
    </>
  )
}
