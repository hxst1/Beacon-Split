import { useState } from 'react'

import { MenuHeading, MenuItem, MenuSeparator } from '@/app/ui/Menu'
import { InlineField } from '@/app/ui/InlineField'
import { useEditor } from '@/features/editor/openFiles'
import { errorMessage, ipc } from '@/ipc'
import { isMac } from '@/lib/platform'
import type { DirEntry } from '@/types/beacon'
import { joinPath, parentOf, useTree } from './treeStore'

type Prompt = 'none' | 'rename' | 'new-file' | 'new-folder' | 'confirm-trash'

interface FileMenuProps {
  workspaceId: string
  projectId: string
  /** The entry the menu was opened on, or `null` for the project root. */
  entry: DirEntry | null
  onDone: () => void
}

/**
 * Everything you can do to a file or folder.
 *
 * The only destructive item moves to the trash, is separated from the rest, and
 * asks first — see `docs/DECISIONS.md`, ADR-019.
 */
export function FileMenu({
  workspaceId,
  projectId,
  entry,
  onDone,
}: FileMenuProps): React.ReactElement {
  const [prompt, setPrompt] = useState<Prompt>('none')
  const refresh = useTree((s) => s.refresh)
  const setError = useTree((s) => s.setError)
  const clipboard = useTree((s) => s.clipboard)
  const setClipboard = useTree((s) => s.setClipboard)
  const closeFile = useEditor((s) => s.close)
  const renameOpenFile = useEditor((s) => s.rename)

  const path = entry?.path ?? ''
  const isDirectory = entry === null || entry.kind === 'directory'
  /** Where new things go: inside a folder, or beside a file. */
  const container = isDirectory ? path : parentOf(path)

  const run = async (action: () => Promise<void>, ...reload: string[]): Promise<void> => {
    try {
      await action()
      for (const dir of new Set(reload)) await refresh(workspaceId, projectId, dir)
      setError(null)
    } catch (error) {
      setError(errorMessage(error))
    }
    onDone()
  }

  if (prompt === 'rename' && entry) {
    return (
      <InlineField
        label="Rename"
        initialValue={entry.name}
        submitLabel="Rename"
        onCancel={onDone}
        onSubmit={(name) => {
          const target = joinPath(parentOf(path), name)
          void run(async () => {
            await ipc.renamePath(workspaceId, projectId, path, target)
            renameOpenFile(projectId, path, target)
          }, parentOf(path))
        }}
      />
    )
  }

  if (prompt === 'new-file' || prompt === 'new-folder') {
    const isFolder = prompt === 'new-folder'
    return (
      <InlineField
        label={isFolder ? 'New folder' : 'New file'}
        initialValue=""
        submitLabel="Create"
        onCancel={onDone}
        onSubmit={(name) => {
          const target = joinPath(container, name)
          void run(
            () =>
              isFolder
                ? ipc.createDir(workspaceId, projectId, target)
                : ipc.createFile(workspaceId, projectId, target),
            container,
          )
        }}
      />
    )
  }

  if (prompt === 'confirm-trash' && entry) {
    return (
      <>
        <MenuHeading>Move “{entry.name}” to the trash?</MenuHeading>
        <MenuItem
          label="Move to trash"
          hint="Recoverable"
          danger
          onSelect={() => {
            void run(async () => {
              await ipc.trashPath(workspaceId, projectId, path)
              closeFile(projectId, path)
            }, parentOf(path))
          }}
        />
        <MenuItem label="Cancel" onSelect={() => setPrompt('none')} />
      </>
    )
  }

  const canPaste = clipboard !== null && clipboard.projectId === projectId

  return (
    <>
      <MenuHeading>{entry?.name ?? 'Project root'}</MenuHeading>

      <MenuItem label="New file…" onSelect={() => setPrompt('new-file')} />
      <MenuItem label="New folder…" onSelect={() => setPrompt('new-folder')} />

      {entry ? (
        <>
          <MenuSeparator />
          <MenuItem label="Rename…" onSelect={() => setPrompt('rename')} />
          <MenuItem
            label="Duplicate"
            onSelect={() => {
              void run(
                () => ipc.duplicatePath(workspaceId, projectId, path).then(() => undefined),
                parentOf(path),
              )
            }}
          />
          <MenuItem
            label="Copy"
            onSelect={() => {
              setClipboard(projectId, path)
              onDone()
            }}
          />
        </>
      ) : null}

      {canPaste ? (
        <MenuItem
          label="Paste"
          hint={clipboard.path.split('/').pop()}
          onSelect={() => {
            void run(
              () =>
                ipc.copyInto(workspaceId, projectId, clipboard.path, container).then(() => undefined),
              container,
            )
          }}
        />
      ) : null}

      <MenuSeparator />
      <MenuItem
        label="Copy path"
        onSelect={() => {
          void navigator.clipboard.writeText(path)
          onDone()
        }}
      />
      <MenuItem
        label={isMac() ? 'Reveal in Finder' : 'Show in file manager'}
        onSelect={() => {
          void run(() => ipc.revealPath(workspaceId, projectId, path))
        }}
      />
      {entry && entry.kind === 'file' ? (
        <MenuItem
          label="Copy contents"
          onSelect={() => {
            void run(async () => {
              const contents = await ipc.readFile(workspaceId, projectId, path)
              if (contents.kind === 'text') await navigator.clipboard.writeText(contents.text)
              else throw new Error(`${entry.name} is not a text file`)
            })
          }}
        />
      ) : null}

      {entry ? (
        <>
          <MenuSeparator />
          <MenuItem label="Move to trash…" danger onSelect={() => setPrompt('confirm-trash')} />
        </>
      ) : null}
    </>
  )
}
