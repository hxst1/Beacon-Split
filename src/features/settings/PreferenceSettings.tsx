import { useEffect, useState } from 'react'

import { selectActiveWorkspace, selectBindings, useBeacon } from '@/app/store'
import { ipc } from '@/ipc'
import { ACTION_TITLES, bindingOf, describeBinding } from '@/app/keymap'
import { isMac, isWindows, modifierLabel } from '@/lib/platform'
import { describePermission } from '@/features/notifications/copy'
import { useNotificationPermission } from '@/features/notifications/permission'
import { playChime, type Chime } from '@/features/notifications/sound'
import { formatShell, parseShell } from './shell'
import styles from './SettingsScreen.module.css'

/**
 * How Beacon behaves for you: where projects live, what a terminal runs, when
 * it may interrupt, and which keys do what.
 */

/**
 * The preferences that belong to no panel: where projects are kept, whether
 * the file tree lists dotfiles, and whether a new version introduces itself.
 *
 * The last two can also be changed where they show — the tree's own switch and
 * the release notes' footer — and are here so there is one place to find them.
 */
export function GeneralSettings(): React.ReactElement {
  const workspace = useBeacon(selectActiveWorkspace)
  const workspaces = useBeacon((s) => s.snapshot?.workspaces.length ?? 0)
  const projectsHome = useBeacon((s) => s.snapshot?.projectsHome)
  const showHidden = useBeacon((s) => s.snapshot?.showHiddenFiles ?? false)
  const setShowHidden = useBeacon((s) => s.setShowHiddenFiles)
  const notices = useBeacon((s) => s.snapshot?.releaseNotices ?? true)
  const setReleaseNotices = useBeacon((s) => s.setReleaseNotices)

  return (
    <>
      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>Projects</h2>
        <p className={styles['sectionNote']}>
          Projects under this folder are stored relative to it, so the same configuration works on
          macOS, Linux and Windows. Projects elsewhere keep their absolute path.
        </p>
        <div className={styles['rows']}>
          <div className={styles['row']}>
            <span className={styles['rowLabel']}>Projects home</span>
            <span className={styles['rowValue']} title={projectsHome}>
              {projectsHome ?? '—'}
            </span>
          </div>
          <div className={styles['row']}>
            <span className={styles['rowLabel']}>Workspaces</span>
            <span className={styles['rowValue']}>{workspaces}</span>
          </div>
          {workspace ? (
            <div className={styles['row']}>
              <span className={styles['rowLabel']}>Projects in {workspace.name}</span>
              <span className={styles['rowValue']}>{workspace.projects.length}</span>
            </div>
          ) : null}
        </div>
      </section>

      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>Files and updates</h2>
        <div className={styles['rows']}>
          <div className={styles['row']}>
            <span className={styles['rowLabel']}>Show hidden files in the file tree</span>
            <button
              type="button"
              className={styles['toggle']}
              data-on={showHidden}
              role="switch"
              aria-checked={showHidden}
              aria-label="Show hidden files in the file tree"
              onClick={() => void setShowHidden(!showHidden)}
            />
          </div>
          <div className={styles['row']}>
            <span className={styles['rowLabel']}>Show what is new after an update</span>
            <button
              type="button"
              className={styles['toggle']}
              data-on={notices}
              role="switch"
              aria-checked={notices}
              aria-label="Show what is new after an update"
              onClick={() => void setReleaseNotices(!notices)}
            />
          </div>
        </div>
      </section>
    </>
  )
}

/**
 * What a terminal runs.
 *
 * The shell is a shell, not another terminal emulator: Beacon already is one,
 * and running kitty inside it would be an emulator inside an emulator. What
 * people want from that question is fish instead of zsh, which is this.
 */
export function TerminalSettings(): React.ReactElement {
  const shell = useBeacon((s) => s.snapshot?.shell ?? null)
  const setShell = useBeacon((s) => s.setShell)
  const [draft, setDraft] = useState<string | null>(null)

  const value = draft ?? (shell ? formatShell(shell) : '')

  const commit = (): void => {
    void setShell(parseShell(value))
    setDraft(null)
  }

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Shell</h2>
      {isWindows() ? (
        <p className={styles['sectionNote']}>
          Beacon is the terminal emulator, so this is a shell — PowerShell, Git Bash, cmd, nu —
          and not another one. Leave it empty for PowerShell: version 7 when it is installed,
          Windows PowerShell otherwise. Arguments go after the program; quote a path with spaces
          in it.
        </p>
      ) : (
        <p className={styles['sectionNote']}>
          Beacon is the terminal emulator, so this is a shell — zsh, fish, nu — and not another
          one. Leave it empty for your account's shell, started as a login shell, which is what
          every terminal does. Arguments go after the program.
        </p>
      )}

      <input
        className={styles['command']}
        style={{ width: '100%' }}
        placeholder={isWindows() ? 'Default: PowerShell -NoLogo' : `Default: ${'$SHELL'} -l`}
        spellCheck={false}
        value={value}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') commit()
          if (event.key === 'Escape') setDraft(null)
        }}
      />

      <p className={styles['sectionNote']} style={{ marginTop: 10 }}>
        Anything specific to another emulator — kitty's graphics or keyboard protocols — is not
        available here, and will not be: those belong to the emulator, which is the part Beacon
        replaces. Terminals already running keep the shell they started with.
      </p>
    </section>
  )
}

/**
 * When Beacon may interrupt you, and how: a banner, a sound, or both.
 */
export function NotificationSettings(): React.ReactElement {
  const notifications = useBeacon((s) => s.snapshot?.notifications ?? true)
  const setNotifications = useBeacon((s) => s.setNotifications)

  return (
    <section className={styles['section']}>
      <h2 className={styles['sectionTitle']}>Interruptions</h2>
      <p className={styles['sectionNote']}>
        With several projects open, the expensive thing is not switching tabs — it is a
        permission prompt in one of them going unseen, or a long turn finishing while you are in
        a browser. Beacon can say so, and only when you are not already looking at that project.
      </p>

      <div className={styles['rows']}>
        <div className={styles['row']}>
          <span className={styles['rowLabel']}>
            Notify me when Claude is waiting, or has finished a long turn
          </span>
          <button
            type="button"
            className={styles['toggle']}
            data-on={notifications}
            role="switch"
            aria-checked={notifications}
            aria-label="Notify me when Claude is waiting, or has finished a long turn"
            onClick={() => void setNotifications(!notifications)}
          />
        </div>
        <SoundRow chime="waiting" label="Play a sound when Claude is waiting for you" />
        <SoundRow chime="done" label="Play a sound when a long turn finishes" />
      </div>

      <SystemPermission />
    </section>
  )
}

/**
 * One sound's switch, and a way to hear it before deciding.
 *
 * Separate from notifications on purpose: a banner is for someone at the
 * screen and a sound for someone who is not, and either can be wanted alone.
 */
function SoundRow({ chime, label }: { chime: Chime; label: string }): React.ReactElement {
  const sounds = useBeacon((s) => s.snapshot?.sounds ?? { waiting: false, done: false })
  const setSounds = useBeacon((s) => s.setSounds)
  const on = sounds[chime]

  return (
    <div className={styles['row']}>
      <span className={styles['rowLabel']}>{label}</span>
      <button
        type="button"
        className={styles['secondary']}
        aria-label={`Play the sound: ${label.toLowerCase()}`}
        onClick={() => playChime(chime)}
      >
        Play
      </button>
      <button
        type="button"
        className={styles['toggle']}
        data-on={on}
        role="switch"
        aria-checked={on}
        aria-label={label}
        onClick={() => void setSounds({ ...sounds, [chime]: !on })}
      />
    </div>
  )
}

/**
 * What macOS itself will do, which is the half Beacon does not control.
 *
 * Shown as its own thing rather than folded into the switch above, because the
 * two fail differently: the switch is a preference, and this is a permission
 * that can be revoked from outside while Beacon is running. Conflating them
 * produces the worst version of this feature — a switch that is on, and
 * notifications that silently go nowhere.
 */
function SystemPermission(): React.ReactElement {
  const { permission, asking, request, openSettings, refresh } = useNotificationPermission()
  const copy = describePermission(permission)
  const [tested, setTested] = useState<string | null>(null)

  const test = async (): Promise<void> => {
    try {
      await ipc.sendNotification('Beacon Split', 'This is what a notification looks like.')
      setTested(
        `Sent. If nothing appeared, ${isMac() ? 'macOS' : 'the system'} is holding it back.`,
      )
    } catch (error) {
      setTested(String(error))
    }
    void refresh()
  }

  return (
    <>
      <div className={styles['rows']}>
        <div className={styles['row']}>
          <span className={styles['rowLabel']}>System permission</span>
          <span className={styles['rowValue']}>{copy.label}</span>
        </div>
      </div>

      {copy.hint ? <p className={styles['sectionNote']}>{copy.hint}</p> : null}

      <div className={styles['buttons']}>
        {copy.action === 'ask' ? (
          <button
            type="button"
            className={styles['primary']}
            disabled={asking}
            onClick={() => void request()}
          >
            {asking ? 'Waiting for macOS…' : 'Ask macOS'}
          </button>
        ) : null}

        {copy.action === 'openSettings' ? (
          <button type="button" className={styles['primary']} onClick={() => void openSettings()}>
            Open System Settings
          </button>
        ) : null}

        <button type="button" className={styles['secondary']} onClick={() => void test()}>
          Send a test notification
        </button>

        {tested ? <span className={styles['rowValue']}>{tested}</span> : null}
      </div>
    </>
  )
}

export function KeyboardSettings(): React.ReactElement {
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
        correct on macOS, Linux and Windows, and nothing fires while you are typing. Click a
        shortcut and press the new one; Escape cancels. Jumping to a numbered tab is fixed, since
        the binding is the number.
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
