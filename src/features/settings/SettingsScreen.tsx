import { useEffect, useState } from 'react'

import { useBeacon } from '@/app/store'
import { ClaudeSettings, CodexSettings, RequirementsSettings } from './AgentSettings'
import { AppearanceSettings, LayoutSettings } from './LookSettings'
import {
  GeneralSettings,
  KeyboardSettings,
  NotificationSettings,
  TerminalSettings,
} from './PreferenceSettings'
import styles from './SettingsScreen.module.css'

type SectionId =
  | 'appearance'
  | 'layout'
  | 'claude'
  | 'codex'
  | 'requirements'
  | 'general'
  | 'terminal'
  | 'notifications'
  | 'keyboard'
  | 'about'

/**
 * The sections, in the groups they are found by.
 *
 * Grouped by what a setting is about rather than by which panel happens to use
 * it: what you judge by eye, the agents and what Beacon installs into them,
 * how Beacon behaves for you, and Beacon itself.
 */
const GROUPS: Array<{ label: string; sections: Array<{ id: SectionId; label: string }> }> = [
  {
    label: 'Look',
    sections: [
      { id: 'appearance', label: 'Appearance' },
      { id: 'layout', label: 'Layout' },
    ],
  },
  {
    label: 'Agents',
    sections: [
      { id: 'claude', label: 'Claude Code' },
      { id: 'codex', label: 'Codex' },
      { id: 'requirements', label: 'Requirements' },
    ],
  },
  {
    label: 'Preferences',
    sections: [
      { id: 'general', label: 'General' },
      { id: 'terminal', label: 'Terminal' },
      { id: 'notifications', label: 'Notifications' },
      { id: 'keyboard', label: 'Keyboard' },
    ],
  },
  {
    label: 'Beacon',
    sections: [{ id: 'about', label: 'About' }],
  },
]

/**
 * Beacon's settings, as a screen rather than a menu.
 *
 * A popover was the wrong shape for this: choosing a layout means comparing
 * four of them, and a settings surface that grows will not fit beside a button.
 */
export function SettingsScreen({ onClose }: { onClose: () => void }): React.ReactElement {
  const [section, setSection] = useState<SectionId>('appearance')

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
          {GROUPS.map((group) => (
            <div className={styles['navGroup']} key={group.label}>
              <span className={styles['navHeading']}>{group.label}</span>
              {group.sections.map((entry) => (
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
            </div>
          ))}
        </nav>

        <div className={styles['content']}>
          <Section id={section} />
        </div>
      </div>
    </div>
  )
}

function Section({ id }: { id: SectionId }): React.ReactElement {
  switch (id) {
    case 'appearance':
      return <AppearanceSettings />
    case 'layout':
      return <LayoutSettings />
    case 'claude':
      return <ClaudeSettings />
    case 'codex':
      return <CodexSettings />
    case 'requirements':
      return <RequirementsSettings />
    case 'general':
      return <GeneralSettings />
    case 'terminal':
      return <TerminalSettings />
    case 'notifications':
      return <NotificationSettings />
    case 'keyboard':
      return <KeyboardSettings />
    case 'about':
      return <AboutSection />
  }
}

function AboutSection(): React.ReactElement {
  const version = useBeacon((s) => s.snapshot?.version)

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
          <span className={styles['rowLabel']}>Version</span>
          <span className={styles['rowValue']}>{version ?? '—'}</span>
        </div>
      </div>
    </section>
  )
}
