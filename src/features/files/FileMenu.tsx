import { useState } from 'react'

import { MenuHeading, MenuItem, MenuSeparator } from '@/app/ui/Menu'
import { selectProjects, useBeacon } from '@/app/store'
import { useEditor } from '@/features/editor/openFiles'
import { errorMessage, ipc } from '@/ipc'
import { isMac } from '@/lib/platform'
import type { DirEntry } from '@/types/beacon'
import { InlineField } from '@/app/ui/InlineField'
import { destinationError, joinPath, nameError, parentOf, useTree } from './treeStore'

/** The items a keyboard shortcut can ask for without opening the whole menu. */
export type MenuPrompt = 'rename' | 'confirm-trash'

type Prompt = MenuPrompt | 'none' | 'new-file' | 'new-folder' | 'move'

interface FileMenuProps {
  workspaceId: string
  projectId: string
  /** The entry the menu was opened on, or `null` for the project root. */
  entry: DirEntry | null
  /** Which prompt to open on, for F2 and Delete. */
  initialPrompt?: MenuPrompt | undefined
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
  initialPrompt,
  onDone,
}: FileMenuProps): React.ReactElement {
  const [prompt, setPrompt] = useState<Prompt>(entry ? (initialPrompt ?? 'none') : 'none')
  const refresh = useTree((s) => s.refresh)
  const reveal = useTree((s) => s.reveal)
  const setError = useTree((s) => s.setError)
  const clipboard = useTree((s) => s.clipboard)
  const setClipboard = useTree((s) => s.setClipboard)
  const closeFile = useEditor((s) => s.close)
  const renameOpenFile = useEditor((s) => s.rename)
  const openFile = useEditor((s) => s.open)
  const showPanel = useBeacon((s) => s.showPanel)
  const project = useBeacon((s) => selectProjects(s).find((p) => p.id === projectId))

  const path = entry?.path ?? ''
  const isDirectory = entry === null || entry.kind === 'directory'
  /** Where new things go: inside a folder, or beside a file. */
  const container = isDirectory ? path : parentOf(path)

  /**
   * Runs a menu action and then puts the result on screen.
   *
   * A path handed back is something that now exists and nobody has seen yet —
   * a new file, a duplicate, a paste. Revealing it opens whatever it landed in
   * and selects it, because a creation that leaves the panel looking untouched
   * reads as a creation that did not happen.
   */
  const run = async (
    action: () => Promise<string | void>,
    ...reload: string[]
  ): Promise<void> => {
    try {
      const created = await action()
      if (typeof created === 'string') await reveal(workspaceId, projectId, created)
      else for (const dir of new Set(reload)) await refresh(workspaceId, projectId, dir)
      setError(null)
    } catch (error) {
      setError(errorMessage(error))
    }
    onDone()
  }

  if (prompt === 'rename' && entry) {
    return (
      <InlineField
        validate={nameError}
        label="Rename"
        initialValue={entry.name}
        submitLabel="Rename"
        onCancel={onDone}
        onSubmit={(name) => {
          const target = joinPath(parentOf(path), name)
          void run(async () => {
            await ipc.renamePath(workspaceId, projectId, path, target)
            renameOpenFile(projectId, path, target)
            return target
          })
        }}
      />
    )
  }

  if (prompt === 'move' && entry) {
    return (
      <InlineField
        validate={destinationError}
        label={`Move “${entry.name}” into`}
        initialValue={parentOf(path)}
        submitLabel="Move"
        onCancel={onDone}
        onSubmit={(folder) => {
          const target = joinPath(folder.replace(/^\/+|\/+$/g, ''), entry.name)
          if (target === path) {
            onDone()
            return
          }
          void run(async () => {
            await ipc.renamePath(workspaceId, projectId, path, target)
            renameOpenFile(projectId, path, target)
            return target
          }, parentOf(path))
        }}
      />
    )
  }

  if (prompt === 'new-file' || prompt === 'new-folder') {
    const isFolder = prompt === 'new-folder'
    return (
      <InlineField
        validate={nameError}
        label={isFolder ? 'New folder' : 'New file'}
        initialValue=""
        submitLabel="Create"
        onCancel={onDone}
        onSubmit={(name) => {
          const target = joinPath(container, name)
          void run(async () => {
            if (isFolder) {
              await ipc.createDir(workspaceId, projectId, target)
            } else {
              await ipc.createFile(workspaceId, projectId, target)
              // A new file is one you are about to write in.
              await openFile(workspaceId, projectId, target)
              void showPanel('editor')
            }
            return target
          })
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
  /** What Copy path hands over: what every other tool will accept. */
  const absolutePath = project ? joinAbsolute(project.absolutePath, path) : path

  return (
    <>
      <MenuHeading>{entry?.name ?? 'Project root'}</MenuHeading>

      <MenuItem label="New file…" onSelect={() => setPrompt('new-file')} />
      <MenuItem label="New folder…" onSelect={() => setPrompt('new-folder')} />

      {entry ? (
        <>
          <MenuSeparator />
          <MenuItem label="Rename…" onSelect={() => setPrompt('rename')} />
          <MenuItem label="Move to…" hint="Folder" onSelect={() => setPrompt('move')} />
          <MenuItem
            label="Duplicate"
            onSelect={() => {
              void run(() => ipc.duplicatePath(workspaceId, projectId, path))
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
            void run(() => ipc.copyInto(workspaceId, projectId, clipboard.path, container))
          }}
        />
      ) : null}

      <MenuSeparator />
      <MenuItem
        label="Copy path"
        onSelect={() => {
          void navigator.clipboard.writeText(absolutePath)
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

/** The project root itself when the menu was opened on empty space. */
function joinAbsolute(root: string, relative: string): string {
  return relative ? `${root}/${relative}` : root
}
