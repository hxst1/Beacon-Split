import { useEffect, useState } from 'react'

import { MenuHeading, MenuItem, MenuSeparator } from '@/app/ui/Menu'
import { selectHidden, useBeacon } from '@/app/store'
import { errorMessage, ipc } from '@/ipc'
import { PANEL_LABELS } from '@/lib/layout'
import { shortcutLabel } from '@/lib/platform'
import type { LayoutNode, LayoutPreset, PanelId } from '@/types/beacon'
import { LayoutThumb } from './LayoutThumb'
import styles from './Settings.module.css'

const PRESET_LABELS: Record<LayoutPreset, string> = {
  'claude-left': 'Claude left',
  'claude-right': 'Claude right',
  'claude-right-tall': 'Tall right',
  'claude-left-tall': 'Tall left',
  custom: 'Custom',
}

const TOGGLEABLE: Array<{ panel: PanelId; key: string }> = [
  { panel: 'files', key: 'E' },
  { panel: 'git', key: 'G' },
  { panel: 'terminal', key: 'J' },
]

interface PresetOption {
  preset: LayoutPreset
  layout: LayoutNode
}

/** Layout, panel visibility and the paths Beacon is working from. */
export function Settings({ onDone }: { onDone: () => void }): React.ReactElement {
  const current = useBeacon((s) => s.snapshot?.preset)
  const hidden = useBeacon(selectHidden)
  const projectsHome = useBeacon((s) => s.snapshot?.projectsHome)
  const setPreset = useBeacon((s) => s.setPreset)
  const togglePanel = useBeacon((s) => s.togglePanel)

  const [presets, setPresets] = useState<PresetOption[]>([])
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
    <div className={styles['root']}>
      <MenuHeading>Layout</MenuHeading>
      {error ? (
        <div className={styles['path']}>{error}</div>
      ) : (
        <div className={styles['presets']}>
          {presets.map(({ preset, layout }) => (
            <button
              key={preset}
              type="button"
              className={styles['preset']}
              data-selected={preset === current}
              onClick={() => {
                void setPreset(preset)
                onDone()
              }}
            >
              <LayoutThumb node={layout} />
              {PRESET_LABELS[preset]}
            </button>
          ))}
        </div>
      )}

      <MenuSeparator />
      <MenuHeading>Panels</MenuHeading>
      {TOGGLEABLE.map(({ panel, key }) => (
        <MenuItem
          key={panel}
          label={PANEL_LABELS[panel]}
          hint={shortcutLabel(key)}
          active={!hidden.includes(panel)}
          onSelect={() => void togglePanel(panel)}
        />
      ))}

      <MenuSeparator />
      <MenuHeading>Projects home</MenuHeading>
      <div className={styles['path']} title={projectsHome}>
        {projectsHome ?? '—'}
      </div>
    </div>
  )
}
