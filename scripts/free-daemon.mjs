/**
 * Moves a running session daemon's binary out of the way, on Windows.
 *
 * The daemon outlives the window by design, so there is usually one running
 * from `target/` while you work. Windows will not overwrite or delete a program
 * that is running, and the next `cargo build` fails with "Access is denied"
 * until it exits. It does allow the file to be renamed: the daemon carries on
 * from its new name — exactly as on macOS, where the build replaces the file
 * under a running process — and the old name is free for the build.
 *
 * Earlier leftovers are removed when nothing is running from them any more.
 * Elsewhere this does nothing.
 */
import { closeSync, existsSync, openSync, readdirSync, renameSync, rmSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

if (process.platform !== 'win32') process.exit(0)

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

for (const profile of ['debug', 'release']) {
  const dir = join(root, 'target', profile)
  if (!existsSync(dir)) continue

  for (const name of readdirSync(dir)) {
    if (name.startsWith('beacon-daemon.exe.') && name.endsWith('.old')) {
      try {
        rmSync(join(dir, name))
      } catch {
        // Still running; it goes next time.
      }
    }
  }

  const binary = join(dir, 'beacon-daemon.exe')
  if (!existsSync(binary) || !inUse(binary)) continue

  renameSync(binary, `${binary}.${Date.now()}.old`)
  console.log(`a daemon is running from target/${profile}; moved it aside so the build can replace it`)
}

/** A running program's file opens for reading, never for writing. */
function inUse(path) {
  try {
    closeSync(openSync(path, 'r+'))
    return false
  } catch {
    return true
  }
}
