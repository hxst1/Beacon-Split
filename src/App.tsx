import { useEffect } from 'react'

import { AppShell } from '@/app/AppShell'
import { Boot } from '@/app/Boot'
import { Welcome } from '@/features/workspaces/Welcome'
import { useBeacon } from '@/app/store'

export function App(): React.ReactElement {
  const status = useBeacon((s) => s.status)
  const fatal = useBeacon((s) => s.fatal)
  const hasWorkspaces = useBeacon((s) => (s.snapshot?.workspaces.length ?? 0) > 0)
  const load = useBeacon((s) => s.load)

  useEffect(() => {
    void load()
  }, [load])

  if (status === 'loading') return <Boot />
  if (status === 'error') return <Boot error={fatal} />
  return hasWorkspaces ? <AppShell /> : <Welcome />
}
