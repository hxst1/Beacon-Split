import { beforeEach, describe, expect, it, vi } from 'vitest'

const readFile = vi.fn()
const writeFile = vi.fn()
const fileRevision = vi.fn()

vi.mock('@/ipc', () => ({
  ipc: {
    readFile: (...args: unknown[]) => readFile(...args),
    writeFile: (...args: unknown[]) => writeFile(...args),
    fileRevision: (...args: unknown[]) => fileRevision(...args),
  },
  errorMessage: (error: unknown) => String(error),
}))

const { useEditor, isDirty, unsavedIn } = await import('./openFiles')

const WS = 'ws_1'
const PJ = 'pj_1'

/**
 * Writes are queued per file in module state, so each test uses its own file:
 * a save left in flight by a failing assertion would otherwise stall every test
 * after it behind a promise that never settles.
 */
let counter = 0
let path = ''

beforeEach(() => {
  vi.resetAllMocks()
  useEditor.setState({ byProject: {}, active: {}, error: null })
  counter += 1
  path = `file-${counter}.txt`
})

/** Opens the test's file holding `text`, at revision 1. */
async function openFile(text: string): Promise<void> {
  readFile.mockResolvedValue({ kind: 'text', text, revision: 1 })
  await useEditor.getState().open(WS, PJ, path)
}

const fileNow = (at = path): ReturnType<typeof unsavedIn>[number] | undefined =>
  useEditor.getState().byProject[PJ]?.find((file) => file.path === at)

/** Lets queued promises run without letting a hung one hold up the test. */
const settle = (): Promise<void> =>
  new Promise((resolve) => {
    setTimeout(resolve, 0)
  })

describe('saving what is on screen', () => {
  it('keeps the tab dirty when the buffer moved while the save was in flight', async () => {
    await openFile('AB')

    let finishWrite: (outcome: unknown) => void = () => {}
    writeFile.mockReturnValue(
      new Promise((resolve) => {
        finishWrite = resolve
      }),
    )

    // Cmd+S with "ABC" on screen, and the user carries on typing during the write.
    useEditor.getState().edit(PJ, path, 'ABC')
    const saving = useEditor.getState().save(WS, PJ, path)
    await settle()
    useEditor.getState().edit(PJ, path, 'ABCD')

    finishWrite({ outcome: 'written', revision: 2 })
    await saving

    // "ABCD" is on screen but only "ABC" reached disk: the tab must still say so.
    expect(writeFile).toHaveBeenLastCalledWith(WS, PJ, path, 'ABC', 1)
    expect(isDirty(PJ, path)).toBe(true)
  })

  it('reports the tab clean when the buffer is what was written', async () => {
    await openFile('AB')
    writeFile.mockResolvedValue({ outcome: 'written', revision: 2 })

    useEditor.getState().edit(PJ, path, 'ABC')
    expect(isDirty(PJ, path)).toBe(true)

    await useEditor.getState().save(WS, PJ, path)
    expect(isDirty(PJ, path)).toBe(false)
  })

  it('checks a second save against what the first one left behind', async () => {
    await openFile('AB')

    writeFile.mockResolvedValue({ outcome: 'written', revision: 2 })
    useEditor.getState().edit(PJ, path, 'ABC')
    await useEditor.getState().save(WS, PJ, path)

    writeFile.mockResolvedValue({ outcome: 'written', revision: 3 })
    useEditor.getState().edit(PJ, path, 'ABCD')
    await useEditor.getState().save(WS, PJ, path)

    expect(writeFile).toHaveBeenLastCalledWith(WS, PJ, path, 'ABCD', 2)
    expect(useEditor.getState().error).toBeNull()
  })

  it('does not let two saves of one file overtake each other', async () => {
    await openFile('AB')

    const finishers: Array<(outcome: unknown) => void> = []
    writeFile.mockImplementation(
      () =>
        new Promise((resolve) => {
          finishers.push(resolve)
        }),
    )

    // Two Cmd+S in quick succession. Sent together, both would be checked
    // against the revision from before either, so the second would be refused
    // as a conflict with the first — and the text last on screen is the text
    // that never reached the disk.
    useEditor.getState().edit(PJ, path, 'ABC')
    const first = useEditor.getState().save(WS, PJ, path)
    const second = useEditor.getState().save(WS, PJ, path)
    await settle()

    expect(writeFile).toHaveBeenCalledTimes(1)
    finishers[0]?.({ outcome: 'written', revision: 2 })
    await first
    await settle()

    // Only now does the second go out, against the revision the first left.
    expect(writeFile).toHaveBeenCalledTimes(2)
    expect(writeFile).toHaveBeenLastCalledWith(WS, PJ, path, 'ABC', 2)
    finishers[1]?.({ outcome: 'written', revision: 3 })
    await second
  })

  it('says so rather than overwriting when the file moved on disk', async () => {
    await openFile('AB')
    writeFile.mockResolvedValue({ outcome: 'stale' })

    useEditor.getState().edit(PJ, path, 'ABC')
    await useEditor.getState().save(WS, PJ, path)

    expect(fileNow()?.changedOnDisk).toBe(true)
    expect(useEditor.getState().error).toMatch(/changed on disk/)
    expect(fileNow()?.draft).toBe('ABC')
  })

  it('does not silently overwrite when there is no revision to check against', async () => {
    // A filesystem that cannot report a modification time leaves the file with
    // no revision to check a save against.
    readFile.mockResolvedValue({ kind: 'text', text: 'AB' })
    await useEditor.getState().open(WS, PJ, path)
    writeFile.mockResolvedValue({ outcome: 'written', revision: 2 })

    useEditor.getState().edit(PJ, path, 'ABC')
    await useEditor.getState().save(WS, PJ, path)

    // A guarded save with nothing to guard against must not quietly become a
    // blind overwrite: that is the one thing `overwrite` exists to ask for.
    expect(writeFile).not.toHaveBeenCalled()
    expect(useEditor.getState().error).toMatch(/cannot tell/)

    // Having been told, the user can still insist.
    await useEditor.getState().overwrite(WS, PJ, path)
    expect(writeFile).toHaveBeenLastCalledWith(WS, PJ, path, 'ABC', null)
  })

  it('shows the saved text after a save, not the text the file was opened with', async () => {
    await openFile('AB')
    writeFile.mockResolvedValue({ outcome: 'written', revision: 2 })

    useEditor.getState().edit(PJ, path, 'ABC')
    await useEditor.getState().save(WS, PJ, path)

    // `contents` seeds the editor whenever it is rebuilt. Left at the text the
    // file was opened with, a rebuild puts back what the user just replaced and
    // a successful save looks like one that never happened.
    expect(fileNow()?.contents).toEqual({ kind: 'text', text: 'ABC' })
  })

  it('does not rebuild the editor on a save, but does on a reload', async () => {
    await openFile('AB')
    const epochAtOpen = fileNow()?.epoch

    writeFile.mockResolvedValue({ outcome: 'written', revision: 2 })
    useEditor.getState().edit(PJ, path, 'ABC')
    await useEditor.getState().save(WS, PJ, path)

    // EditorPane keys the editor on the epoch. Bumping it on a save would throw
    // away the undo history, the cursor and the scroll position on every Cmd+S.
    expect(fileNow()?.epoch).toBe(epochAtOpen)

    readFile.mockResolvedValue({ kind: 'text', text: 'theirs', revision: 3 })
    await useEditor.getState().reload(WS, PJ, path)
    expect(fileNow()?.epoch).not.toBe(epochAtOpen)
  })
})

describe('keeping a draft alive', () => {
  it('remembers what was typed, so switching tabs does not lose it', async () => {
    await openFile('AB')
    useEditor.getState().edit(PJ, path, 'ABC')

    // The editor is unmounted whenever you look at another tab, hide the panel
    // or switch project. The text has to outlive the view that showed it.
    expect(fileNow()?.draft).toBe('ABC')
    expect(unsavedIn(PJ).map((file) => file.path)).toEqual([path])
  })

  it('takes what is on disk when a clean file changed underneath it', async () => {
    await openFile('AB')
    fileRevision.mockResolvedValue(2)
    readFile.mockResolvedValue({ kind: 'text', text: 'theirs', revision: 2 })

    await useEditor.getState().checkForChanges(WS, PJ)

    expect(fileNow()?.draft).toBe('theirs')
    expect(fileNow()?.changedOnDisk).toBeUndefined()
  })

  it('asks rather than reloading when the file changed under unsaved work', async () => {
    await openFile('AB')
    useEditor.getState().edit(PJ, path, 'mine')
    fileRevision.mockResolvedValue(2)

    await useEditor.getState().checkForChanges(WS, PJ)

    expect(fileNow()?.changedOnDisk).toBe(true)
    expect(fileNow()?.draft).toBe('mine')
    expect(readFile).toHaveBeenCalledTimes(1)
  })

  it('keeps the buffer when the file is deleted underneath it', async () => {
    await openFile('AB')
    fileRevision.mockResolvedValue(null)

    await useEditor.getState().checkForChanges(WS, PJ)

    // Reloading a file that is gone would replace a perfectly good buffer with
    // an error, and the buffer is the only copy left.
    expect(fileNow()?.goneFromDisk).toBe(true)
    expect(fileNow()?.draft).toBe('AB')
  })
})

describe('following what happens to files', () => {
  it('follows a renamed folder, so the tabs inside it stay pointed at the file', async () => {
    readFile.mockResolvedValue({ kind: 'text', text: 'x', revision: 1 })
    await useEditor.getState().open(WS, PJ, 'src/a.ts')
    await useEditor.getState().open(WS, PJ, 'src/deep/b.ts')

    useEditor.getState().rename(PJ, 'src', 'app')

    expect(useEditor.getState().byProject[PJ]?.map((file) => file.path)).toEqual([
      'app/a.ts',
      'app/deep/b.ts',
    ])
    expect(useEditor.getState().active[PJ]).toBe('app/deep/b.ts')
  })

  it('closes the tabs inside a folder that was trashed', async () => {
    readFile.mockResolvedValue({ kind: 'text', text: 'x', revision: 1 })
    await useEditor.getState().open(WS, PJ, 'src/a.ts')
    await useEditor.getState().open(WS, PJ, 'keep.ts')

    useEditor.getState().close(PJ, 'src')

    expect(useEditor.getState().byProject[PJ]?.map((file) => file.path)).toEqual(['keep.ts'])
  })

  it('lands on the neighbouring tab when the one you were on is closed', async () => {
    readFile.mockResolvedValue({ kind: 'text', text: 'x', revision: 1 })
    await useEditor.getState().open(WS, PJ, 'a.ts')
    await useEditor.getState().open(WS, PJ, 'b.ts')
    await useEditor.getState().open(WS, PJ, 'c.ts')
    useEditor.getState().activate(PJ, 'b.ts')

    useEditor.getState().close(PJ, 'b.ts')

    expect(useEditor.getState().active[PJ]).toBe('c.ts')
  })
})
