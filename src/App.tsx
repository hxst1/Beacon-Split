import { useEffect } from 'react'

import { AppShell } from '@/app/AppShell'
import { Boot } from '@/app/Boot'
import { Welcome } from '@/features/workspaces/Welcome'
import { startConnectionTracking, startSystemThemeTracking, useBeacon } from '@/app/store'
import { startActivityTracking } from '@/features/terminal/activity'
import { startUsageTracking } from '@/features/usage/usage'

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
      startSystemThemeTracking(),
    ]
    void load()

    return () => stop.forEach((unsubscribe) => unsubscribe())
  }, [load])

  if (status === 'loading') return <Boot />
  if (status === 'error') return <Boot error={fatal} />
  return hasWorkspaces ? <AppShell /> : <Welcome />
}
