# Beacon Split

An agent-first development workspace.

Beacon is not another code editor. It is the surface you work *from* when your
main collaborator is Claude Code and you have several projects open at once. The
unit of the application is not a file — it is:

```
Workspace → Project → Claude session
```

The whole design is pointed at one goal: **flow, speed, and as little context
switching as possible.** Every frequent action is a keystroke, or at most a
click or two.

## Status

Milestones 0 and 2 through 5 are complete, and Milestone 1 is nearly there.
Today Beacon gives you workspaces, projects, tabs, layout presets, the workspace
accent, a file tree with an editor and a `.env` view, git status, diff, stage
and commit, and — per project — a real shell and a real `claude` session, each
in its own PTY.

Sessions outlive the window: closing Beacon leaves them running, and opening it
again reattaches to them.

What is left is a few smaller things listed in the roadmap.

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for what lands when.

## Requirements

- Node 20+ and pnpm 10+
- Rust 1.85+ (edition 2024)
- macOS: Xcode Command Line Tools
- Linux: the usual Tauri v2 system dependencies (webkit2gtk-4.1, libayatana-appindicator3)

## Running it

```sh
pnpm install
pnpm app:dev      # Tauri dev build, hot-reloading frontend
```

Other useful commands:

```sh
pnpm check        # typecheck + vitest + rustfmt + clippy + cargo test
pnpm typecheck    # TypeScript only
pnpm rs:test      # Rust tests only
pnpm app:build    # production bundle, daemon included
```

## Where things live

```
crates/beacon-core/   Domain model, config, persistence, sessions, git.
crates/beacon-daemon/ Owns the PTYs, so sessions outlive the window.
src-tauri/            Desktop shell: window setup and the IPC surface.
src/                  React frontend.
  ipc/                The only module that talks to Tauri.
  app/                Window shell: title bar, workbench, panels, shortcuts.
  features/           Workspaces and projects.
  styles/             Design tokens and global styles.
docs/                 Architecture, roadmap, decisions.
```

`beacon-core` has no dependency on Tauri, which is what let session management
move into `beacon-daemon` without touching the UI. The window is a client: it
attaches to the daemon, renders what it has, and detaches. Closing it ends
nothing.

The daemon listens on a unix socket in the per-user temporary directory, starts
on demand, and stops itself after five minutes with no sessions and nobody
attached.

## Configuration

Beacon stores its state as JSON:

- macOS: `~/Library/Application Support/beacon-split/`
- Linux: `$XDG_CONFIG_HOME/beacon-split/` (usually `~/.config/beacon-split/`)

| File | Contents |
| --- | --- |
| `settings.json` | Application preferences, including `projectsHome` |
| `workspaces.json` | Workspaces and their projects |
| `ui-state.json` | Active tabs and panel sizes |

Project paths under your projects home are stored **relative** to it
(`Personal/beacon-split`, not `/Users/you/projects/Personal/beacon-split`), so
the same configuration works on macOS and Linux.

## Keyboard

`⌘` on macOS, `Ctrl` on Linux.

| Shortcut | Action |
| --- | --- |
| `⌘1` … `⌘9` | Switch to project tab |
| `⌘E` | Toggle Files |
| `⌘G` | Toggle Git |
| `⌘O` | Toggle the editor |
| `⌘J` | Toggle the terminal |
| `⌘↩` | Fullscreen the focused panel |
| `⌘K` | Command palette |
| `⌘P` | Quick open |

Every shortcut is editable in Settings → Keyboard: click one and press the new
one. Only the ones you change are stored, so later changes to a default still
reach you. Numbered project tabs are fixed, since the binding is the number.

## A promise about your files

Removing a project from Beacon removes it from Beacon. It never deletes a
repository, and it never touches the folder on disk. Anything that genuinely
destroys data will be visibly separated from everything that does not.

Deleting a file moves it to the system trash, and every file path is confined to
its project — absolute paths, `..`, and symlinks pointing outside are all
refused.

`.env` values are never logged, never cached outside the file they came from,
and never sent anywhere.
