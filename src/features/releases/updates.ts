import { check } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { create } from 'zustand'

/**
 * Whether a newer Beacon is available, and installing it.
 *
 * Only meaningful in a build that was published with an update key. Anyone who
 * compiled Beacon themselves updates by pulling, so a check that finds no
 * configuration is a quiet no rather than an error — the alternative is an
 * application that complains about not being able to update itself every time a
 * developer opens it.
 */

export type UpdateState =
  | { status: 'unknown' }
  | { status: 'current' }
  | { status: 'available'; version: string; notes: string }
  | { status: 'downloading'; progress: number }
  | { status: 'ready'; version: string }

interface UpdateStore {
  state: UpdateState
  error: string | null
}

export const useUpdates = create<UpdateStore>(() => ({
  state: { status: 'unknown' },
  error: null,
}))

/** Looks once, on start. */
export async function checkForUpdate(): Promise<void> {
  try {
    const update = await check()
    useUpdates.setState({
      state: update
        ? { status: 'available', version: update.version, notes: update.body ?? '' }
        : { status: 'current' },
      error: null,
    })
  } catch {
    // No endpoint, no key, no network. None of those are worth saying.
    useUpdates.setState({ state: { status: 'unknown' }, error: null })
  }
}

/**
 * Downloads and installs, then restarts.
 *
 * Restarting is the whole point of the button, so it happens without asking
 * again: the asking already happened, and an application that installs an
 * update and then waits is one you have to remember to finish.
 */
export async function installUpdate(): Promise<void> {
  try {
    const update = await check()
    if (!update) {
      useUpdates.setState({ state: { status: 'current' } })
      return
    }

    let downloaded = 0
    let total = 0
    await update.downloadAndInstall((event) => {
      if (event.event === 'Started') total = event.data.contentLength ?? 0
      if (event.event === 'Progress') {
        downloaded += event.data.chunkLength
        useUpdates.setState({
          state: {
            status: 'downloading',
            progress: total > 0 ? Math.round((downloaded / total) * 100) : 0,
          },
        })
      }
      if (event.event === 'Finished') {
        useUpdates.setState({ state: { status: 'ready', version: update.version } })
      }
    })

    await relaunch()
  } catch (error) {
    useUpdates.setState({
      state: { status: 'unknown' },
      error: error instanceof Error ? error.message : String(error),
    })
  }
}
