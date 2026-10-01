import { describe, expect, it } from 'vitest'

import { fileKind } from './fileKind'

describe('fileKind', () => {
  it('shows a folder as a folder, and says whether it is open', () => {
    expect(fileKind('src', true)).toBe('folder')
    expect(fileKind('src', true, true)).toBe('folderOpen')
    // A directory is a directory whatever it is called — `assets.d` is not a
    // `.d` file.
    expect(fileKind('assets.d', true)).toBe('folder')
  })

  it('knows the files somebody recognises at a glance', () => {
    expect(fileKind('package.json', false)).toBe('package')
    expect(fileKind('Cargo.toml', false)).toBe('package')
    expect(fileKind('Dockerfile', false)).toBe('container')
    expect(fileKind('Makefile', false)).toBe('shell')
    expect(fileKind('.gitignore', false)).toBe('git')
  })

  it('tells a lock file from the thing it locks', () => {
    // The pair that proves the whole-name table earns its keep: these two only
    // differ by what they are called.
    expect(fileKind('package.json', false)).toBe('package')
    expect(fileKind('package-lock.json', false)).toBe('lock')
    expect(fileKind('Cargo.toml', false)).toBe('package')
    expect(fileKind('Cargo.lock', false)).toBe('lock')
    expect(fileKind('pnpm-lock.yaml', false)).toBe('lock')
  })

  it('reads README and LICENSE however they are spelled', () => {
    for (const name of ['README', 'README.md', 'readme.txt', 'Readme']) {
      expect(fileKind(name, false)).toBe('readme')
    }
    for (const name of ['LICENSE', 'LICENCE', 'license.md', 'COPYING']) {
      expect(fileKind(name, false)).toBe('license')
    }
  })

  it('sorts the ordinary endings', () => {
    const expected: Record<string, string> = {
      'main.rs': 'code',
      'App.tsx': 'code',
      'build.sh': 'shell',
      'tsconfig.json': 'json',
      'vercel.toml': 'config',
      'styles.css': 'styles',
      'index.html': 'markup',
      'notes.md': 'text',
      'icon.svg': 'image',
      'demo.mp4': 'media',
      'bundle.tar.gz': 'archive',
      'cache.sqlite': 'database',
    }
    for (const [name, icon] of Object.entries(expected)) {
      expect(fileKind(name, false), name).toBe(icon)
    }
  })

  it('does not care about case', () => {
    expect(fileKind('MAIN.RS', false)).toBe('code')
    expect(fileKind('Photo.JPEG', false)).toBe('image')
  })

  it('reads the ending after the last dot', () => {
    // `.env.local` is configuration, not a `local` file; the first dot belongs
    // to the name.
    expect(fileKind('.env.local', false)).toBe('config')
    expect(fileKind('vite.config.ts', false)).toBe('code')
    expect(fileKind('archive.tar.gz', false)).toBe('archive')
  })

  it('treats a bare dotfile as configuration', () => {
    expect(fileKind('.prettierrc', false)).toBe('config')
    expect(fileKind('.somethingnobodyhasheardof', false)).toBe('config')
  })

  it('gives anything unknown a plain file, never nothing', () => {
    // The fallback is the one that has to be right: most repositories have
    // something in them nobody thought of, and a row with no icon reads as a
    // row that failed to load.
    expect(fileKind('mystery', false)).toBe('file')
    expect(fileKind('data.xyzzy', false)).toBe('file')
    expect(fileKind('', false)).toBe('file')
  })
})
