import { useState } from 'react'

import { ACCENT_PRESETS, ICON_PRESETS } from '@/lib/accent'
import styles from './InlineField.module.css'

interface InlineFieldProps {
  label: string
  initialValue: string
  submitLabel: string
  /** `accent` and `icon` are empty strings unless those pickers were shown. */
  onSubmit: (value: string, accent: string, icon: string) => void
  onCancel: () => void
  /** Shows the accent picker, starting from this colour. */
  withAccent?: string | undefined
  /** Shows the icon picker, starting from this emoji. */
  withIcon?: string | undefined
}

/**
 * A one-field form used inside popovers for renaming and creating.
 *
 * Beacon has no modal dialogs by design: a rename should never take the window
 * away from you.
 */
export function InlineField({
  label,
  initialValue,
  submitLabel,
  onSubmit,
  onCancel,
  withAccent,
  withIcon,
}: InlineFieldProps): React.ReactElement {
  const [value, setValue] = useState(initialValue)
  const [accent, setAccent] = useState(withAccent ?? '')
  const [icon, setIcon] = useState(withIcon ?? '')

  const submit = (): void => {
    const trimmed = value.trim()
    if (trimmed) onSubmit(trimmed, accent, icon)
  }

  return (
    <div className={styles['form']}>
      <span className={styles['label']}>{label}</span>
      <input
        className={styles['input']}
        value={value}
        autoFocus
        spellCheck={false}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') submit()
          if (event.key === 'Escape') onCancel()
        }}
      />

      {withAccent ? (
        <div className={styles['swatches']}>
          {ACCENT_PRESETS.map((preset) => (
            <button
              key={preset.value}
              type="button"
              title={preset.name}
              className={styles['swatch']}
              style={{ background: preset.value }}
              data-selected={preset.value === accent}
              onClick={() => setAccent(preset.value)}
            />
          ))}
        </div>
      ) : null}

      {withIcon !== undefined ? (
        <div className={styles['icons']}>
          <button
            type="button"
            title="No icon"
            className={styles['icon']}
            data-none="true"
            data-selected={icon === ''}
            onClick={() => setIcon('')}
          >
            ✕
          </button>
          {ICON_PRESETS.map((preset) => (
            <button
              key={preset}
              type="button"
              className={styles['icon']}
              data-selected={preset === icon}
              onClick={() => setIcon(preset)}
            >
              {preset}
            </button>
          ))}
        </div>
      ) : null}

      <div className={styles['actions']}>
        <button type="button" className={styles['button']} onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className={styles['button']} data-primary="true" onClick={submit}>
          {submitLabel}
        </button>
      </div>
    </div>
  )
}
