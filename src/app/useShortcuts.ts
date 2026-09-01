import { useEffect } from 'react'

import { hasPrimaryModifier } from '@/lib/platform'
import { bindingOf, missingHandlers, runBinding } from './keymap'
import { useBeacon } from './store'

/**
 * The keyboard layer.
 *
 * Bindings come from the backend, already resolved from defaults and whatever
 * the user changed, and are expressed against "the primary modifier" so one
 * table is correct on macOS and Linux.
 */
export function useShortcuts(): void {
  const bindings = useBeacon((s) => s.snapshot?.bindings)

  useEffect(() => {
    if (!bindings) return

    const unimplemented = missingHandlers(bindings)
    if (unimplemented.length > 0) {
      // A bindable action with nothing behind it would look like a broken
      // shortcut; better to say so where a developer will see it.
      console.warn('actions with no handler:', unimplemented.join(', '))
    }

    const onKeyDown = (event: KeyboardEvent): void => {
      if (!hasPrimaryModifier(event)) return
      // Whatever has focus got there first. The editor handles Cmd+S and its
      // own search bindings itself and marks them handled; running the
      // application binding as well would do the same thing twice.
      if (event.defaultPrevented) return

      const pressed = bindingOf(event)
      if (!pressed) return

      // Jumping to a numbered tab is not rebindable: the binding is the number,
      // and there are as many of them as there are projects.
      const digit = /^mod\+([1-9])$/.exec(pressed)
      if (digit?.[1]) {
        event.preventDefault()
        void useBeacon.getState().selectProjectAt(Number(digit[1]) - 1)
        return
      }

      // The overlays are how you reach everything without the mouse, so they
      // work from anywhere — including from inside a text field.
      const overlayAction = bindings.find(
        (binding) =>
          binding.binding === pressed &&
          (binding.action === 'palette.open' || binding.action === 'quickOpen.open'),
      )
      if (!overlayAction) {
        // A plain field is left alone: its native editing shortcuts do not mark
        // themselves handled, so an application binding would quietly shadow
        // one. The code editor is not in that position — it claims what it
        // wants above — and shortcuts dying whenever the cursor was in a file
        // made half the keyboard layer feel broken.
        const target = event.target as HTMLElement | null
        if (target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA') return
      }

      if (runBinding(event, bindings)) event.preventDefault()
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [bindings])
}
