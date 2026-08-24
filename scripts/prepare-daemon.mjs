/**
 * Stages the session daemon where Tauri's bundler expects a sidecar.
 *
 * Tauri requires an external binary to be named with the target triple it was
 * built for, so a bundle can never pick up one built for a different machine.
 * Beacon builds one daemon, for the host, and names it accordingly.
 */
import { execFileSync } from 'node:child_process'
import { copyFileSync, mkdirSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const profile = process.argv.includes('--release') ? 'release' : 'debug'

const triple = execFileSync('rustc', ['-vV'], { encoding: 'utf8' })
  .split('\n')
  .find((line) => line.startsWith('host:'))
  ?.slice('host:'.length)
  .trim()

if (!triple) {
  console.error('could not work out the host target triple from `rustc -vV`')
  process.exit(1)
}

const built = join(root, 'target', profile, 'beacon-daemon')
if (!existsSync(built)) {
  console.error(`no daemon at ${built} — build it first`)
  process.exit(1)
}

const destination = join(root, 'src-tauri', 'binaries', `beacon-daemon-${triple}`)
mkdirSync(dirname(destination), { recursive: true })
copyFileSync(built, destination)
console.log(`staged ${profile} daemon as beacon-daemon-${triple}`)
