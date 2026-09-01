import { useEffect } from 'react'

import { AppShell } from '@/app/AppShell'
import { Boot } from '@/app/Boot'
import { Welcome } from '@/features/workspaces/Welcome'
import { startConnectionTracking, startSystemThemeTracking, useBeacon } from '@/app/store'
import { startClipTracking } from '@/features/clips/clips'
import { startActivityTracking } from '@/features/terminal/activity'
import { checkForUpdate } from '@/features/releases/updates'
import { startNotifications } from '@/features/terminal/notify'
import { startUsageTracking } from '@/features/usage/usage'
import { startAgentTracking } from '@/features/workstreams/agents'

export function App(): React.ReactElement {
  const status = useBeacon((s) => s.status)
  const fatal = useBeacon((s) => s.fatal)
  const hasWorkspaces = useBeacon((s) => (s.snapshot?.workspaces.length ?? 0) > 0)
  const load = useBeacon((s) => s.load)

  useEffect(() => {
    // One place where the window starts listening to the daemon, rather than
    // several modules doing it as a side effect of being imported.
    const stop = [
      startConnectionTracking(),
      startActivityTracking(),
      startUsageTracking(),
      startAgentTracking(),
      startClipTracking(),
      startSystemThemeTracking(),
      startNotifications(),
    ]
    void load()
    // Once, on start. Nothing here nags: an update that is not urgent should
    // not interrupt, and one that is will still be there tomorrow.
    void checkForUpdate()

    return () => stop.forEach((unsubscribe) => unsubscribe())
  }, [load])

  if (status === 'loading') return <Boot />
  if (status === 'error') return <Boot error={fatal} />
  return hasWorkspaces ? <AppShell /> : <Welcome />
}
