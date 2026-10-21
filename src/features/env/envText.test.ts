import { describe, expect, it } from 'vitest'

import { formatEnvValue, isEnvKey, readEnvLine, readEnvLines, setEnvValue } from './envText'

describe('adding a variable', () => {
  it('starts an empty file', () => {
    expect(setEnvValue('', 'API_KEY', 'abc')).toBe('API_KEY=abc\n')
  })

  it('goes at the end, after everything already there', () => {
    expect(setEnvValue('# keys\nA=1\n', 'B', '2')).toBe('# keys\nA=1\nB=2\n')
  })

  it('does not glue itself onto a last line with no newline', () => {
    expect(setEnvValue('A=1', 'B', '2')).toBe('A=1\nB=2\n')
  })

  it('keeps a windows file in CRLF', () => {
    expect(setEnvValue('A=1\r\n', 'B', '2')).toBe('A=1\r\nB=2\r\n')
  })
})

describe('changing a variable', () => {
  it('rewrites the line in place and leaves the rest alone', () => {
    expect(setEnvValue('# keys\nA=1\n\nB=2\n', 'A', 'one')).toBe('# keys\nA=one\n\nB=2\n')
  })

  it('keeps export and a trailing comment', () => {
    expect(setEnvValue('export PORT=3000 # the dev port\n', 'PORT', '4000')).toBe(
      'export PORT=4000 # the dev port\n',
    )
  })

  it('keeps a trailing comment when the new value needs quotes', () => {
    const text = setEnvValue('PORT=3000 # dev\n', 'PORT', 'a b')
    expect(text).toBe('PORT="a b" # dev\n')
    expect(readEnvLines(text)).toEqual([{ key: 'PORT', value: 'a b' }])
  })

  it('keeps a trailing comment after a quoted value', () => {
    expect(setEnvValue('A="x" # note\n', 'A', 'y')).toBe('A=y # note\n')
  })

  it('keeps a comment that has quotes of its own', () => {
    expect(readEnvLine("A='x' # don't commit")).toEqual({ key: 'A', value: 'x' })
    expect(readEnvLine('B="x" # say "hi"')).toEqual({ key: 'B', value: 'x' })
    expect(readEnvLine('C="a \\"b\\"" # note')).toEqual({ key: 'C', value: 'a "b"' })
    expect(setEnvValue("A='x' # don't commit\n", 'A', 'y')).toBe("A=y # don't commit\n")
  })

  it('moves a comment the new value would swallow onto its own line', () => {
    const text = setEnvValue("A=1 # it's\n", 'A', 'a b')
    expect(readEnvLines(text)).toEqual([{ key: 'A', value: 'a b' }])
    expect(text).toContain("# it's\n")
  })

  it('does not mistake a hash inside a quoted value for a comment', () => {
    expect(setEnvValue('A="x # y"\n', 'A', 'z')).toBe('A=z\n')
  })

  it('changes the line it was asked to when a key is assigned twice', () => {
    expect(setEnvValue('A=1\nA=2\n', 'A', 'x', 1)).toBe('A=x\nA=2\n')
  })

  it('changes the one that wins when no line is given', () => {
    expect(setEnvValue('A=1\nA=2\n', 'A', 'x')).toBe('A=1\nA=x\n')
  })

  it('falls back to the key when the line has moved', () => {
    expect(setEnvValue('# new\nA=1\n', 'A', 'x', 1)).toBe('# new\nA=x\n')
  })

  it('keeps CRLF on the line it rewrote', () => {
    expect(setEnvValue('A=1\r\nB=2\r\n', 'A', 'x')).toBe('A=x\r\nB=2\r\n')
  })

  it('does not touch a commented-out assignment', () => {
    expect(setEnvValue('# A=old\n', 'A', 'new')).toBe('# A=old\nA=new\n')
  })

  it('finds the first variable behind a byte-order mark, and keeps the mark', () => {
    expect(setEnvValue('\uFEFFA=1\n', 'A', '2')).toBe('\uFEFFA=2\n')
    expect(readEnvLines('\uFEFFA=1\n')).toEqual([{ key: 'A', value: '1' }])
  })
})

describe('writing a value so it reads back the same', () => {
  const awkward = [
    '',
    'plain',
    'postgres://u:p@localhost/app?x=1#frag',
    'two words',
    ' padded ',
    'a #b',
    'say "hi"',
    '"quoted"',
    "'quoted'",
    "it's",
    'C:\\new',
    "it's C:\\new",
    'back\\"slash',
    '-----BEGIN KEY-----\nabc\n-----END KEY-----',
    'x # y',
  ]

  it.each(awkward)('round-trips %j', (value) => {
    const written = setEnvValue('# head\nOTHER=1\n', 'K', value)
    expect(readEnvLines(written)).toEqual([
      { key: 'OTHER', value: '1' },
      { key: 'K', value },
    ])
    expect(readEnvLines(setEnvValue(written, 'K', 'next'))).toContainEqual({
      key: 'K',
      value: 'next',
    })
  })

  it('leaves a plain value bare and quotes one with spaces', () => {
    expect(formatEnvValue('abc#def')).toBe('abc#def')
    expect(formatEnvValue('two words')).toBe('"two words"')
  })

  it('writes a line break as an escape inside double quotes', () => {
    expect(formatEnvValue('one\ntwo')).toBe('"one\\ntwo"')
  })

  it('refuses what the parser could not read back, rather than writing something else', () => {
    expect(formatEnvValue('a\\nb\nc')).toBeNull()
    expect(formatEnvValue('a\rb')).toBeNull()
    expect(() => setEnvValue('', 'K', 'a\rb')).toThrow()
  })
})

describe('reading what somebody pasted', () => {
  it('reads export, quotes and comments the way the file would', () => {
    expect(readEnvLine('export TOKEN="abc def" # from the dashboard')).toEqual({
      key: 'TOKEN',
      value: 'abc def',
    })
  })

  it('takes every assignment in a block and skips the rest', () => {
    expect(readEnvLines('# copied\nA=1\r\n\nB=two\nnot one\n')).toEqual([
      { key: 'A', value: '1' },
      { key: 'B', value: 'two' },
    ])
  })
})

describe('which keys can be written', () => {
  it('takes what the parser takes', () => {
    expect(isEnvKey('NEXT_PUBLIC_URL')).toBe(true)
    expect(isEnvKey('app.port')).toBe(true)
    expect(isEnvKey('')).toBe(false)
    expect(isEnvKey('BAD KEY')).toBe(false)
    expect(isEnvKey('A-B')).toBe(false)
  })
})
