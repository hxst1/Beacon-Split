/**
 * Which icon a row in the file tree gets.
 *
 * Kept apart from the drawing, and pure, because the interesting part is the
 * recognition rather than the rendering: which of a few hundred names and
 * endings means what. That is a table, and a table is a thing to test.
 *
 * Shapes carry the meaning, not colours. The tree is a quiet list somebody
 * scans while doing something else, and a column of coloured badges would
 * compete with the thing they are actually reading. The icon says what kind of
 * thing this is; the name says which one.
 */

export type FileKind =
  | 'folder'
  | 'folderOpen'
  | 'file'
  | 'code'
  | 'json'
  | 'text'
  | 'readme'
  | 'license'
  | 'image'
  | 'media'
  | 'archive'
  | 'config'
  | 'lock'
  | 'git'
  | 'database'
  | 'shell'
  | 'styles'
  | 'markup'
  | 'package'
  | 'container'

/**
 * Whole names, matched before anything else and without their case.
 *
 * These are the files somebody recognises at a glance in a repository, and
 * getting them from their ending alone would be wrong: `package.json` is not
 * just some JSON, and a `Dockerfile` has no ending at all.
 */
const BY_NAME: Record<string, FileKind> = {
  'package.json': 'package',
  'cargo.toml': 'package',
  'pyproject.toml': 'package',
  'go.mod': 'package',
  'go.sum': 'lock',
  gemfile: 'package',
  'pom.xml': 'package',
  'build.gradle': 'package',
  'composer.json': 'package',
  'package-lock.json': 'lock',
  'pnpm-lock.yaml': 'lock',
  'yarn.lock': 'lock',
  'cargo.lock': 'lock',
  'bun.lockb': 'lock',
  'poetry.lock': 'lock',
  'composer.lock': 'lock',
  dockerfile: 'container',
  'docker-compose.yml': 'container',
  'docker-compose.yaml': 'container',
  '.dockerignore': 'container',
  makefile: 'shell',
  justfile: 'shell',
  '.gitignore': 'git',
  '.gitattributes': 'git',
  '.gitmodules': 'git',
  '.mailmap': 'git',
  '.editorconfig': 'config',
  '.npmrc': 'config',
  '.nvmrc': 'config',
  '.prettierrc': 'config',
}

/** Endings, lower-cased, without the dot. */
const BY_EXTENSION: Record<string, FileKind> = {
  // Things that are programs.
  ts: 'code',
  tsx: 'code',
  js: 'code',
  jsx: 'code',
  mjs: 'code',
  cjs: 'code',
  rs: 'code',
  py: 'code',
  go: 'code',
  rb: 'code',
  java: 'code',
  kt: 'code',
  swift: 'code',
  c: 'code',
  cc: 'code',
  cpp: 'code',
  h: 'code',
  hpp: 'code',
  cs: 'code',
  php: 'code',
  lua: 'code',
  dart: 'code',
  ex: 'code',
  exs: 'code',
  zig: 'code',
  scala: 'code',

  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  fish: 'shell',
  ps1: 'shell',

  json: 'json',
  jsonc: 'json',
  json5: 'json',

  toml: 'config',
  yaml: 'config',
  yml: 'config',
  ini: 'config',
  conf: 'config',
  cfg: 'config',
  env: 'config',
  properties: 'config',

  css: 'styles',
  scss: 'styles',
  sass: 'styles',
  less: 'styles',
  styl: 'styles',

  html: 'markup',
  htm: 'markup',
  xml: 'markup',
  vue: 'markup',
  svelte: 'markup',
  astro: 'markup',

  md: 'text',
  mdx: 'text',
  txt: 'text',
  rst: 'text',
  adoc: 'text',
  pdf: 'text',

  png: 'image',
  jpg: 'image',
  jpeg: 'image',
  gif: 'image',
  webp: 'image',
  avif: 'image',
  ico: 'image',
  bmp: 'image',
  svg: 'image',

  mp4: 'media',
  mov: 'media',
  avi: 'media',
  mkv: 'media',
  webm: 'media',
  mp3: 'media',
  wav: 'media',
  flac: 'media',
  ogg: 'media',

  zip: 'archive',
  tar: 'archive',
  gz: 'archive',
  tgz: 'archive',
  bz2: 'archive',
  xz: 'archive',
  '7z': 'archive',
  rar: 'archive',

  sqlite: 'database',
  sqlite3: 'database',
  db: 'database',
  sql: 'database',

  lock: 'lock',
}

/**
 * The icon for one row.
 *
 * Specific before general, which is the whole of the rule: a whole name beats
 * an ending, an ending beats nothing, and nothing is a plain file rather than
 * a guess. Anything unrecognised has to look like a file — it *is* one — so
 * the fallback is never an absence.
 */
export function fileKind(name: string, isDirectory: boolean, expanded = false): FileKind {
  if (isDirectory) return expanded ? 'folderOpen' : 'folder'

  const lower = name.toLowerCase()

  // README and LICENSE come in too many spellings to list: `README`,
  // `README.md`, `readme.txt`, `LICENCE`, `COPYING`.
  if (lower.startsWith('readme')) return 'readme'
  if (lower.startsWith('license') || lower.startsWith('licence') || lower === 'copying') {
    return 'license'
  }

  const named = BY_NAME[lower]
  if (named) return named

  // A leading dot belongs to the name rather than to an ending, and what
  // follows it is what the file is: `.env.local` is configuration, not a
  // `local` file. Anything dotted that is still unrecognised is configuration
  // too — that is what a dotfile is.
  if (lower.startsWith('.')) {
    const first = lower.slice(1).split('.')[0] ?? ''
    return BY_EXTENSION[first] ?? 'config'
  }

  // Otherwise the ending is what follows the last dot, so `archive.tar.gz` is
  // an archive and `vite.config.ts` is code.
  const dot = lower.lastIndexOf('.')
  if (dot < 0) return 'file'

  return BY_EXTENSION[lower.slice(dot + 1)] ?? 'file'
}
