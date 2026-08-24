import { useCallback, useEffect, useState } from 'react'

import { selectActiveWorkspace, selectBindings, selectHidden, useBeacon } from '@/app/store'
import { errorMessage, ipc } from '@/ipc'
import { ACCENT_PRESETS } from '@/lib/accent'
import { PANEL_LABELS } from '@/lib/layout'
import { ACTION_TITLES, bindingOf, describeBinding } from '@/app/keymap'
import { modifierLabel } from '@/lib/platform'
import type {
  Integration,
  LayoutNode,
  LayoutPreset,
  PanelId,
  Requirement,
} from '@/types/beacon'
import { LayoutThumb } from './LayoutThumb'
import styles from './SettingsScreen.module.css'

type SectionId =
  | 'requirements'
  | 'layout'
  | 'panels'
  | 'workspace'
  | 'keyboard'
  | 'claude'
  | 'about'

const SECTIONS: Array<{ id: SectionId; label: string }> = [
  { id: 'requirements', label: 'Requirements' },
  { id: 'layout', label: 'Layout' },
  { id: 'panels', label: 'Panels' },
  { id: 'workspace', label: 'Workspace' },
  { id: 'keyboard', label: 'Keyboard' },
  { id: 'claude', label: 'Claude Code' },
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
          {section === 'requirements' ? <RequirementsSection /> : null}
          {section === 'layout' ? <LayoutSection /> : null}
          {section === 'panels' ? <PanelsSection /> : null}
          {section === 'workspace' ? <WorkspaceSection /> : null}
          {section === 'keyboard' ? <KeyboardSection /> : null}
          {section === 'claude' ? <ClaudeSection /> : null}
          {section === 'about' ? <AboutSection /> : null}
        </div>
      </div>
    </div>
  )
}

/**
 * What Beacon needs from the machine, and how to get what is missing.
 *
 * Written for somebody who was handed this application and has not set anything
 * up. A check that only reports "missing" leaves them exactly as stuck, so each
 * one says what it costs, where it was looked for, and what to run.
 */
function RequirementsSection(): React.ReactElement {
  const [requirements, setRequirements] = useState<Requirement[] | null>(null)
  const [daemon, setDaemon] = useState<boolean | null>(null)
  const [copied, setCopied] = useState<string | null>(null)

  const look = useCallback(() => {
    setRequirements(null)
    void Promise.all([ipc.checkRequirements(), ipc.daemonAvailable()]).then(
      ([found, hasDaemon]) => {
        setRequirements(found)
        setDaemon(hasDaemon)
      },
    )
  }, [])

  useEffect(look, [look])

  const copy = (command: string): void => {
    void navigator.clipboard.writeText(command)
    setCopied(command)
    window.setTimeout(() => setCopied((current) => (current === command ? null : current)), 1200)
  }

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>What Beacon needs</h2>
      <p className={styles['sectionNote']}>
        Beacon runs the tools you already have rather than bundling its own. Each is looked for
        through your login shell, which is the same way a session finds it — so what this says is
        what will actually happen.
      </p>

      {daemon === false ? (
        <div className={styles['conflict']}>
          The session daemon is missing from this build, so terminals and Claude cannot start. That
          is a packaging fault rather than something you can install: it should sit beside the
          application. Rebuild with <code>pnpm app:build</code>, or ask whoever gave you this.
        </div>
      ) : null}

      {requirements === null ? (
        <p className={styles['sectionNote']}>Looking…</p>
      ) : (
        requirements.map((requirement) => (
          <div className={styles['requirement']} key={requirement.id}>
            <div className={styles['requirementHead']}>
              <span
                className={styles['stateDot']}
                data-state={requirement.path ? 'installed' : undefined}
              />
              <span className={styles['requirementName']}>{requirement.name}</span>
              {!requirement.path ? (
                <span className={styles['tag']} data-importance={requirement.importance}>
                  {requirement.importance === 'required' ? 'Needed' : 'Optional'}
                </span>
              ) : null}
              <span style={{ flex: 1 }} />
              {requirement.version ? (
                <span className={styles['found']}>{requirement.version}</span>
              ) : null}
            </div>

            {requirement.path ? (
              <div className={styles['found']} title={requirement.path}>
                {requirement.path}
              </div>
            ) : (
              <>
                <p className={styles['sectionNote']} style={{ marginBottom: 4 }}>
                  {requirement.whatBreaks}
                </p>
                {requirement.install.map((option) => (
                  <div className={styles['installOption']} key={option.command}>
                    <span className={styles['installLabel']}>{option.label}</span>
                    <button
                      type="button"
                      className={styles['installCommand']}
                      title="Copy"
                      onClick={() => copy(option.command)}
                    >
                      {copied === option.command ? 'Copied' : option.command}
                    </button>
                  </div>
                ))}
                {requirement.note ? (
                  <p className={styles['sectionNote']} style={{ marginTop: 10, marginBottom: 0 }}>
                    {requirement.note}
                  </p>
                ) : null}
              </>
            )}
          </div>
        ))
      )}

      <button type="button" className={styles['resetAll']} onClick={look}>
        Check again
      </button>
    </section>
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
  const bindings = useBeacon(selectBindings)
  const setBinding = useBeacon((s) => s.setBinding)
  const resetBindings = useBeacon((s) => s.resetBindings)

  const [capturing, setCapturing] = useState<string | null>(null)
  const [problem, setProblem] = useState<string | null>(null)

  // While a row is capturing it owns the keyboard: the shortcut being pressed
  // must not also fire the action it is being taken from.
  useEffect(() => {
    if (!capturing) return

    const onKeyDown = (event: KeyboardEvent): void => {
      event.preventDefault()
      event.stopPropagation()

      if (event.key === 'Escape') {
        setCapturing(null)
        return
      }

      const pressed = bindingOf(event)
      if (!pressed) {
        // Without the primary modifier it would fire while typing.
        setProblem(`A shortcut has to include ${modifierLabel()}.`)
        return
      }

      const action = capturing
      setCapturing(null)
      void setBinding(action, pressed).then(setProblem)
    }

    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [capturing, setBinding])

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Shortcuts</h2>
      <p className={styles['sectionNote']}>
        Every shortcut includes the primary modifier — {modifierLabel()} here — so one table is
        correct on macOS and Linux, and nothing fires while you are typing. Click a shortcut and
        press the new one; Escape cancels. Jumping to a numbered tab is fixed, since the binding is
        the number.
      </p>

      <div className={styles['rows']}>
        {bindings.map((entry) => {
          const changed = entry.binding !== entry.defaultBinding
          return (
            <div className={styles['row']} key={entry.action}>
              <span className={styles['rowLabel']}>
                {ACTION_TITLES[entry.action] ?? entry.action}
              </span>

              {changed ? (
                <button
                  type="button"
                  className={styles['revert']}
                  title={`Back to ${describeBinding(entry.defaultBinding)}`}
                  aria-label="Reset this shortcut"
                  onClick={() => void setBinding(entry.action, null).then(setProblem)}
                >
                  ↺
                </button>
              ) : (
                <span className={styles['revertSpacer']} />
              )}

              <button
                type="button"
                className={styles['binding']}
                data-capturing={capturing === entry.action}
                data-changed={changed}
                onClick={() => {
                  setProblem(null)
                  setCapturing(entry.action)
                }}
              >
                {capturing === entry.action ? 'Press a key…' : describeBinding(entry.binding)}
              </button>
            </div>
          )
        })}
      </div>

      {problem ? <p className={styles['conflict']}>{problem}</p> : null}

      <button
        type="button"
        className={styles['resetAll']}
        onClick={() => void resetBindings().then(() => setProblem(null))}
      >
        Reset all shortcuts
      </button>
    </section>
  )
}

/**
 * The Claude Code integration.
 *
 * Two halves, installed separately because they cost different things. Hooks
 * are additive — Beacon adds entries and removes them again. The status line is
 * a single slot, so taking it means displacing whatever was there; Beacon runs
 * the previous one rather than replacing it, and says so.
 *
 * Both opt-in. These write into a file belonging to another application, and
 * doing that unprompted is not Beacon's to decide however useful the result.
 */
function ClaudeSection(): React.ReactElement {
  const [integration, setIntegration] = useState<Integration | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    ipc
      .claudeIntegration()
      .then((found) => {
        if (!cancelled) setIntegration(found)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(errorMessage(err))
      })
    return () => {
      cancelled = true
    }
  }, [])

  const act = (run: () => Promise<Integration>): void => {
    run()
      .then(setIntegration)
      .catch((err: unknown) => setError(errorMessage(err)))
  }

  const hooks = integration?.hooks ?? null
  const hooksLabel =
    hooks === 'installed'
      ? 'Installed'
      : hooks === 'stale'
        ? 'Installed, but pointing at another copy of Beacon'
        : 'Not installed'

  return (
    <>
      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>What Claude is doing</h2>
        <p className={styles['sectionNote']}>
          Tabs can say whether Claude is working, has finished, or has stopped and is waiting for
          you to answer it. That last one is the point: with several projects open, the expensive
          thing is not switching tabs, it is not knowing which one needs you.
        </p>
        <p className={styles['sectionNote']}>
          Beacon adds one hook per event to your <code>~/.claude/settings.json</code> and touches
          nothing else. The hook does nothing outside Beacon — a Claude started anywhere else has
          no socket to report to, so it exits immediately.
        </p>

        <div className={styles['command']}>{integration?.hookCommand ?? '…'}</div>

        <div className={styles['buttons']}>
          <span className={styles['state']}>
            <span className={styles['stateDot']} data-state={hooks ?? 'notInstalled'} />
            {hooks === null ? 'Checking…' : hooksLabel}
          </span>
          <span style={{ flex: 1 }} />

          {hooks === 'installed' ? (
            <button
              type="button"
              className={styles['resetAll']}
              style={{ marginTop: 0 }}
              onClick={() => act(() => ipc.removeClaudeHooks().then(() => ipc.claudeIntegration()))}
            >
              Remove
            </button>
          ) : (
            <button
              type="button"
              className={styles['primary']}
              onClick={() => act(() => ipc.installClaudeHooks().then(() => ipc.claudeIntegration()))}
            >
              {hooks === 'stale' ? 'Update' : 'Install'}
            </button>
          )}
        </div>
      </section>

      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>What Claude is costing</h2>
        <p className={styles['sectionNote']}>
          Shows how much of the five-hour allowance is left in the title bar, and how full each
          project's context is — enough to decide which project to spend the rest of it on, and
          when a session is worth clearing.
        </p>
        <p className={styles['sectionNote']}>
          Claude Code only reports these through its status line, and a status line is one slot
          rather than a list. Beacon takes the slot and runs whatever was there, so what Claude
          Code shows does not change. Removing this puts your own line back exactly.
        </p>

        <div className={styles['command']}>{integration?.statusLineCommand ?? '…'}</div>

        <div className={styles['buttons']}>
          <span className={styles['state']}>
            <span
              className={styles['stateDot']}
              data-state={integration?.statusLine ? 'installed' : 'notInstalled'}
            />
            {integration === null
              ? 'Checking…'
              : integration.statusLine
                ? 'Installed'
                : 'Not installed'}
          </span>
          <span style={{ flex: 1 }} />

          {integration?.statusLine ? (
            <button
              type="button"
              className={styles['resetAll']}
              style={{ marginTop: 0 }}
              onClick={() => act(() => ipc.removeClaudeStatusLine())}
            >
              Remove
            </button>
          ) : (
            <button
              type="button"
              className={styles['primary']}
              onClick={() => act(() => ipc.installClaudeStatusLine())}
            >
              Install
            </button>
          )}
        </div>

        {error ? <p className={styles['conflict']}>{error}</p> : null}

        <p className={styles['sectionNote']} style={{ marginTop: 14 }}>
          A Claude session already running will not pick either of these up — restart it once they
          are installed. And if Claude Code signs you out mid-session, it stops reporting: Beacon
          then stops claiming to know, rather than leaving the last numbers on screen as though
          they were still true.
        </p>
      </section>
    </>
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
