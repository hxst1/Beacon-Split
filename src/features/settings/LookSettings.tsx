import { useEffect, useState } from 'react'

import { selectActiveWorkspace, selectHidden, useBeacon } from '@/app/store'
import { errorMessage, ipc } from '@/ipc'
import { ACCENT_PRESETS } from '@/lib/accent'
import { applyAppearance } from '@/lib/appearance'
import { PANEL_LABELS, prune } from '@/lib/layout'
import type { Appearance, LayoutNode, LayoutPreset, PanelId, Theme } from '@/types/beacon'
import { LayoutThumb } from './LayoutThumb'
import styles from './SettingsScreen.module.css'

/**
 * How the window looks and how it is arranged: everything you judge by eye.
 */

/** The theme, the window's material, and this workspace's colour. */
export function AppearanceSettings(): React.ReactElement {
  return (
    <>
      <ThemeAndMaterial />
      <AccentSection />
    </>
  )
}

/** Where the panels go, and which of them are shown. */
export function LayoutSettings(): React.ReactElement {
  return (
    <>
      <ArrangementSection />
      <PanelsSection />
    </>
  )
}

const THEMES: Array<{ theme: Theme; label: string; swatch: [string, string] }> = [
  { theme: 'system', label: 'System', swatch: ['#1c1c23', '#f2f2f5'] },
  { theme: 'dark', label: 'Dark', swatch: ['#15151b', '#08080b'] },
  { theme: 'light', label: 'Light', swatch: ['#ffffff', '#f6f6f8'] },
]

/**
 * The two things about the look that are taste rather than design.
 *
 * Applied as you drag rather than on release: how translucent a window should
 * be is not a number anyone knows in advance, it is something you find by
 * moving it and looking. Only the value you settle on is written to disk.
 */
function ThemeAndMaterial(): React.ReactElement {
  const appearance = useBeacon((s) => s.snapshot?.appearance)
  const setAppearance = useBeacon((s) => s.setAppearance)

  // Local while dragging: a slider that waits for a round trip stutters.
  const [draft, setDraft] = useState<Appearance | null>(null)
  const current = draft ?? appearance ?? null

  useEffect(() => setDraft(null), [appearance])

  if (!current) return <section className={styles['section']}>Loading…</section>

  const preview = (next: Appearance): void => {
    setDraft(next)
    applyAppearance(next)
  }

  const commit = (next: Appearance): void => {
    void setAppearance(next)
  }

  return (
    <>
      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>Theme</h2>
        <p className={styles['sectionNote']}>
          Beacon is dark by default and follows the system unless you say otherwise. The workspace
          accent works the same in either — it is the one colour a workspace declares, and
          everything tinted is mixed from it.
        </p>
        <div className={styles['themes']}>
          {THEMES.map(({ theme, label, swatch }) => (
            <button
              key={theme}
              type="button"
              className={styles['themeOption']}
              data-selected={theme === current.theme}
              onClick={() => commit({ ...current, theme })}
            >
              <span className={styles['themeSwatch']}>
                <span style={{ background: swatch[0] }} />
                <span style={{ background: swatch[1] }} />
              </span>
              {label}
            </button>
          ))}
        </div>
      </section>

      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>Material</h2>
        <p className={styles['sectionNote']}>
          How much of what is behind the window comes through, and whether it arrives sharp or
          frosted. Drag to see it; it is saved when you let go. Opacity stops at half — below
          that the desktop starts competing with the text.
        </p>

        <div className={styles['rows']}>
          <div className={styles['slider']}>
            <span className={styles['sliderLabel']}>Opacity</span>
            <input
              type="range"
              className={styles['sliderInput']}
              min={50}
              max={100}
              step={1}
              value={Math.round(current.windowOpacity * 100)}
              onChange={(event) =>
                preview({ ...current, windowOpacity: Number(event.target.value) / 100 })
              }
              onPointerUp={() => commit(current)}
              onKeyUp={() => commit(current)}
            />
            <span className={styles['sliderValue']}>
              {Math.round(current.windowOpacity * 100)}%
            </span>
          </div>

          {/* A switch rather than an amount. The window server picks the
              radius, so every position between off and on would have looked
              identical — which is what a blur slider here used to do. */}
          <div className={styles['row']}>
            <span className={styles['rowLabel']}>Frost what shows through</span>
            <button
              type="button"
              className={styles['toggle']}
              data-on={current.frosted}
              role="switch"
              aria-checked={current.frosted}
              aria-label="Frost what shows through"
              onClick={() => commit({ ...current, frosted: !current.frosted })}
            />
          </div>
        </div>

        <button
          type="button"
          className={styles['resetAll']}
          onClick={() => commit({ theme: current.theme, windowOpacity: 0.86, frosted: true })}
        >
          Back to the defaults
        </button>
      </section>
    </>
  )
}

/** The active workspace's accent, which tints everything else. */
function AccentSection(): React.ReactElement | null {
  const workspace = useBeacon(selectActiveWorkspace)
  const updateWorkspace = useBeacon((s) => s.updateWorkspace)

  if (!workspace) return null

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Accent</h2>
      <p className={styles['sectionNote']}>
        The colour for {workspace.name}. It is how you recognise which workspace you are in
        before reading anything, so it is deliberately subtle — a hairline and a faint bloom
        around the window, not a border.
      </p>
      <div className={styles['swatches']}>
        {ACCENT_PRESETS.map((preset) => (
          <button
            key={preset.value}
            type="button"
            title={preset.name}
            className={styles['swatch']}
            style={{ background: preset.value }}
            data-selected={preset.value === workspace.accent}
            onClick={() => void updateWorkspace(workspace.id, { accent: preset.value })}
          />
        ))}
      </div>
    </section>
  )
}

const PRESET_LABELS: Record<LayoutPreset, string> = {
  'claude-left': 'Claude left',
  'claude-right': 'Claude right',
  'claude-right-tall': 'Tall right',
  'claude-left-tall': 'Tall left',
  custom: 'Custom',
}

/**
 * Every panel that can be put away, derived rather than listed.
 *
 * It used to be written out by hand, and adding the Codex panel did not update
 * it — so the one panel nobody knew about was also the one Settings did not
 * offer. A list that has to be remembered is a list that will be forgotten.
 */
const TOGGLEABLE: PanelId[] = (Object.keys(PANEL_LABELS) as PanelId[]).sort()


function ArrangementSection(): React.ReactElement {
  const current = useBeacon((s) => s.snapshot?.preset)
  const setPreset = useBeacon((s) => s.setPreset)
  const hidden = useBeacon(selectHidden)
  const [presets, setPresets] = useState<Array<{ preset: LayoutPreset; layout: LayoutNode }>>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    ipc
      .layoutPresets()
      .then((options) => {
        if (!cancelled) setPresets(options)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err))
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Arrangement</h2>
      <p className={styles['sectionNote']}>
        Each preview is drawn from the layout it would apply, with the panels you have put away
        left out — so what you see is what you get. Dragging a splitter keeps the preset; the
        sizes are yours from then on.
      </p>

      {error ? (
        <p className={styles['sectionNote']}>{error}</p>
      ) : (
        <div className={styles['presets']}>
          {presets.map(({ preset, layout }) => (
            <button
              key={preset}
              type="button"
              className={styles['preset']}
              data-selected={preset === current}
              onClick={() => void setPreset(preset)}
            >
              {/* Pruned, or the preview would promise a window nobody has: every
                  preset holds all six panels, and Codex and the editor start
                  put away, so an untouched install was shown two agents side
                  by side and given one big one. */}
              <LayoutThumb node={prune(layout, hidden) ?? layout} />
              {PRESET_LABELS[preset]}
            </button>
          ))}
        </div>
      )}
    </section>
  )
}

function PanelsSection(): React.ReactElement {
  const hidden = useBeacon(selectHidden)
  const togglePanel = useBeacon((s) => s.togglePanel)

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Visible panels</h2>
      <p className={styles['sectionNote']}>
        A hidden panel keeps its place in the layout, so showing it again puts it back where it
        was. Claude cannot be hidden — it is what the window is for.
      </p>

      <div className={styles['rows']}>
        {TOGGLEABLE.map((panel) => {
          const visible = !hidden.includes(panel)
          return (
            <div className={styles['row']} key={panel}>
              <span className={styles['rowLabel']}>{PANEL_LABELS[panel]}</span>
              <button
                type="button"
                className={styles['toggle']}
                data-on={visible}
                role="switch"
                aria-checked={visible}
                aria-label={PANEL_LABELS[panel]}
                onClick={() => void togglePanel(panel)}
              />
            </div>
          )
        })}
      </div>
    </section>
  )
}
