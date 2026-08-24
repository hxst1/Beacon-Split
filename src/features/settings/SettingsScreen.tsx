import { useEffect, useState } from 'react'

import { selectActiveWorkspace, selectHidden, useBeacon } from '@/app/store'
import { errorMessage, ipc } from '@/ipc'
import { ACCENT_PRESETS } from '@/lib/accent'
import { PANEL_LABELS } from '@/lib/layout'
import { modifierLabel } from '@/lib/platform'
import type { LayoutNode, LayoutPreset, PanelId } from '@/types/beacon'
import { LayoutThumb } from './LayoutThumb'
import styles from './SettingsScreen.module.css'

type SectionId = 'layout' | 'panels' | 'workspace' | 'keyboard' | 'about'

const SECTIONS: Array<{ id: SectionId; label: string }> = [
  { id: 'layout', label: 'Layout' },
  { id: 'panels', label: 'Panels' },
  { id: 'workspace', label: 'Workspace' },
  { id: 'keyboard', label: 'Keyboard' },
  { id: 'about', label: 'About' },
]

const PRESET_LABELS: Record<LayoutPreset, string> = {
  'claude-left': 'Claude left',
  'claude-right': 'Claude right',
  'claude-right-tall': 'Tall right',
  'claude-left-tall': 'Tall left',
  custom: 'Custom',
}

const TOGGLEABLE: PanelId[] = ['editor', 'files', 'git', 'terminal']

const BINDINGS: Array<{ keys: string[]; action: string }> = [
  { keys: ['K'], action: 'Command palette' },
  { keys: ['P'], action: 'Quick open' },
  { keys: ['1', '…', '9'], action: 'Switch to project' },
  { keys: ['E'], action: 'Toggle Files' },
  { keys: ['G'], action: 'Toggle Git' },
  { keys: ['O'], action: 'Toggle the editor' },
  { keys: ['J'], action: 'Toggle the terminal' },
  { keys: ['↩'], action: 'Fullscreen the focused panel' },
  { keys: ['S'], action: 'Save the file being edited' },
]

/**
 * Beacon's settings, as a screen rather than a menu.
 *
 * A popover was the wrong shape for this: choosing a layout means comparing
 * four of them, and a settings surface that grows will not fit beside a button.
 */
export function SettingsScreen({ onClose }: { onClose: () => void }): React.ReactElement {
  const [section, setSection] = useState<SectionId>('layout')

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [onClose])

  return (
    <div className={styles['scrim']} onPointerDown={onClose}>
      <div className={styles['screen']} onPointerDown={(event) => event.stopPropagation()}>
        <header className={styles['header']}>
          <span className={styles['title']}>Settings</span>
          <button type="button" className={styles['close']} aria-label="Close settings" onClick={onClose}>
            ✕
          </button>
        </header>

        <nav className={styles['nav']}>
          {SECTIONS.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={styles['navItem']}
              data-active={entry.id === section}
              onClick={() => setSection(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </nav>

        <div className={styles['content']}>
          {section === 'layout' ? <LayoutSection /> : null}
          {section === 'panels' ? <PanelsSection /> : null}
          {section === 'workspace' ? <WorkspaceSection /> : null}
          {section === 'keyboard' ? <KeyboardSection /> : null}
          {section === 'about' ? <AboutSection /> : null}
        </div>
      </div>
    </div>
  )
}

function LayoutSection(): React.ReactElement {
  const current = useBeacon((s) => s.snapshot?.preset)
  const setPreset = useBeacon((s) => s.setPreset)
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
        Each preview is drawn from the layout it would apply, so what you see is what you get.
        Dragging a splitter keeps the preset; the sizes are yours from then on.
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
              <LayoutThumb node={layout} />
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

function WorkspaceSection(): React.ReactElement {
  const workspace = useBeacon(selectActiveWorkspace)
  const updateWorkspace = useBeacon((s) => s.updateWorkspace)
  const projectsHome = useBeacon((s) => s.snapshot?.projectsHome)

  if (!workspace) return <section className={styles['section']}>No workspace.</section>

  return (
    <>
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

      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>Projects</h2>
        <p className={styles['sectionNote']}>
          Projects under this folder are stored relative to it, so the same configuration works on
          macOS and Linux. Projects elsewhere keep their absolute path.
        </p>
        <div className={styles['rows']}>
          <div className={styles['row']}>
            <span className={styles['rowLabel']}>Projects home</span>
            <span className={styles['rowValue']} title={projectsHome}>
              {projectsHome ?? '—'}
            </span>
          </div>
          <div className={styles['row']}>
            <span className={styles['rowLabel']}>Projects in {workspace.name}</span>
            <span className={styles['rowValue']}>{workspace.projects.length}</span>
          </div>
        </div>
      </section>
    </>
  )
}

function KeyboardSection(): React.ReactElement {
  const modifier = modifierLabel()

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Shortcuts</h2>
      <p className={styles['sectionNote']}>
        Bindings are written against the primary modifier — {modifier} here — so the same table is
        correct on macOS and Linux. Editing them is not built yet; commands and shortcuts already
        resolve against one registry, so it will be a surface over this list rather than a rework.
      </p>

      <div className={styles['rows']}>
        {BINDINGS.map((binding) => (
          <div className={styles['row']} key={binding.action}>
            <span className={styles['rowLabel']}>{binding.action}</span>
            <span className={styles['keys']}>
              <span className={styles['key']}>{modifier}</span>
              {binding.keys.map((key) => (
                <span className={styles['key']} key={key}>
                  {key}
                </span>
              ))}
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}

function AboutSection(): React.ReactElement {
  const projectsHome = useBeacon((s) => s.snapshot?.projectsHome)
  const workspaces = useBeacon((s) => s.snapshot?.workspaces.length ?? 0)

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Beacon</h2>
      <p className={styles['sectionNote']}>
        An agent-first development workspace. Settings, workspaces and window state are three JSON
        files, written atomically and versioned, so they can be read, edited and synced like
        anything else you own.
      </p>

      <div className={styles['rows']}>
        <div className={styles['row']}>
          <span className={styles['rowLabel']}>Workspaces</span>
          <span className={styles['rowValue']}>{workspaces}</span>
        </div>
        <div className={styles['row']}>
          <span className={styles['rowLabel']}>Projects home</span>
          <span className={styles['rowValue']} title={projectsHome}>
            {projectsHome ?? '—'}
          </span>
        </div>
      </div>
    </section>
  )
}
