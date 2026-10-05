import { useEffect, useLayoutEffect, useRef, useState } from 'react'

import { describeBinding } from '@/app/keymap'
import { focusPanel } from '@/app/panelFocus'
import { useBeacon } from '@/app/store'
import { playChime, type Chime } from '@/features/notifications/sound'
import { pickFolder } from '@/ipc'
import type { Theme } from '@/types/beacon'
import { anchorSelector, guideSteps, placeCard, type Anchor, type Box } from './steps'
import styles from './WelcomeGuide.module.css'

/**
 * The welcome guide: the basics set, then a walk round the window, ending at
 * the Claude panel where the first sign-in happens.
 *
 * Deliberately on its own. Its switches call the same store actions Settings
 * does rather than borrowing Settings' rows, and it finds what it points at by
 * the `data-panel` and `data-region` hooks rather than by how any screen is
 * laid out — so Settings can be rearranged, and its buttons moved, without the
 * guide going stale. Whatever it cannot find on screen, it explains in the
 * middle of the window instead.
 *
 * It counts as seen as soon as it opens: one that came back on every start
 * until somebody finished it would be the nagging it is meant to spare them.
 */
export function WelcomeGuide({ onClose }: { onClose: () => void }): React.ReactElement {
  const snapshot = useBeacon((s) => s.snapshot)
  const missing = useBeacon((s) => s.missing)
  const markWelcomed = useBeacon((s) => s.markWelcomed)
  const showPanel = useBeacon((s) => s.showPanel)
  const addProject = useBeacon((s) => s.addProject)
  const [index, setIndex] = useState(0)
  const cardRef = useRef<HTMLDivElement>(null)

  const welcomed = snapshot?.welcomed ?? true
  useEffect(() => {
    if (!welcomed) void markWelcomed()
  }, [welcomed, markWelcomed])

  const workspace = snapshot?.workspaces.find((w) => w.id === snapshot.activeWorkspace)
  const steps = guideSteps({
    hasProject: (workspace?.projects.length ?? 0) > 0,
    hidden: snapshot?.hidden ?? [],
    hint: (action) => {
      const binding = snapshot?.bindings.find((entry) => entry.action === action)?.binding
      return binding ? describeBinding(binding) : undefined
    },
    claudeInstalled: !missing.some((requirement) => requirement.id === 'claude'),
  })
  // Steps can drop out underneath: adding the first project removes the step
  // that asked for it, and the same index is then the step after it.
  const at = Math.min(index, steps.length - 1)
  const step = steps[at]
  const last = at === steps.length - 1

  const target = useAnchorBox(step?.anchor)
  const viewport = useViewport()
  const [cardSize, setCardSize] = useState({ width: 360, height: 220 })
  useLayoutEffect(() => {
    const card = cardRef.current
    if (!card) return
    const measure = (): void => setCardSize({ width: card.offsetWidth, height: card.offsetHeight })
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(card)
    return () => observer.disconnect()
  }, [])

  // The card keeps the keyboard, so nothing typed during the guide lands in a
  // terminal underneath it. It takes it back after every render — a button
  // that did its job and went away leaves the focus nowhere — and whenever
  // something else takes it: a shortcut focusing a panel, or the folder
  // picker handing the window back.
  useEffect(() => {
    const card = cardRef.current
    if (card && !card.contains(document.activeElement)) card.focus()
  })
  useEffect(() => {
    const reclaim = (): void => {
      const card = cardRef.current
      if (card && !card.contains(document.activeElement)) card.focus()
    }
    document.addEventListener('focusin', reclaim)
    window.addEventListener('focus', reclaim)
    return () => {
      document.removeEventListener('focusin', reclaim)
      window.removeEventListener('focus', reclaim)
    }
  }, [])

  const next = (): void => (last ? onClose() : setIndex(at + 1))
  const back = (): void => setIndex(Math.max(0, at - 1))

  const goToClaude = async (): Promise<void> => {
    await showPanel('claude')
    onClose()
    // After the overlay is gone, or the focus would land back on the card.
    requestAnimationFrame(() => focusPanel('claude'))
  }

  // Every key stops here. The guide is modal: Beacon's shortcuts listen on the
  // window, and one that showed a panel or focused a terminal mid-step would
  // move things out from under it.
  const onKeyDown = (event: React.KeyboardEvent): void => {
    event.stopPropagation()
    const inControl = (event.target as HTMLElement).closest('button')
    if (event.key === 'Escape') onClose()
    else if (event.key === 'ArrowRight' && !inControl) next()
    else if (event.key === 'ArrowLeft' && !inControl) back()
    else if (event.key === 'Tab') keepTabInside(event)
    else return
    event.preventDefault()
  }

  const keepTabInside = (event: React.KeyboardEvent): void => {
    const card = cardRef.current
    if (!card) return
    const stops = [...card.querySelectorAll<HTMLElement>('button:not([disabled])')]
    if (stops.length === 0) return
    const position = stops.indexOf(document.activeElement as HTMLElement)
    const direction = event.shiftKey ? -1 : 1
    // From the card itself, the first stop forwards and the last one back.
    const from = position === -1 ? (event.shiftKey ? 0 : -1) : position
    const nextStop = stops[(from + direction + stops.length) % stops.length]
    nextStop?.focus()
  }

  if (!step) return <></>

  const position = placeCard(target, cardSize, viewport)

  return (
    <div
      className={styles['root']}
      onKeyDown={onKeyDown}
      // A click on the dimmed window is not a click on anything: it must not
      // take the focus away from the card either.
      onMouseDown={(event) => {
        if (!cardRef.current?.contains(event.target as Node)) event.preventDefault()
      }}
    >
      {target ? (
        <div
          className={styles['spotlight']}
          style={{
            top: target.top - 4,
            left: target.left - 4,
            width: target.width + 8,
            height: target.height + 8,
          }}
        />
      ) : (
        <div className={styles['scrim']} />
      )}

      <div
        ref={cardRef}
        className={styles['card']}
        style={position}
        role="dialog"
        aria-modal="true"
        aria-labelledby="welcome-title"
        tabIndex={-1}
      >
        <div className={styles['progress']} aria-hidden="true">
          {steps.map((candidate, i) => (
            <span key={candidate.id} className={styles['dot']} data-on={i === at} />
          ))}
        </div>

        <h2 id="welcome-title" className={styles['title']}>
          {step.title}
        </h2>
        {step.paragraphs.map((paragraph) => (
          <p key={paragraph} className={styles['text']}>
            {paragraph}
          </p>
        ))}

        {step.action === 'basics' ? <Basics /> : null}

        <div className={styles['footer']}>
          {last ? null : (
            <button type="button" className={styles['quiet']} onClick={onClose}>
              Skip
            </button>
          )}
          <span className={styles['count']}>
            {at + 1} / {steps.length}
          </span>
          {at > 0 ? (
            <button type="button" className={styles['secondary']} onClick={back}>
              Back
            </button>
          ) : null}

          {step.action === 'addProject' ? (
            <button
              type="button"
              className={styles['primary']}
              onClick={() =>
                void (async () => {
                  const folder = await pickFolder('Add project', snapshot?.projectsHome)
                  if (folder) await addProject(folder)
                })()
              }
            >
              Add project…
            </button>
          ) : null}
          {step.action === 'signIn' ? (
            <button type="button" className={styles['primary']} onClick={() => void goToClaude()}>
              Go to Claude
            </button>
          ) : (
            <button
              type="button"
              className={step.action === 'addProject' ? styles['secondary'] : styles['primary']}
              onClick={next}
            >
              {step.action === 'addProject' ? 'Later' : 'Next'}
            </button>
          )}
        </div>
      </div>
    </div>
  )
}

/**
 * Where a step's target is on screen, kept current while it moves.
 *
 * Looked up again whenever the page changes, not only once: the target may
 * appear after the step opens — a panel the step itself showed, the first
 * project's panels once it is added.
 */
function useAnchorBox(anchor: Anchor | undefined): Box | null {
  const [box, setBox] = useState<Box | null>(null)
  const selector = anchor ? anchorSelector(anchor) : null

  useLayoutEffect(() => {
    if (!selector) {
      setBox(null)
      return
    }

    let frame = 0
    let watched: HTMLElement | null = null
    const resize = new ResizeObserver(() => measure())
    const measure = (): void => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        const element = document.querySelector<HTMLElement>(selector)
        // The target can change size without the page doing so — a panel
        // beside it shown or hidden — so it is watched itself once found.
        if (element !== watched) {
          if (watched) resize.unobserve(watched)
          if (element) resize.observe(element)
          watched = element
        }
        const rect = element?.getBoundingClientRect()
        setBox((previous) => {
          if (!rect || rect.width === 0 || rect.height === 0) return null
          const next = { top: rect.top, left: rect.left, width: rect.width, height: rect.height }
          const same =
            previous &&
            previous.top === next.top &&
            previous.left === next.left &&
            previous.width === next.width &&
            previous.height === next.height
          return same ? previous : next
        })
      })
    }

    measure()
    resize.observe(document.body)
    const mutations = new MutationObserver(measure)
    mutations.observe(document.body, { childList: true, subtree: true })
    window.addEventListener('resize', measure)

    return () => {
      cancelAnimationFrame(frame)
      resize.disconnect()
      mutations.disconnect()
      window.removeEventListener('resize', measure)
    }
  }, [selector])

  return box
}

function useViewport(): { width: number; height: number } {
  const read = (): { width: number; height: number } => ({
    width: window.innerWidth,
    height: window.innerHeight,
  })
  const [viewport, setViewport] = useState(read)
  useEffect(() => {
    const update = (): void => setViewport(read())
    window.addEventListener('resize', update)
    return () => window.removeEventListener('resize', update)
  }, [])
  return viewport
}

const THEMES: { theme: Theme; label: string }[] = [
  { theme: 'system', label: 'System' },
  { theme: 'dark', label: 'Dark' },
  { theme: 'light', label: 'Light' },
]

const CHIMES: { chime: Chime; label: string }[] = [
  { chime: 'waiting', label: 'A sound when Claude is waiting for you' },
  { chime: 'done', label: 'A sound when a long turn finishes' },
]

/**
 * The basics, set from here.
 *
 * Its own controls on the store's own actions, the ones Settings calls too:
 * the same switch, wherever Settings ends up keeping it.
 */
function Basics(): React.ReactElement | null {
  const snapshot = useBeacon((s) => s.snapshot)
  const setAppearance = useBeacon((s) => s.setAppearance)
  const setNotifications = useBeacon((s) => s.setNotifications)
  const setSounds = useBeacon((s) => s.setSounds)

  if (!snapshot) return null
  const { appearance, notifications, sounds } = snapshot

  return (
    <div className={styles['basics']}>
      <div className={styles['row']}>
        <span className={styles['label']}>Theme</span>
        <div className={styles['segments']} role="radiogroup" aria-label="Theme">
          {THEMES.map(({ theme, label }) => (
            <button
              key={theme}
              type="button"
              role="radio"
              aria-checked={appearance.theme === theme}
              className={styles['segment']}
              data-on={appearance.theme === theme}
              onClick={() => void setAppearance({ ...appearance, theme })}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className={styles['row']}>
        <span className={styles['label']}>Notify me when a project needs me</span>
        <Switch
          on={notifications}
          label="Notify me when a project needs me"
          onChange={(on) => void setNotifications(on)}
        />
      </div>

      {CHIMES.map(({ chime, label }) => (
        <div key={chime} className={styles['row']}>
          <span className={styles['label']}>{label}</span>
          <button
            type="button"
            className={styles['quiet']}
            aria-label={`Play the sound: ${label.toLowerCase()}`}
            onClick={() => playChime(chime)}
          >
            Play
          </button>
          <Switch
            on={sounds[chime]}
            label={label}
            onChange={(on) => void setSounds({ ...sounds, [chime]: on })}
          />
        </div>
      ))}

      <p className={styles['aside']}>
        Only ever for a project you are not looking at. All of this is in Settings too.
      </p>
    </div>
  )
}

function Switch({
  on,
  label,
  onChange,
}: {
  on: boolean
  label: string
  onChange: (on: boolean) => void
}): React.ReactElement {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      className={styles['switch']}
      data-on={on}
      onClick={() => onChange(!on)}
    />
  )
}
