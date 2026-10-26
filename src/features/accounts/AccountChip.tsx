import { useState } from 'react'

import { MenuHeading, MenuItem, MenuSeparator } from '@/app/ui/Menu'
import { Popover } from '@/app/ui/Popover'
import { InlineField } from '@/app/ui/InlineField'
import { useBeacon } from '@/app/store'
import type { AgentKind } from '@/types/beacon'
import { accountEmail, accountLabel, agentName, switchedTo } from './account'
import styles from './AccountChip.module.css'

/** Which one-field form the menu is showing, if any. */
type Form = 'add' | 'rename'

/**
 * Which account this agent is using, and a click to change it.
 *
 * It is in the panel header because the mistake it prevents is expensive and
 * silent: asking the wrong account to do an hour of work, and finding out from
 * a rate limit or an invoice. The header is where somebody already looks to
 * see which project they are in, so the account costs no extra place to look.
 *
 * Quiet by design — the same grey as everything else in the header, no icon,
 * no colour. It is read constantly and acted on rarely.
 *
 * An account is a configuration home (ADR-099), which means switching cannot
 * be instant: the home is an environment variable and a running agent cannot
 * be handed another one. The menu says so, in the menu, rather than letting
 * somebody discover it by watching nothing happen.
 */
export function AccountChip({ agent }: { agent: AgentKind }): React.ReactElement | null {
  const account = useBeacon((state) => state.requirements.find((entry) => entry.id === agent))
    ?.account
  const accounts = useBeacon((state) => state.snapshot?.accounts)
  const addAccount = useBeacon((state) => state.addAccount)
  const setAccount = useBeacon((state) => state.setAccount)
  const renameAccount = useBeacon((state) => state.renameAccount)
  const forgetAccount = useBeacon((state) => state.forgetAccount)

  const [anchor, setAnchor] = useState<DOMRect | null>(null)
  const [form, setForm] = useState<Form | null>(null)

  const mine = accounts?.[agent]
  const added = mine?.added ?? []
  const current = added.find((profile) => profile.id === mine?.active)
  // A name that matches no account reads as the machine's own, which is what
  // the backend does with it too — so a hand-edited settings file leaves the
  // menu agreeing with the session rather than showing nothing selected.
  const active = current?.id
  const label = accountLabel(agent, account)

  // Nothing known and nothing added: there is no account to name and none to
  // switch to, so there is nothing worth a place in the header. Every agent
  // too old to be asked lands here.
  if (!label && added.length === 0) return null

  const close = (): void => {
    setAnchor(null)
    setForm(null)
  }

  const pick = (id: string | null, name: string): void => {
    close()
    if (id === (active ?? null)) return
    void setAccount(agent, id, switchedTo(agent, name))
  }

  return (
    <>
      <button
        type="button"
        className={styles['chip']}
        title={label?.title ?? `Which account ${agentName(agent)} is using`}
        aria-expanded={anchor !== null}
        onClick={(event) => setAnchor(event.currentTarget.getBoundingClientRect())}
      >
        <span className={styles['name']}>{current?.label ?? label?.name ?? 'account'}</span>
        {/* Under the person's own label the sign-in state has nowhere else to
            show, and it is the one fact that stops the account working. */}
        {current && label?.signedOut ? (
          <span className={styles['detail']}>sign in</span>
        ) : label?.detail ? (
          <span className={styles['detail']}>{label.detail}</span>
        ) : null}
        <span className={styles['caret']} aria-hidden="true">
          ▾
        </span>
      </button>

      {anchor ? (
        <Popover anchor={anchor} align="end" onClose={close}>
          {form ? (
            <InlineField
              label={form === 'add' ? `Name this ${agentName(agent)} account` : 'Rename it'}
              initialValue={form === 'rename' ? (current?.label ?? '') : ''}
              submitLabel={form === 'add' ? 'Add' : 'Rename'}
              onCancel={() => setForm(null)}
              onSubmit={(name) => {
                close()
                if (form === 'rename' && current) {
                  void renameAccount(agent, current.id, name)
                } else {
                  void addAccount(agent, name)
                }
              }}
            />
          ) : (
            <div className={styles['menu']}>
              <MenuHeading>{agentName(agent)} account</MenuHeading>

              <MenuItem
                label="This machine"
                // Only when it is the one in use: the identity above was asked
                // of the active account, so under any other it would be that
                // account's email sitting against this row.
                hint={(active ? null : accountEmail(account)) ?? undefined}
                active={!active}
                onSelect={() => pick(null, 'this machine')}
              />
              {added.map((profile) => (
                <MenuItem
                  key={profile.id}
                  label={profile.label}
                  active={profile.id === active}
                  onSelect={() => pick(profile.id, profile.label)}
                />
              ))}

              <MenuSeparator />
              <MenuItem label="Add an account…" onSelect={() => setForm('add')} />
              {current ? (
                <>
                  <MenuItem label="Rename…" onSelect={() => setForm('rename')} />
                  <MenuItem
                    label={`Forget ${current.label}`}
                    hint="keeps its files"
                    danger
                    onSelect={() => {
                      close()
                      void forgetAccount(agent, current.id)
                    }}
                  />
                </>
              ) : null}

              <div className={styles['note']}>
                An account is a folder {agentName(agent)} keeps its own settings and sign-in in.
                Switching applies the next time it starts, and the conversation does not come
                along — it stays with the account it happened in.
              </div>
              {added.length === 0 ? (
                <div className={styles['note']}>
                  A new one takes your settings with it and is signed out. Run{' '}
                  <code>{agent === 'claude' ? 'claude' : 'codex login'}</code> in this project's
                  terminal once it is selected, and it signs in there.
                </div>
              ) : null}
            </div>
          )}
        </Popover>
      ) : null}
    </>
  )
}
