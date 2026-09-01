import { beforeEach, describe, expect, it, vi } from 'vitest'

const listDir = vi.fn()

vi.mock('@/ipc', () => ({
  ipc: { listDir: (...args: unknown[]) => listDir(...args) },
  errorMessage: (error: unknown) => String(error),
}))

/**
 * The dotfile toggle is remembered in the webview's storage, which a Node test
 * run does not have. Standing one in before the store is imported is what lets
 * "remembered from last launch" be a thing this test can talk about.
 */
const store = new Map<string, string>([['beacon.files.showHidden', 'true']])
vi.stubGlobal('localStorage', {
  getItem: (key: string) => store.get(key) ?? null,
  setItem: (key: string, value: string) => void store.set(key, value),
})

const { useTree, visibleRows, visibleDirectories, nameError, destinationError } = await import('./treeStore')

const WS = 'ws_1'
const PJ = 'pj_1'

const file = (path: string): unknown => ({
  name: path.split('/').pop(),
  path,
  kind: 'file',
  hidden: path.split('/').pop()?.startsWith('.') === true,
})

const dir = (path: string): unknown => ({
  name: path.split('/').pop(),
  path,
  kind: 'directory',
  hidden: false,
})

/** What `listDir` answers, per directory. */
const listing = (byPath: Record<string, unknown[]>): void => {
  listDir.mockImplementation((_ws: string, _pj: string, path: string) => {
    const entries = byPath[path]
    if (!entries) return Promise.reject(new Error(`no such directory: ${path}`))
    return Promise.resolve(entries)
  })
}

const paths = (): string[] => listDir.mock.calls.map((call) => call[2] as string)

beforeEach(() => {
  vi.resetAllMocks()
  useTree.setState({
    entries: {},
    expanded: {},
    loading: {},
    selected: {},
    error: null,
    clipboard: null,
  })
})

describe('reading directories', () => {
  it('reads a folder again when it is opened a second time', async () => {
    listing({ '': [dir('src')], src: [file('src/main.ts')] })
    await useTree.getState().load(WS, PJ, '')

    await useTree.getState().toggle(WS, PJ, 'src')
    await useTree.getState().toggle(WS, PJ, 'src')
    listing({ '': [dir('src')], src: [file('src/main.ts'), file('src/added.ts')] })
    await useTree.getState().toggle(WS, PJ, 'src')

    expect(useTree.getState().entries[`${PJ}:src`]).toHaveLength(2)
  })

  it('reads the project root again after the first attempt failed', async () => {
    // A project folder that was briefly unmounted, and a refresh afterwards.
    listing({})
    await useTree.getState().load(WS, PJ, '')
    expect(useTree.getState().error).not.toBeNull()

    listing({ '': [file('README.md')] })
    await useTree.getState().refreshAll(WS, PJ)

    expect(useTree.getState().entries[`${PJ}:`]).toHaveLength(1)
    expect(useTree.getState().error).toBeNull()
  })

  it('drops a listing that no longer reads instead of leaving its rows up', async () => {
    listing({ '': [dir('gone')], gone: [file('gone/a.txt')] })
    await useTree.getState().load(WS, PJ, '')
    await useTree.getState().toggle(WS, PJ, 'gone')

    listing({ '': [dir('gone')] })
    await useTree.getState().refresh(WS, PJ, 'gone')

    expect(useTree.getState().entries[`${PJ}:gone`]).toBeUndefined()
    expect(useTree.getState().expanded[`${PJ}:gone`]).toBeUndefined()
  })
})

describe('refreshing on focus', () => {
  it('re-reads only the folders that are on screen', async () => {
    listing({
      '': [dir('node_modules'), dir('src')],
      node_modules: [dir('node_modules/pkg')],
      src: [file('src/main.ts')],
    })
    await useTree.getState().load(WS, PJ, '')
    await useTree.getState().toggle(WS, PJ, 'node_modules')
    await useTree.getState().toggle(WS, PJ, 'src')
    // Opened once and put away again: it must not cost anything from here on.
    await useTree.getState().toggle(WS, PJ, 'node_modules')

    listDir.mockClear()
    await useTree.getState().refreshAll(WS, PJ)

    expect(paths()).toEqual(['', 'src'])
  })

  it('leaves a folder hidden inside a collapsed one alone', async () => {
    listing({
      '': [dir('a')],
      a: [dir('a/b')],
      'a/b': [file('a/b/c.ts')],
    })
    await useTree.getState().load(WS, PJ, '')
    await useTree.getState().toggle(WS, PJ, 'a')
    await useTree.getState().toggle(WS, PJ, 'a/b')
    await useTree.getState().toggle(WS, PJ, 'a')

    listDir.mockClear()
    await useTree.getState().refreshAll(WS, PJ)

    expect(paths()).toEqual([''])
  })
})

describe('showing something that was just created', () => {
  it('opens the folder it landed in and selects it', async () => {
    listing({ '': [dir('src')], src: [file('src/new.ts')] })
    await useTree.getState().load(WS, PJ, '')

    // The folder was never expanded: right-click, New file, and the file has
    // to end up somewhere the user can see.
    await useTree.getState().reveal(WS, PJ, 'src/new.ts')

    expect(useTree.getState().expanded[`${PJ}:src`]).toBe(true)
    expect(useTree.getState().entries[`${PJ}:src`]).toHaveLength(1)
    expect(useTree.getState().selected[PJ]).toBe('src/new.ts')
  })

  it('opens every folder above it, not only the last one', async () => {
    listing({ '': [dir('a')], a: [dir('a/b')], 'a/b': [file('a/b/new.ts')] })
    await useTree.getState().load(WS, PJ, '')

    await useTree.getState().reveal(WS, PJ, 'a/b/new.ts')

    expect(useTree.getState().expanded[`${PJ}:a`]).toBe(true)
    expect(useTree.getState().expanded[`${PJ}:a/b`]).toBe(true)
  })
})

describe('the rows the tree draws', () => {
  const view = (
    entries: Record<string, unknown[]>,
    expanded: string[] = [],
    showHidden = false,
  ): Parameters<typeof visibleRows>[0] => ({
    entries: Object.fromEntries(
      Object.entries(entries).map(([path, list]) => [`${PJ}:${path}`, list]),
    ) as never,
    expanded: Object.fromEntries(expanded.map((path) => [`${PJ}:${path}`, true])) as never,
    loading: {},
    showHidden,
  })

  it('lists an open folder followed by what is inside it', () => {
    const rows = visibleRows(
      view({ '': [dir('src'), file('README.md')], src: [file('src/main.ts')] }, ['src']),
      PJ,
    )
    expect(rows.map((row) => (row.type === 'entry' ? row.entry.path : row.note))).toEqual([
      'src',
      'src/main.ts',
      'README.md',
    ])
  })

  it('says an expanded folder is empty rather than showing nothing', () => {
    const rows = visibleRows(view({ '': [dir('empty')], empty: [] }, ['empty']), PJ)
    expect(rows.map((row) => (row.type === 'note' ? row.note : row.entry.path))).toEqual([
      'empty',
      'empty',
    ])
  })

  it('says so when a folder holds nothing but hidden files', () => {
    const rows = visibleRows(view({ '': [file('.env')] }), PJ)
    expect(rows).toEqual([expect.objectContaining({ type: 'note', note: 'hidden' })])
  })

  it('shows dotfiles once they are asked for', () => {
    const listed = view({ '': [file('.env'), file('README.md')] }, [], true)
    expect(visibleRows(listed, PJ)).toHaveLength(2)
  })

  it('offers the root for refreshing even before anything is expanded', () => {
    expect(visibleDirectories(view({ '': [dir('src')] }), PJ)).toEqual([''])
  })
})

describe('naming a file', () => {
  it('refuses a name that is nothing but space', () => {
    expect(nameError('   ')).toBe('Enter a name')
  })

  it('refuses a path where a name belongs', () => {
    expect(nameError('src/thing.ts')).not.toBeNull()
  })

  it('accepts an ordinary name with the space around it trimmed', () => {
    expect(nameError('  thing.ts ')).toBeNull()
  })
})

describe('choosing where something moves to', () => {
  it('takes an empty destination as the project root', () => {
    // Moving something out to the top is an answer, not a missing one.
    expect(destinationError('')).toBeNull()
  })

  it('takes a folder deeper in the project', () => {
    expect(destinationError('src/features')).toBeNull()
  })

  it('refuses a destination that climbs out of the project', () => {
    expect(destinationError('../elsewhere')).not.toBeNull()
    expect(destinationError('src/../..')).not.toBeNull()
  })

  it('forgives the slashes people type around a path', () => {
    expect(destinationError('/src/')).toBeNull()
  })
})

describe('deciding what gets re-read', () => {
  it('re-reads a dotfile folder that is open but filtered out of the display', async () => {
    // Otherwise turning dotfiles back on shows a listing from an hour ago.
    const hiddenDir = { name: '.config', path: '.config', kind: 'directory', hidden: true }
    listing({ '': [hiddenDir], '.config': [file('.config/a')] })
    await useTree.getState().load(WS, PJ, '')
    await useTree.getState().toggle(WS, PJ, '.config')

    expect(visibleDirectories({ ...useTree.getState(), showHidden: true }, PJ)).toContain('.config')
    // It is genuinely off screen while dotfiles are hidden — which is exactly
    // why refreshing has to ask for it anyway.
    expect(visibleDirectories({ ...useTree.getState(), showHidden: false }, PJ)).not.toContain(
      '.config',
    )
  })
})
