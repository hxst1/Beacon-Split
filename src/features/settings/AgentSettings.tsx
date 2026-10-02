import { useCallback, useEffect, useState } from 'react'

import { useBeacon } from '@/app/store'
import { errorMessage, ipc } from '@/ipc'
import { isWindows } from '@/lib/platform'
import { useWorkstreamsSupported } from '@/features/workstreams/capabilities'
import type { CodexIntegration, Integration, Requirement } from '@/types/beacon'
import styles from './SettingsScreen.module.css'

/**
 * The agents Beacon runs and what it installs into them: Claude Code's hooks
 * and status line, Codex's plugin, and the programs both need on the machine.
 */

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
 *
 * Beacon's own subagents come last: they change what a session is offered,
 * not what Beacon can see of it.
 */
export function ClaudeSettings(): React.ReactElement {
  const [integration, setIntegration] = useState<Integration | null>(null)
  const [error, setError] = useState<string | null>(null)
  const agents = useBeacon((s) => s.snapshot?.claudeAgents ?? true)
  const setClaudeAgents = useBeacon((s) => s.setClaudeAgents)
  const agentsPossible = useWorkstreamsSupported()

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
        ? 'Installed, but out of date: another copy of Beacon, or an older set of events'
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
      {agentsPossible ? (
        <section className={styles['section']}>
          <h2 className={styles['sectionTitle']}>Delegating</h2>
          <p className={styles['sectionNote']}>
            Beacon offers three small agents to the sessions it starts — one that searches the
            repository, one that runs tests, one that reviews a finished change. Each exists to
            keep a large pile of text out of the conversation doing the work and hand back only
            the part that changes what happens next.
          </p>
          <p className={styles['sectionNote']}>
            They are passed to Claude Code for one session at a time. Nothing is written into your
            projects, your <code>.claude/agents/</code> is untouched, and a Claude you start
            yourself never sees them. The cost is that their descriptions sit in every session's
            context whether they are used or not, which is why this is a switch.
          </p>

          <div className={styles['rows']}>
            <div className={styles['row']}>
              <span className={styles['rowLabel']}>Offer Beacon&rsquo;s agents to new sessions</span>
              <button
                type="button"
                className={styles['toggle']}
                data-on={agents}
                role="switch"
                aria-checked={agents}
                aria-label="Offer Beacon's agents to new sessions"
                onClick={() => void setClaudeAgents(!agents)}
              />
            </div>
          </div>
        </section>
      ) : null}
    </>
  )
}

/**
 * The Codex integration.
 *
 * A different shape from Claude Code's, because Codex is put together
 * differently: it loads hooks from plugins and plugins from marketplaces, so
 * Beacon generates one of each and asks Codex to install it.
 *
 * The part that cannot be automated is given its own paragraph rather than a
 * footnote. Codex keeps a hash of every hook it has been shown and runs none it
 * has not, so until somebody trusts Beacon's in Codex itself, installing has
 * achieved nothing visible — and a user who is not told that would reasonably
 * conclude the feature is broken.
 */
export function CodexSettings(): React.ReactElement {
  const [integration, setIntegration] = useState<CodexIntegration | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [working, setWorking] = useState(false)

  useEffect(() => {
    let cancelled = false
    ipc
      .codexIntegration()
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

  const act = (run: () => Promise<CodexIntegration>): void => {
    setWorking(true)
    setError(null)
    run()
      .then(setIntegration)
      .catch((err: unknown) => setError(errorMessage(err)))
      .finally(() => setWorking(false))
  }

  const plugin = integration?.plugin ?? null
  const label =
    plugin === 'installed'
      ? 'Installed'
      : plugin === 'stale'
        ? 'Installed, but out of date: another copy of Beacon, or an older set of events'
        : 'Not installed'
  // No version means no Codex. Everything here would fail, so it says why
  // instead of offering a button that cannot work.
  const missing = integration !== null && integration.capabilities.version === undefined

  return (
    <>
      <section className={styles['section']}>
        <h2 className={styles['sectionTitle']}>What Codex is doing</h2>
        <p className={styles['sectionNote']}>
          The same thing the Claude Code hooks do, for the other agent: a tab that can say whether
          Codex is working, has finished, or has stopped and is waiting for an answer.
        </p>

        {missing ? (
          <p className={styles['sectionNote']}>
            Codex is not installed on this machine. Requirements has the commands for it; a Codex
            panel works as a plain terminal until then.
          </p>
        ) : (
          <>
            <p className={styles['sectionNote']}>
              Codex takes hooks from plugins rather than from a settings file, so Beacon writes a
              small marketplace of its own and asks Codex to install one plugin from it. Nothing
              else in your Codex configuration is touched, and the hook does nothing outside
              Beacon — a Codex you start yourself has no socket to report to and exits
              immediately.
            </p>

            <div className={styles['command']}>{integration?.marketplace ?? '…'}</div>

            <div className={styles['buttons']}>
              <span className={styles['state']}>
                <span className={styles['stateDot']} data-state={plugin ?? 'notInstalled'} />
                {plugin === null ? 'Checking…' : label}
              </span>
              <span style={{ flex: 1 }} />

              {plugin === 'installed' ? (
                <button
                  type="button"
                  className={styles['resetAll']}
                  style={{ marginTop: 0 }}
                  disabled={working}
                  onClick={() => act(() => ipc.removeCodexPlugin())}
                >
                  Remove
                </button>
              ) : (
                <button
                  type="button"
                  className={styles['primary']}
                  disabled={working}
                  onClick={() => act(() => ipc.installCodexPlugin())}
                >
                  {working ? 'Installing…' : plugin === 'stale' ? 'Update' : 'Install'}
                </button>
              )}
            </div>

            {plugin === 'installed' ? (
              <p className={styles['sectionNote']}>
                One step left, and it is not Beacon's to take. Codex will not run a hook it has
                not been shown, so open Codex and run <code>/hooks</code> once to trust Beacon's.
                Until you do, a Codex panel runs perfectly well and simply reports nothing.
              </p>
            ) : null}
          </>
        )}

        {error ? <p className={styles['conflict']}>{error}</p> : null}
      </section>
    </>
  )
}

/**
 * What Beacon needs from the machine, and how to get what is missing.
 *
 * Written for somebody who was handed this application and has not set anything
 * up. A check that only reports "missing" leaves them exactly as stuck, so each
 * one says what it costs, where it was looked for, and what to run.
 */
export function RequirementsSettings(): React.ReactElement {
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
        {isWindows() ? ' on your PATH' : ' through your login shell'}, which is the same way a
        session finds it — so what this says is what will actually happen.
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
