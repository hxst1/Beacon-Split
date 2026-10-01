import type { ShellSpec } from '@/types/beacon'

/**
 * Reads the shell field: a program, then its arguments.
 *
 * The program may be quoted, because on Windows it is usually a path through
 * `C:\Program Files`, and splitting that at the space would name a program
 * that does not exist. Arguments stay split on whitespace, as before.
 */
export function parseShell(text: string): ShellSpec | null {
  const trimmed = text.trim()
  const quoted = /^"([^"]+)"\s*(.*)$/.exec(trimmed)
  const [program, ...args] = quoted
    ? [quoted[1] ?? '', ...(quoted[2] ?? '').split(/\s+/)]
    : trimmed.split(/\s+/)
  return program ? { program, args: args.filter(Boolean) } : null
}

/** The reverse of {@link parseShell}, quoting a program that needs it. */
export function formatShell(shell: ShellSpec): string {
  const program = /\s/.test(shell.program) ? `"${shell.program}"` : shell.program
  return [program, ...shell.args].join(' ')
}
