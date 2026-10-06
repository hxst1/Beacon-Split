# Sharing Beacon with someone else

Written for handing Beacon to a colleague on macOS, which is the one that needs
explaining. Linux has its own short section at the end; Windows is the
installer from the release page and nothing more.

## The short version

`pnpm app:build` produces two things under `target/release/bundle/`:

- `macos/Beacon Split.app` — the application
- `dmg/Beacon Split_<version>_aarch64.dmg` — the thing to send

Both are Apple Silicon only. An Intel Mac needs a build on an Intel machine, or
a universal build, which is not set up.

## Gatekeeper will refuse it, and the message is misleading

The build is **ad-hoc signed**: it has no Apple Developer ID and is not
notarised. Anyone who downloads the DMG gets the file quarantined, and macOS
says:

> "Beacon Split" is damaged and can't be opened. You should move it to the Bin.

It is not damaged. That is what macOS says about an app whose signature it
cannot trace to a developer it knows.

There are three ways out, in the order worth considering them.

### 1. Let them build it (best for a handful of colleagues)

An application built on the machine it runs on is never quarantined. It also
means they get updates by pulling.

```sh
git clone <the repo> && cd beacon-split
pnpm install
pnpm app:dev
```

They need Node 20+, pnpm, Rust 1.85+, and the Xcode command line tools. Beacon
tells them what else is missing on first run — see Settings → Requirements.

### 2. Have them clear the quarantine flag

If they want the DMG rather than the source:

```sh
xattr -dr com.apple.quarantine "/Applications/Beacon Split.app"
```

Be straight with them about what this is: it removes a check macOS applies to
software it cannot verify. It is reasonable for an application a colleague
built and handed over in person. It is not something to tell strangers to run.

### 3. Sign and notarise it properly

The real answer for anything beyond a few colleagues. It needs an Apple
Developer Program membership and a Developer ID Application certificate.
Tauri picks these up from the environment:

```sh
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="app-specific-password"
export APPLE_TEAM_ID="TEAMID"
pnpm app:build
```

Signed and notarised, the DMG opens with no warnings and nobody has to be told
to run anything.

## On Linux it is an AppImage, and there is nothing to get past

No signing, no quarantine, no warning: an AppImage is an executable file.

```sh
chmod +x 'Beacon Split_'*'_amd64.AppImage'
./'Beacon Split_'*'_amd64.AppImage'
```

It carries its own GTK and WebKit, so nothing has to be installed for Beacon
itself to run — only the tools it runs, below.

**Build it in CI, not here.** An AppImage carries the GTK and WebKit of the
machine that made it, and those link against that machine's glibc, so one built
on Arch runs on Arch. The release workflow builds it on Ubuntu 22.04 for that
reason, which puts the floor at glibc 2.35. On a rolling distribution
`pnpm app:build` stops after the executable and says why — linuxdeploy's GTK
plugin needs a gdk-pixbuf layout that gdk-pixbuf 2.44 no longer produces. See
ADR-093. `target/release/beacon-split` runs, and is what to open when the
question is whether a release build works.

To hand someone a build from a machine you control, take the AppImage from the
release the tag produced rather than from your own `target/`.

## What they need installed

Beacon runs the tools you already have rather than bundling its own, and checks
for them on startup. Settings → Requirements lists each one with what it costs
to be without it and what to run.

| | Needed for | Install |
| --- | --- | --- |
| Claude Code | The Claude panel — the point of the application | `curl -fsSL https://claude.ai/install.sh \| bash` |
| Git | The Git panel, and Quick Open honouring your ignore rules | `xcode-select --install`, or on Linux the distribution's own |

Claude Code needs a Pro, Max, Team or Enterprise account, and signing in happens
by running `claude` once in a terminal. Beacon does not handle signing in — it
runs the CLI they already use.

Nothing else is required. Workspaces, projects, files, the editor and terminals
work without either.
