import { useEffect } from 'react'
import { create } from 'zustand'

import { ipc } from '@/ipc'
import { supportsWorkstreams, type ClaudeCapabilities } from '@/types/beacon'

interface CapabilityState {
  capabilities: ClaudeCapabilities | null
  asked: boolean
}

/**
 * What the installed Claude Code can do.
 *
 * Asked once and kept: the backend works it out from the CLI's own `--help` and
 * caches it for the life of the process, and it cannot change under a running
 * Claude Code either.
 */
export const useCapabilities = create<CapabilityState>(() => ({
  capabilities: null,
  asked: false,
}))

export function loadCapabilities(): void {
  if (useCapabilities.getState().asked) return
  useCapabilities.setState({ asked: true })

  ipc
    .claudeCapabilities()
    .then((capabilities) => useCapabilities.setState({ capabilities }))
    .catch(() => {
      // Left as null, which every gate below reads as "cannot", so a feature
      // hides itself rather than failing when it is used.
    })
}

/**
 * Whether named, resumable conversations are possible on this machine.
 *
 * `false` until the answer arrives, so the chip appears once rather than
 * appearing and then vanishing on an older Claude Code.
 */
export function useWorkstreamsSupported(): boolean {
  const capabilities = useCapabilities((state) => state.capabilities)
  useEffect(loadCapabilities, [])
  return capabilities !== null && supportsWorkstreams(capabilities)
}
