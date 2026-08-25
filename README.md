<div align="center">

# Beacon Split

**An agent-first development workspace.**

Beacon is not another code editor. It is the surface you work *from* when your
main collaborator is Claude Code and you have several projects open at once.

[Install](#install) · [What it does](#what-it-does) · [Building it](#building-it) · [Contributing](CONTRIBUTING.md) · [Architecture](docs/ARCHITECTURE.md)

</div>

---

The unit of the application is not a file. It is:

```
Workspace → Project → Claude session
```

Everything is pointed at one goal: **flow, speed, and as little context
switching as possible.** Every frequent action is a keystroke, or at most a
click or two. If something you do daily takes a trip through three menus, that
is a bug.

## What it does

**Runs the real thing.** A `claude` session and a shell per project, each in its
own PTY. Beacon does not reimplement Claude Code — colours, prompts,
permissions, selection and scrolling are whatever the CLI does, because it *is*
the CLI.

**Sessions outlive the window.** A background daemon owns them. Close Beacon and
your work keeps running; open it again and it reattaches, scrollback intact.

**Tells you which project needs you.** With three projects open, the expensive
thing is not switching tabs — it is a permission prompt going unseen for twenty
minutes. Tabs report what Claude is actually doing, from Claude Code's own
hooks rather than guessed from terminal output, and a project that stops and
waits can say so with a system notification.

**Shows what a session is costing.** How much of the five-hour allowance is
left, and how full each project's context is — enough to decide which project
to spend the rest of it on, and when a conversation is worth clearing.

**Files, editor and git, at the size they deserve.** A file tree with the
operations you expect, a light editor, a `.env` view that keeps values hidden
until asked, and git status, diff, stage and commit. Deleting moves to the
trash. Nothing here is trying to become an IDE.

**Keyboard first.** A command palette, quick open, and shortcuts you can
rebind. Layouts are a tree of splits with four presets, and every divider
drags.

**Yours to look at.** Light and dark, how much of the desktop shows through the
window, and whether what shows through is frosted.

## Install

Beacon is macOS today. Releases carry an Apple Silicon and an Intel build;
Apple Silicon is the one it is developed and tested on. Linux is next; Windows
is not planned.

**From a release** — [download the latest][releases], open the `.dmg`, drag
Beacon to Applications.

macOS will refuse it the first time with *"Beacon Split is damaged and can't be
opened"*. It is not damaged: the build is not signed with an Apple Developer ID.
Clear the quarantine flag once:

```sh
xattr -dr com.apple.quarantine "/Applications/Beacon Split.app"
```

That removes a check macOS applies to software it cannot verify. It is a
reasonable thing to do for a build you chose to download; it is not something to
do casually. See [`docs/DISTRIBUTING.md`](docs/DISTRIBUTING.md) for the
alternatives.

**From source** — see [Building it](#building-it). A build made on the machine
it runs on is never quarantined.

### What you need installed

Beacon runs the tools you already have rather than bundling its own, and checks
for them on first run. Anything missing is explained in the panel it affects,
and in Settings → Requirements.

| | Needed for | Install |
| --- | --- | --- |
| [Claude Code][claude] | The Claude panel — the point of the application | `curl -fsSL https://claude.ai/install.sh \| bash` |
| Git | The Git panel, and Quick Open honouring your ignore rules | `xcode-select --install` |

Claude Code needs a Pro, Max, Team or Enterprise account, and you sign in by
running `claude` once in a terminal. Beacon does not handle signing in — it runs
the CLI you already use.

## Keyboard

`⌘` on macOS, `Ctrl` on Linux. All of these are editable in Settings → Keyboard.

| | |
| --- | --- |
| `⌘K` | Command palette |
| `⌘P` | Quick open |
| `⌘1` … `⌘9` | Switch to project tab |
| `⌘E` / `⌘G` / `⌘O` / `⌘J` | Toggle Files, Git, the editor, the terminal |
| `⌘↩` | Fullscreen the focused panel |
| `⌘⇧R` | Restart Claude |
| `⌘S` / `⌘G` | Save / go to line, in the editor |

## Building it

You need Node 20+, pnpm 10+, Rust 1.85+, and the Xcode command line tools.

```sh
git clone https://github.com/hxst1/Beacon-Split.git
cd Beacon-Split
pnpm install
pnpm app:dev
```

| | |
| --- | --- |
| `pnpm app:dev` | Run it, with the frontend hot-reloading |
| `pnpm app:build` | Produce a `.app` and a `.dmg` |
| `pnpm check` | Typecheck, tests, rustfmt, clippy — what CI runs |

## Where things live

```
crates/beacon-core/    Domain model, config, sessions, git, files. No Tauri, no UI.
crates/beacon-daemon/  Owns the PTYs, so sessions outlive the window.
src-tauri/             Desktop shell: window setup and the IPC surface.
src/                   React frontend.
  ipc/                 The only module that talks to Tauri.
  app/                 Window shell: title bar, workbench, panels, shortcuts.
  features/            Everything else, one directory per thing.
docs/                  Architecture, roadmap, and every decision worth recording.
```

Two rules hold the shape:

1. **`beacon-core` never depends on Tauri.** It is why session management could
   move into a daemon without touching the UI.
2. **The UI never imports `@tauri-apps/api` outside `src/ipc/`.** When the
   transport changes, one directory changes.

## Your files

Removing a project from Beacon removes it from Beacon. It never deletes a
repository. Deleting a file moves it to the system trash, and every file path is
confined to its project — absolute paths, `..`, and symlinks pointing outside
are all refused.

`.env` values are never logged, never cached outside the file they came from,
and never sent anywhere.

## Contributing

Genuinely welcome — read [CONTRIBUTING.md](CONTRIBUTING.md) first. The short
version: the scope is deliberately narrow, `docs/DECISIONS.md` explains why
things are the way they are, and a change that contradicts one of those
decisions is welcome as long as it says which one and why it was wrong.

## Documentation

| | |
| --- | --- |
| [Architecture](docs/ARCHITECTURE.md) | How the pieces fit, and the constraints that shaped them |
| [Decisions](docs/DECISIONS.md) | Every choice worth recording, and what it cost |
| [Roadmap](docs/ROADMAP.md) | What is built, what is next, what is deliberately not planned |
| [Contributing](CONTRIBUTING.md) | How to work on it |
| [Distributing](docs/DISTRIBUTING.md) | Handing a build to somebody else |
| [Publishing](docs/PUBLISHING.md) | Cutting a release |

## Licence

[AGPL-3.0](LICENSE).

Use it, change it, run it however you like. If you distribute a modified
version — including running one as a service other people use — you have to
publish your changes under the same licence. That is the whole point: work put
into Beacon stays available to everyone using it.

The copyright holder is not bound by this and may also license Beacon under
other terms. Contributions are made under the same licence and grant that
right; see [CONTRIBUTING.md](CONTRIBUTING.md#licensing).

[releases]: https://github.com/hxst1/Beacon-Split/releases
[claude]: https://claude.com/claude-code
