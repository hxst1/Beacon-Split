import { useState } from 'react'

import { MenuHeading, MenuItem, MenuSeparator } from '@/app/ui/Menu'
import { InlineField } from '@/app/ui/InlineField'
import { selectActiveWorkspace, useBeacon } from '@/app/store'
import { isMac } from '@/lib/platform'
import type { Project } from '@/types/beacon'

interface ProjectMenuProps {
  project: Project
  onDone: () => void
}

/**
 * Right-click actions for a project tab.
 *
 * "Remove" is the only one that sounds destructive and is deliberately not:
 * Beacon never deletes a repository. The hint says so at the point of decision
 * rather than in a confirmation dialog.
 */
export function ProjectMenu({ project, onDone }: ProjectMenuProps): React.ReactElement {
  const [renaming, setRenaming] = useState(false)
  const workspaces = useBeacon((s) => s.snapshot?.workspaces ?? [])
  const activeWorkspace = useBeacon(selectActiveWorkspace)
  const renameProject = useBeacon((s) => s.renameProject)
  const removeProject = useBeacon((s) => s.removeProject)
  const moveProject = useBeacon((s) => s.moveProject)
  const revealProject = useBeacon((s) => s.revealProject)
  const stopProject = useBeacon((s) => s.stopProject)

  if (renaming) {
    return (
      <InlineField
        label="Rename project"
        initialValue={project.name}
        submitLabel="Rename"
        onCancel={onDone}
        onSubmit={(name) => {
          void renameProject(project.id, name)
          onDone()
        }}
      />
    )
  }

  const otherWorkspaces = workspaces.filter((w) => w.id !== activeWorkspace?.id)

  return (
    <>
      <MenuHeading>{project.displayPath}</MenuHeading>

      <MenuItem label="Rename…" onSelect={() => setRenaming(true)} />
      <MenuItem
        label={isMac() ? 'Reveal in Finder' : 'Open in file manager'}
        onSelect={() => {
          void revealProject(project.id)
          onDone()
        }}
      />
      <MenuItem
        label="Copy path"
        onSelect={() => {
          void navigator.clipboard.writeText(project.absolutePath)
          onDone()
        }}
      />

      {otherWorkspaces.length > 0 ? (
        <>
          <MenuSeparator />
          <MenuHeading>Move to</MenuHeading>
          {otherWorkspaces.map((workspace) => (
            <MenuItem
              key={workspace.id}
              label={workspace.name}
              dot={workspace.accent}
              onSelect={() => {
                void moveProject(project.id, workspace.id)
                onDone()
              }}
            />
          ))}
        </>
      ) : null}

      <MenuSeparator />
      <MenuItem
        label="Stop processes"
        hint="Terminal, Claude"
        onSelect={() => {
          void stopProject(project.id)
          onDone()
        }}
      />
      <MenuItem
        label="Remove from workspace"
        hint="Files are kept"
        onSelect={() => {
          void removeProject(project.id)
          onDone()
        }}
      />
    </>
  )
}
