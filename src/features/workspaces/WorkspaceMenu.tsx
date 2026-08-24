import { useState } from 'react'

import { MenuHeading, MenuItem, MenuSeparator } from '@/app/ui/Menu'
import { InlineField } from '@/app/ui/InlineField'
import { selectActiveWorkspace, selectWorkspaces, useBeacon } from '@/app/store'
import { ACCENT_PRESETS } from '@/lib/accent'

type Mode = 'list' | 'create' | 'rename' | 'confirm-delete'

/** Switch workspaces, or edit the one you are in. */
export function WorkspaceMenu({ onDone }: { onDone: () => void }): React.ReactElement {
  const [mode, setMode] = useState<Mode>('list')
  const workspaces = useBeacon(selectWorkspaces)
  const active = useBeacon(selectActiveWorkspace)
  const selectWorkspace = useBeacon((s) => s.selectWorkspace)
  const createWorkspace = useBeacon((s) => s.createWorkspace)
  const updateWorkspace = useBeacon((s) => s.updateWorkspace)
  const deleteWorkspace = useBeacon((s) => s.deleteWorkspace)

  if (mode === 'create') {
    return (
      <InlineField
        label="New workspace"
        initialValue=""
        submitLabel="Create"
        onCancel={onDone}
        withAccent={ACCENT_PRESETS[0].value}
        onSubmit={(name, accent) => {
          void createWorkspace(name, accent)
          onDone()
        }}
      />
    )
  }

  if (mode === 'rename' && active) {
    return (
      <InlineField
        label="Workspace"
        initialValue={active.name}
        submitLabel="Save"
        onCancel={onDone}
        withAccent={active.accent}
        onSubmit={(name, accent) => {
          void updateWorkspace(active.id, { name, accent })
          onDone()
        }}
      />
    )
  }

  if (mode === 'confirm-delete' && active) {
    return (
      <>
        <MenuHeading>Delete “{active.name}”?</MenuHeading>
        <MenuItem
          label="Delete workspace"
          hint="Project files are kept"
          danger
          onSelect={() => {
            void deleteWorkspace(active.id)
            onDone()
          }}
        />
        <MenuItem label="Cancel" onSelect={() => setMode('list')} />
      </>
    )
  }

  return (
    <>
      <MenuHeading>Workspaces</MenuHeading>
      {workspaces.map((workspace) => (
        <MenuItem
          key={workspace.id}
          label={workspace.name}
          dot={workspace.accent}
          active={workspace.id === active?.id}
          hint={workspace.projects.length > 0 ? String(workspace.projects.length) : undefined}
          onSelect={() => {
            void selectWorkspace(workspace.id)
            onDone()
          }}
        />
      ))}

      <MenuSeparator />
      <MenuItem label="New workspace…" onSelect={() => setMode('create')} />
      {active ? (
        <>
          <MenuItem label="Edit workspace…" onSelect={() => setMode('rename')} />
          <MenuItem label="Delete workspace…" onSelect={() => setMode('confirm-delete')} />
        </>
      ) : null}
    </>
  )
}
