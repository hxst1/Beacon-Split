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
      return (await import('@codemirror/lang-python')).python()
    default:
      // Files that carry their type in the name rather than an extension.
      if (name === 'package.json' || name.endsWith('.lock')) {
        return (await import('@codemirror/lang-json')).json()
      }
      return null
  }
}
