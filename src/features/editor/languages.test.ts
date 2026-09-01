import { describe, expect, it } from 'vitest'

import { lineSeparatorOf } from './languages'

describe('keeping a file’s line endings', () => {
  it('leaves a unix file alone', () => {
    expect(lineSeparatorOf('one\ntwo\nthree')).toBe('\n')
  })

  it('keeps CRLF, so saving a windows file does not rewrite every line', () => {
    expect(lineSeparatorOf('one\r\ntwo\r\nthree')).toBe('\r\n')
  })

  it('has an answer for a file with no line ending at all', () => {
    expect(lineSeparatorOf('')).toBe('\n')
    expect(lineSeparatorOf('one line')).toBe('\n')
  })

  it('goes with the majority when a file is mixed', () => {
    // Neither answer round-trips a mixed file, so the one that changes fewer
    // lines is the one that makes the smaller mess of the diff.
    expect(lineSeparatorOf('a\r\nb\r\nc\r\nd\ne')).toBe('\r\n')
    expect(lineSeparatorOf('a\nb\nc\nd\r\ne')).toBe('\n')
  })
})
