import { useEffect, useState } from 'react'

import { MenuHeading, MenuItem, MenuSeparator } from '@/app/ui/Menu'
import { Popover } from '@/app/ui/Popover'
import { InlineField } from '@/app/ui/InlineField'
import { adviceFor, contextHealth, useProjectUsage } from '@/features/usage/usage'
import { workstreamLabel, type Workstream } from '@/types/beacon'
import { useWorkstreamsSupported } from './capabilities'
import {
  forkWorkstream,
  lastWorkedIn,
  loadWorkstreams,
  renameWorkstream,
  resumeWorkstream,
  startWorkstream,
  useCurrentWorkstream,
  useProjectWorkstreams,
  useWorkstreams,
} from './workstreams'
import styles from './WorkstreamChip.module.css'

/** Which one-field form the menu is showing, if any. */
type Form = 'new' | 'rename' | 'fork'

/**
 * Which piece of work this Claude is on, in the panel header.
 *
 * The one thing that has to be visible without opening anything: with several
 * conversations behind one project, the expensive mistake is carrying on in the
 * wrong one. Everything else — starting, resuming, forking, renaming — is a
 * click away rather than in the header, because the header is read constantly
 * and used rarely.
 *
 * Renders nothing on a Claude Code without the flags for it. A chip that could
 * only tell you the feature is unavailable would be worse than no chip.
 */
export function WorkstreamChip({
  workspaceId,
  projectId,
}: {
  workspaceId: string
  projectId: string
}): React.ReactElement | null {
  const supported = useWorkstreamsSupported()
  const { list, current } = useProjectWorkstreams(projectId)
  const stream = useCurrentWorkstream(projectId)
  const usage = useProjectUsage(projectId)
  const busy = useWorkstreams((state) => state.busy === projectId)

  const [anchor, setAnchor] = useState<DOMRect | null>(null)
  const [form, setForm] = useState<Form | null>(null)
  const [dismissed, setDismissed] = useState<string[]>([])
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (supported) void loadWorkstreams(projectId)
  }, [supported, projectId])

  // Only so "12m ago" and the advice stay honest while the menu is open.
  useEffect(() => {
    if (!anchor) return
    const timer = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [anchor])

  if (!supported) return null

  const close = (): void => {
    setAnchor(null)
    setForm(null)
  }

  const context = usage?.report.contextUsedPercentage
  const label = stream ? workstreamLabel(stream) : 'no workstream'
  const others = list.filter((other) => other.id !== current)
  const advice = adviceFor(usage?.report, now)
  const showAdvice = advice && !dismissed.includes(advice.id)

  return (
    <>
      <button
        type="button"
        className={styles['chip']}
        title="Which piece of work this Claude is on"
        data-busy={busy}
        data-advice={showAdvice ? true : undefined}
        onClick={(event) => setAnchor(event.currentTarget.getBoundingClientRect())}
      >
        <span className={styles['name']}>{label}</span>
        {context !== undefined ? (
          <span className={styles['context']} data-health={contextHealth(context)}>
            {Math.round(context)}%
          </span>
        ) : null}
        <span className={styles['caret']} aria-hidden="true">
          ▾
        </span>
      </button>

      {anchor ? (
        <Popover anchor={anchor} align="end" onClose={close}>
          {form ? (
            <InlineField
              label={
                form === 'rename'
                  ? 'Name this workstream'
                  : form === 'fork'
                    ? 'Name the fork'
                    : 'Name the new workstream'
              }
              initialValue={form === 'rename' ? (stream?.name ?? '') : ''}
              submitLabel={form === 'rename' ? 'Rename' : 'Start'}
              validate={() => null}
              onCancel={() => setForm(null)}
              onSubmit={(value) => {
                const name = value.trim() === '' ? null : value.trim()
                close()
                if (form === 'rename' && stream) {
                  void renameWorkstream(projectId, stream.id, name)
                } else if (form === 'fork' && stream) {
                  void forkWorkstream(workspaceId, projectId, stream.id, name)
                } else {
                  void startWorkstream(workspaceId, projectId, name)
                }
              }}
            />
          ) : (
            <div className={styles['menu']}>
              <MenuHeading>Workstream</MenuHeading>
              <Current stream={stream} context={context} />

              {showAdvice ? (
                <div className={styles['advice']}>
                  <div className={styles['adviceTitle']}>{advice.title}</div>
                  <div className={styles['adviceDetail']}>{advice.detail}</div>
                  <button
                    type="button"
                    className={styles['dismiss']}
                    onClick={() => setDismissed((seen) => [...seen, advice.id])}
                  >
                    Dismiss
                  </button>
                </div>
              ) : null}

              <MenuSeparator />
              <MenuItem label="New workstream…" onSelect={() => setForm('new')} />
              {stream ? (
                <>
                  <MenuItem label="Rename…" onSelect={() => setForm('rename')} />
                  {stream.resumable ? (
                    <MenuItem
                      label="Fork…"
                      hint="keeps the history"
                      onSelect={() => setForm('fork')}
                    />
                  ) : null}
                </>
              ) : null}

              {others.length > 0 ? (
                <>
                  <MenuSeparator />
                  <MenuHeading>Recent</MenuHeading>
                  {others.map((other) => (
                    <MenuItem
                      key={other.id}
                      label={workstreamLabel(other)}
                      hint={lastWorkedIn(other.lastActiveAt, now)}
                      onSelect={() => {
                        close()
                        void resumeWorkstream(workspaceId, projectId, other.id)
                      }}
                    />
                  ))}
                </>
              ) : null}
            </div>
          )}
        </Popover>
      ) : null}
    </>
  )
}

/** What is known about the conversation this Claude is in. */
function Current({
  stream,
  context,
}: {
  stream: Workstream | null
  context: number | undefined
}): React.ReactElement {
  if (!stream) {
    return (
      <div className={styles['note']}>
        Claude has not been started in this project yet. Starting one gives it a workstream.
      </div>
    )
  }

  return (
    <div className={styles['current']}>
      <div className={styles['currentName']}>{workstreamLabel(stream)}</div>
      {context !== undefined ? (
        <>
          <div className={styles['track']}>
            <span
              className={styles['fill']}
              data-health={contextHealth(context)}
              style={{ width: `${Math.min(100, Math.max(0, context))}%` }}
            />
          </div>
          <div className={styles['line']}>
            <span>{stream.model ?? 'Context'}</span>
            <span>{Math.round(context)}% used</span>
          </div>
        </>
      ) : (
        <div className={styles['note']}>Claude Code has not reported on this one yet.</div>
      )}
      {stream.forkedFrom ? <div className={styles['note']}>Forked from another workstream.</div> : null}
    </div>
  )
}
