import { StreamLanguage, type StreamParser } from '@codemirror/language'
import type { Extension } from '@codemirror/state'

/**
 * Loads a language mode for a filename, on demand.
 *
 * Dynamic imports rather than a static table: the grammars are the largest part
 * of the editor, and someone who only opens Markdown should never pay for the
 * Rust parser.
 */
export async function languageFor(filename: string): Promise<Extension | null> {
  const name = filename.toLowerCase()
  const extension = name.includes('.') ? (name.split('.').pop() ?? '') : ''

  // Files that carry their type in the name rather than in an extension. These
  // come first: `Dockerfile.dev` and `Cargo.lock` both have an extension, and
  // neither of them means what it looks like.
  const byName = await languageForName(name)
  if (byName) return byName

  switch (extension) {
    case 'ts':
    case 'tsx':
    case 'mts':
    case 'cts':
      return (await import('@codemirror/lang-javascript')).javascript({
        typescript: true,
        jsx: extension === 'tsx',
      })
    case 'js':
    case 'jsx':
    case 'mjs':
    case 'cjs':
      return (await import('@codemirror/lang-javascript')).javascript({ jsx: extension === 'jsx' })
    case 'json':
    case 'jsonc':
      return (await import('@codemirror/lang-json')).json()
    case 'css':
    case 'scss':
    case 'less':
      return (await import('@codemirror/lang-css')).css()
    case 'html':
    case 'htm':
    case 'vue':
    case 'svelte':
      return (await import('@codemirror/lang-html')).html()
    case 'md':
    case 'markdown':
    case 'mdx':
      return (await import('@codemirror/lang-markdown')).markdown()
    case 'rs':
      return (await import('@codemirror/lang-rust')).rust()
    case 'py':
    case 'pyi':
      return (await import('@codemirror/lang-python')).python()

    // Everything below is a stream mode: less precise than a real grammar, but
    // these are configuration and scripts, where telling a comment from a
    // string is most of what highlighting is for.
    case 'toml':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/toml')).toml)
    case 'yaml':
    case 'yml':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/yaml')).yaml)
    case 'sh':
    case 'bash':
    case 'zsh':
    case 'fish':
    case 'zshrc':
    case 'bashrc':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/shell')).shell)
    case 'sql':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/sql')).standardSQL)
    case 'xml':
    case 'svg':
    case 'plist':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/xml')).xml)
    case 'go':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/go')).go)
    case 'rb':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/ruby')).ruby)
    case 'lua':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/lua')).lua)
    case 'swift':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/swift')).swift)
    case 'ini':
    case 'cfg':
    case 'conf':
    case 'editorconfig':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/properties')).properties)
    case 'c':
    case 'h':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/clike')).c)
    case 'cpp':
    case 'cc':
    case 'cxx':
    case 'hpp':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/clike')).cpp)
    case 'java':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/clike')).java)
    case 'kt':
    case 'kts':
      return stream(async () => (await import('@codemirror/legacy-modes/mode/clike')).kotlin)
    default:
      return null
  }
}

/** Names that decide the language on their own, extension or not. */
async function languageForName(name: string): Promise<Extension | null> {
  if (name === 'dockerfile' || name.startsWith('dockerfile.')) {
    return stream(async () => (await import('@codemirror/legacy-modes/mode/dockerfile')).dockerFile)
  }
  if (name === 'makefile' || name === 'gnumakefile') {
    return stream(async () => (await import('@codemirror/legacy-modes/mode/shell')).shell)
  }
  // Lock files are named for their purpose, not their format, and they disagree
  // with each other: `Cargo.lock` is TOML, `pnpm-lock.yaml` is YAML, and
  // `package-lock.json` is JSON. Treating them all as JSON painted `Cargo.lock`
  // entirely red.
  if (name === 'cargo.lock' || name === 'poetry.lock' || name === 'uv.lock') {
    return stream(async () => (await import('@codemirror/legacy-modes/mode/toml')).toml)
  }
  if (name.endsWith('.lock') && name.includes('yaml')) {
    return stream(async () => (await import('@codemirror/legacy-modes/mode/yaml')).yaml)
  }
  if (name === 'gemfile' || name === 'rakefile') {
    return stream(async () => (await import('@codemirror/legacy-modes/mode/ruby')).ruby)
  }
  if (name === '.gitignore' || name === '.dockerignore' || name === '.npmrc') {
    return stream(async () => (await import('@codemirror/legacy-modes/mode/properties')).properties)
  }
  return null
}

/** Wraps a legacy stream parser as a CodeMirror language. */
async function stream(load: () => Promise<StreamParser<unknown>>): Promise<Extension> {
  return StreamLanguage.define(await load())
}

/**
 * The line ending a file already uses.
 *
 * CodeMirror splits on any of the three and joins with `\n` unless told
 * otherwise, so a CRLF file opened and saved comes back with every single line
 * changed. Mixed endings are rare and unfixable either way; the majority wins.
 */
export function lineSeparatorOf(text: string): string {
  const crlf = text.split('\r\n').length - 1
  if (crlf === 0) return '\n'
  const lines = text.split('\n').length - 1
  return crlf * 2 >= lines ? '\r\n' : '\n'
}
