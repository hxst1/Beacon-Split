import { lineSeparatorOf } from '@/features/editor/languages'

/*
 * The reading half of this file mirrors `beacon-core/src/dotenv.rs` line for
 * line. The view lists what that parser reads; anything written here has to
 * read back through it as exactly what was typed, so the two cannot disagree.
 */

/** The keys the parser reads; anything else it skips. */
export function isEnvKey(key: string): boolean {
  return /^[A-Za-z0-9_.]+$/.test(key)
}

/** One line read the way the parser reads it, or null where it skips it. */
export function readEnvLine(raw: string): { key: string; value: string } | null {
  const line = raw.trim()
  if (line === '' || line.startsWith('#')) return null
  const unexported = line.startsWith('export ') ? line.slice('export '.length).trimStart() : line
  const equals = unexported.indexOf('=')
  if (equals === -1) return null
  const key = unexported.slice(0, equals).trim()
  if (!isEnvKey(key)) return null
  return { key, value: readEnvValue(unexported.slice(equals + 1).trim()) }
}

/** Every assignment in a block of text — what somebody pasted, say. */
export function readEnvLines(text: string): { key: string; value: string }[] {
  return text
    .replace(/^\uFEFF/, '')
    .split('\n')
    .flatMap((raw) => readEnvLine(raw) ?? [])
}

function readEnvValue(trimmed: string): string {
  const raw = withoutCommentAfterQuotes(trimmed)
  for (const quote of ['"', "'"]) {
    if (raw.length >= 2 && raw.startsWith(quote) && raw.endsWith(quote)) {
      const inner = raw.slice(1, -1)
      return quote === '"' ? inner.replaceAll('\\n', '\n').replaceAll('\\"', '"') : inner
    }
  }
  const comment = raw.indexOf(' #')
  return comment === -1 ? raw : raw.slice(0, comment).trimEnd()
}

/** `"a value" # a note` → `"a value"`, as `without_comment_after_quotes` does. */
function withoutCommentAfterQuotes(raw: string): string {
  return raw.slice(0, raw.length - commentAfterQuotes(raw).length)
}

/** The ` # a note` after a quoted value, or nothing. */
function commentAfterQuotes(raw: string): string {
  const quote = raw[0]
  if (quote !== '"' && quote !== "'") return ''
  const close = closingQuote(raw, quote)
  if (close === -1) return ''
  const rest = raw.slice(close + 1)
  return /^\s/.test(rest) && rest.trimStart().startsWith('#') ? rest : ''
}

/**
 * Where the quoted value opening `raw` ends: the first quote, so a note can
 * have quotes of its own. `\"` does not end a double-quoted value, the same as
 * it is unescaped; nothing escapes inside single quotes.
 */
function closingQuote(raw: string, quote: string): number {
  let escaped = false
  for (let at = 1; at < raw.length; at += 1) {
    if (raw[at] === quote && !escaped) return at
    escaped = quote === '"' && raw[at] === '\\'
  }
  return -1
}

/**
 * A value as it has to be written to read back unchanged — or null for the
 * few that cannot be, which are better refused than written as something else.
 *
 * Bare where it can be, because that is what people write. Otherwise in double
 * quotes, or single quotes where double would read a backslash as an escape.
 */
export function formatEnvValue(value: string): string | null {
  const candidates = [
    // The parser would read `two words` bare, but a shell sourcing the file
    // would not.
    ...(/\s/.test(value) ? [] : [value]),
    `"${value.replaceAll('"', '\\"').replaceAll('\n', '\\n')}"`,
    `'${value}'`,
  ]
  return (
    candidates.find(
      // The file is read a line at a time, so nothing written can hold a break.
      (written) => !/[\r\n]/.test(written) && readEnvLine(`K=${written}`)?.value === value,
    ) ?? null
  )
}

/**
 * Sets one variable in a `.env` file's text, touching nothing else in it.
 *
 * An existing assignment is rewritten in place — `line`, when given, says which
 * one, since a file can assign the same key twice — and keeps its `export` and
 * any trailing comment. A new one goes at the end. Comments, blank lines and
 * line endings are the file's own and stay as they were.
 *
 * Throws for a value `formatEnvValue` refuses; check that first.
 */
export function setEnvValue(text: string, key: string, value: string, line?: number): string {
  const formatted = formatEnvValue(value)
  if (formatted === null) throw new Error(`${key}'s value cannot be written to a .env file as is`)

  const separator = lineSeparatorOf(text)
  const lines = text.split('\n')
  const assignment = `${key}=${formatted}`

  const target =
    line !== undefined && readEnvLine(lines[line - 1] ?? '')?.key === key
      ? line - 1
      : lines.findLastIndex((raw) => readEnvLine(raw)?.key === key)

  if (target === -1) {
    if (text === '') return `${assignment}${separator}`
    const ended = text.endsWith('\n') ? text : `${text}${separator}`
    return `${ended}${assignment}${separator}`
  }

  const raw = lines[target] ?? ''
  const carriage = raw.endsWith('\r') ? '\r' : ''
  const body = carriage ? raw.slice(0, -1) : raw
  // `\s` takes a byte-order mark too, so the first line keeps one if it had it.
  const lead = /^\s*(export\s+)?/.exec(body)?.[0] ?? ''
  const comment = trailingComment(body)
  const rewritten = `${lead}${assignment}${comment}`
  if (comment === '' || readEnvLine(rewritten)?.value === value) {
    lines[target] = `${rewritten}${carriage}`
  } else {
    // A comment the new value would swallow moves to a line of its own.
    const note = `#${comment.trimStart().slice(1)}`
    lines.splice(target, 1, `${note}${carriage}`, `${lead}${assignment}${carriage}`)
  }
  return lines.join('\n')
}

/** ` # like this` at the end of an assignment, which an edit should keep. */
function trailingComment(body: string): string {
  const value = body.slice(body.indexOf('=') + 1).trim()
  const afterQuotes = commentAfterQuotes(value)
  if (afterQuotes !== '') return afterQuotes
  if (value.startsWith('"') || value.startsWith("'")) return ''
  const at = value.indexOf(' #')
  return at === -1 ? '' : value.slice(at)
}
