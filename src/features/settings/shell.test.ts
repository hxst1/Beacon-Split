import { describe, expect, it } from 'vitest'

import { formatShell, parseShell } from './shell'

describe('the shell field', () => {
  it('reads a program and its arguments', () => {
    expect(parseShell('fish -l')).toEqual({ program: 'fish', args: ['-l'] })
  })

  it('is cleared by an empty field', () => {
    expect(parseShell('   ')).toBeNull()
  })

  it('keeps a quoted path with spaces in one piece', () => {
    expect(parseShell('"C:\\Program Files\\Git\\bin\\bash.exe" -l -i')).toEqual({
      program: 'C:\\Program Files\\Git\\bin\\bash.exe',
      args: ['-l', '-i'],
    })
  })

  it('writes back what it read', () => {
    const shell = { program: 'C:\\Program Files\\PowerShell\\7\\pwsh.exe', args: ['-NoLogo'] }
    expect(parseShell(formatShell(shell))).toEqual(shell)
    expect(formatShell({ program: 'zsh', args: ['-l'] })).toBe('zsh -l')
  })
})
