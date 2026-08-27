import { useCallback, useEffect, useState } from 'react'

import { useBeacon } from '@/app/store'
import type { Clip } from '@/types/beacon'
import {
  age,
  closeDrawer,
  copyClip,
  forgetClip,
  forgetEveryClip,
  isLiteral,
  labelOf,
  preview,
  toggleDrawer,
  useClips,
} from './clips'
import styles from './ClipDrawer.module.css'

/** How often the ages on screen catch up with the clock. */
const TICK_MS = 30_000

/**
 * The drawer of things to copy, and the tab that opens it.
 *
 * Deliberately not a panel in the workbench. A clip is read for two seconds and
 * pasted somewhere else, so it must be reachable without giving up any of the
 * layout the user arranged — it overlays the work rather than displacing it,
 * and there is no scrim, because the terminal underneath is often the thing you
 * are about to paste into.
 */
export function ClipDrawer(): React.ReactElement {
  const clips = useClips((state) => state.clips)
  const open = useClips((state) => state.open)
  const unseen = useClips((state) => state.unseen)

  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (!open) return
    const timer = window.setInterval(() => setNow(Date.now()), TICK_MS)
    return () => window.clearInterval(timer)
  }, [open])

  // Escape closes it, like every other transient surface in Beacon.
  useEffect(() => {
    if (!open) return
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') closeDrawer()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open])

  return (
    <>
      <button
        type="button"
        className={styles['tab']}
        onClick={toggleDrawer}
        aria-expanded={open}
        aria-label={open ? 'Close the clip drawer' : 'Open the clip drawer'}
        title={open ? 'Close clips' : 'Clips'}
      >
        <span className={styles['arrow']} aria-hidden="true">
          {open ? '›' : '‹'}
        </span>
        {!open && unseen > 0 ? <span className={styles['unseen']}>{unseen}</span> : null}
      </button>

      <aside className={styles['drawer']} data-open={open} aria-hidden={!open}>
        <header className={styles['header']}>
          <h2 className={styles['heading']}>Clips</h2>
          <span className={styles['count']}>{clips.length}</span>
          {clips.length > 0 ? (
            <button type="button" className={styles['clear']} onClick={forgetEveryClip}>
              Clear
            </button>
          ) : null}
        </header>

        <div className={styles['list']}>
          {clips.length === 0 ? (
            <Empty />
          ) : (
            clips.map((clip) => <Card key={clip.id} clip={clip} now={now} />)
          )}
        </div>
      </aside>
    </>
  )
}

/**
 * What the drawer says before anything is in it.
 *
 * Says how to fill it rather than that it is empty. The tool only fires when
 * Claude recognises the moment, so the one thing worth telling somebody is that
 * asking plainly always works.
 */
function Empty(): React.ReactElement {
  return (
    <div className={styles['empty']}>
      <p>Nothing to copy yet.</p>
      <p className={styles['hint']}>
        Ask Claude for something you need to paste elsewhere — an environment variable, a
        command, the body of an email — and it lands here with a copy button.
      </p>
    </div>
  )
}

function Card({ clip, now }: { clip: Clip; now: number }): React.ReactElement {
  const copied = useClips((state) => state.copied === clip.id)
  const failed = useClips((state) => state.failed === clip.id)
  const project = useProjectName(clip.project)
  const [expanded, setExpanded] = useState(false)

  const shown = expanded ? { text: clip.body, truncated: false } : preview(clip.body)
  const copy = useCallback(() => void copyClip(clip), [clip])

  return (
    <article className={styles['card']} data-literal={isLiteral(clip.kind)}>
      <div className={styles['top']}>
        <span className={styles['title']} title={clip.title}>
          {clip.title}
        </span>
        <button
          type="button"
          className={styles['forget']}
          onClick={() => void forgetClip(clip.id)}
          aria-label={`Forget ${clip.title}`}
          title="Forget this"
        >
          ×
        </button>
      </div>

      <div className={styles['meta']}>
        <span className={styles['kind']}>{labelOf(clip.kind)}</span>
        {project ? <span className={styles['project']}>{project}</span> : null}
        <span className={styles['age']}>{age(clip.createdAt, now)}</span>
      </div>

      {/* `pre` rather than a div: the body is pasted exactly as it reads, and
          collapsing its whitespace here would make the two disagree. */}
      <pre className={styles['body']}>{shown.text}</pre>

      <div className={styles['actions']}>
        {shown.truncated || expanded ? (
          <button
            type="button"
            className={styles['more']}
            onClick={() => setExpanded((was) => !was)}
          >
            {expanded ? 'Show less' : 'Show all'}
          </button>
        ) : (
          <span />
        )}
        <button
          type="button"
          className={styles['copy']}
          onClick={copy}
          data-state={failed ? 'failed' : copied ? 'copied' : 'idle'}
        >
          {failed ? 'Could not copy' : copied ? 'Copied' : 'Copy'}
        </button>
      </div>
    </article>
  )
}

/**
 * The project a clip came from, by name.
 *
 * `null` when it came from a project that has since been removed — the clip is
 * still perfectly good, so it is shown without a source rather than hidden.
 */
function useProjectName(projectId: string): string | null {
  return useBeacon((state) => {
    for (const workspace of state.snapshot?.workspaces ?? []) {
      const found = workspace.projects.find((project) => project.id === projectId)
      if (found) return found.name
    }
    return null
  })
}
