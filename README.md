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

Milestones 0 and 2 are complete and Milestone 1 is nearly there. Today Beacon
gives you workspaces, projects, tabs, persisted layout, the workspace accent,
and a real shell per project running in a PTY. Claude, files and git are
placeholders that name the milestone they arrive in.

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
pnpm check        # typecheck + rustfmt + clippy + cargo test
pnpm typecheck    # TypeScript only
pnpm rs:test      # Rust tests only
pnpm app:build    # production bundle
```

## Where things live

```
crates/beacon-core/   Domain model, config, persistence. No Tauri, no UI.
src-tauri/            Desktop shell: window setup and the IPC surface.
src/                  React frontend.
  ipc/                The only module that talks to Tauri.
  app/                Window shell: title bar, workbench, panels, shortcuts.
  features/           Workspaces and projects.
  styles/             Design tokens and global styles.
docs/                 Architecture, roadmap, decisions.
```

`beacon-core` deliberately has no dependency on Tauri. Session and process
management will eventually run in a background daemon so that closing the
window does not kill live Claude sessions, and that crate is what moves there.

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
| `⌘E` | Toggle the Files / Git column |
| `⌘J` | Toggle the terminal |
| `⌘↩` | Fullscreen the focused panel |

The command palette (`⌘K`) and quick open (`⌘P`) arrive in Milestone 6, along
with user-configurable bindings.

## A promise about your files

Removing a project from Beacon removes it from Beacon. It never deletes a
repository, and it never touches the folder on disk. Anything that genuinely
destroys data will be visibly separated from everything that does not.

`.env` values are never logged, never cached outside the file they came from,
and never sent anywhere.
