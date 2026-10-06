/**
 * Runs `tauri build`, with whatever this platform's bundler needs said for it.
 *
 * On macOS and Windows nothing needs saying: `tauri build` produces the `.dmg`
 * or the installer, and this is a pass-through that exists so one command means
 * the same thing everywhere.
 *
 * Linux needs two things, and both of them are about linuxdeploy — the tool
 * Tauri drives to put an AppImage together, which carries its own binutils and
 * its own GTK plugin:
 *
 *   - `NO_STRIP`, because the `strip` inside linuxdeploy is older than the
 *     binutils a current distribution's libraries were linked with, and stops
 *     on a section it does not recognise: `unknown type [0x13] section
 *     '.relr.dyn'`, once per bundled library, and then the whole bundle fails.
 *     Nothing is lost by not stripping: the release profile already strips the
 *     binary Beacon ships, and these are the distribution's own libraries.
 *
 *   - skipping the bundle where its GTK plugin cannot run at all. The plugin
 *     copies gdk-pixbuf's loader directory into the AppDir and writes a loader
 *     cache inside it; gdk-pixbuf 2.44 builds the loaders into the library and
 *     stops creating that directory, so on a distribution that has it — Arch
 *     today — the plugin fails on a path that is not there, and fails again on
 *     the next one if you make that one exist.
 *
 * A build that skips it still produces the release executable, which is what
 * `pnpm app:build` is for in a checkout: proving that a release build runs, not
 * producing the artefact anybody installs. The AppImage that gets released is
 * built on Ubuntu in CI — which is where it should be built regardless, since
 * an AppImage carries the GTK and WebKit of the machine that made it, and one
 * built here would only run on a machine as new as this one.
 */
import { execFileSync, spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'

const args = process.argv.slice(2)
const env = { ...process.env }

if (process.platform === 'linux') {
  env['NO_STRIP'] = 'true'

  const loaders = pixbufLoaderDirectory()
  if (loaders && !existsSync(loaders)) {
    args.push('--no-bundle')
    console.log(
      `not bundling an AppImage: linuxdeploy's GTK plugin needs ${loaders}, which gdk-pixbuf ` +
        'no longer creates. Building the release executable instead; the released AppImage is ' +
        'built on Ubuntu in CI.',
    )
  }
}

// Through node rather than the `tauri` on the PATH, which is a shell script on
// unix and a `.cmd` on Windows.
const cli = createRequire(import.meta.url).resolve('@tauri-apps/cli/tauri.js')
const build = spawnSync(process.execPath, [cli, 'build', ...args], { stdio: 'inherit', env })

process.exit(build.status ?? 1)

/**
 * Where gdk-pixbuf says its loader modules live, or `''` when nothing here can
 * say.
 *
 * Asked of pkg-config, because that is what the plugin asks. Without an answer
 * we let the bundle run and fail in the open: guessing that it cannot work is
 * how somebody ends up without an AppImage on a machine that could have made
 * one.
 */
function pixbufLoaderDirectory() {
  try {
    return execFileSync('pkg-config', ['--variable=gdk_pixbuf_binarydir', 'gdk-pixbuf-2.0'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
  } catch {
    return ''
  }
}
