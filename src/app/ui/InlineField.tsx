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
  /**
   * Says what is wrong with a value, or `null` when nothing is.
   *
   * Without one, a form with an empty field simply ignored the submit and sat
   * there looking broken. The default answers for the case every one of these
   * forms shares: a thing being named needs a name.
   */
  validate?: (value: string) => string | null
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
  validate = requireAName,
  withAccent,
  withIcon,
}: InlineFieldProps): React.ReactElement {
  const [value, setValue] = useState(initialValue)
  const [accent, setAccent] = useState(withAccent ?? '')
  const [icon, setIcon] = useState(withIcon ?? '')
  const [problem, setProblem] = useState<string | null>(null)

  const submit = (): void => {
    const wrong = validate(value)
    setProblem(wrong)
    if (!wrong) onSubmit(value.trim(), accent, icon)
  }

  return (
    <div className={styles['form']}>
      <span className={styles['label']}>{label}</span>
      <input
        className={styles['input']}
        value={value}
        autoFocus
        spellCheck={false}
        aria-label={label}
        aria-invalid={problem !== null}
        onChange={(event) => {
          setValue(event.target.value)
          // The complaint goes as soon as the user answers it.
          setProblem(null)
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') submit()
          if (event.key === 'Escape') onCancel()
        }}
      />

      {problem ? (
        <span className={styles['problem']} role="alert">
          {problem}
        </span>
      ) : null}

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

function requireAName(value: string): string | null {
  return value.trim() ? null : 'Enter a name'
}
