import { usePanelFocus } from '@/app/panelFocus'
import { selectHidden, useBeacon } from '@/app/store'
import { AGENT_PANELS } from '@/lib/layout'
import type { AgentKind } from '@/types/beacon'

/**
 * The agent the user is working in.
 *
 * The one the keyboard was in last, as long as it is still on screen; failing
 * that the only one that is, which is the common case — Codex starts put away,
 * so most windows have exactly one agent in them and never have to choose.
 *
 * Two panels now read this, and they read it for the same reason: with two
 * agents open, anything that shows one agent's numbers or one agent's branch
 * without saying which is a way to act on the wrong one. The answer is the
 * agent you are looking at.
 */
export function useAgentInFront(): AgentKind {
  const lastAgent = usePanelFocus((s) => s.lastAgent)
  const hidden = useBeacon(selectHidden)

  const shown = AGENT_PANELS.filter((panel) => !hidden.includes(panel)) as AgentKind[]
  if (lastAgent !== null && shown.includes(lastAgent)) return lastAgent
  return shown[0] ?? 'claude'
}
