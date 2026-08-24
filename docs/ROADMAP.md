# Roadmap

Built as vertical slices: every milestone ends with something real you can use,
not a layer waiting for the next one.

A milestone is done when TypeScript compiles, Rust compiles, clippy is clean,
the tests pass, and `pnpm app:dev` starts.

## Milestone 0 — Foundation ✅

- Tauri v2 + React + TypeScript + Rust, cargo workspace
- `beacon-core` with no Tauri dependency, ready to become a daemon
- Dark UI base: tokens, translucent surfaces, accent system
- Documentation

## Milestone 1 — Workspaces and projects 🚧

Done:

- Create, rename, recolour and delete workspaces
- Add a project with the native folder picker; toolchain detected automatically
- Rename, remove, move between workspaces, reveal in Finder / file manager
- Project tabs with instant switching, `⌘1`–`⌘9`
- Portable project paths, persisted across restarts
- Panel layout persisted, resizable, toggleable
- Workspace accent applied across the window

Remaining:

- Reordering tabs by dragging
- Workspace icons
- A settings surface for `projectsHome`

## Milestone 2 — Terminal / PTY ✅

- `portable-pty` behind a `SessionManager` in `beacon-core`, with its event sink
  as a trait so the daemon can host it unchanged
- A real login shell per project, opened at the project root
- xterm.js with live resize; the PTY grid follows the panel
- Scrollback retained in the backend and replayed on reattach, with stream
  offsets so nothing is lost or repeated
- Terminal instances outlive their React components, so switching projects
  reparents rather than rebuilds
- Stop a project's processes; removing a project or workspace stops them first

Still to do here: coalescing very fast output into fewer IPC events, and
multiple terminals per project.

## Milestone 2.5 — Settings and layouts

Placed before Claude on purpose. Three of the four panels are still
placeholders, so generalising the grid is cheap right now; every milestone after
this one adds real content to a region and makes the change more expensive.

- A settings surface, reached from a gear in the title bar
- Layout presets, selectable per workspace or globally:
  - Claude left (large), Files/Git right, Terminal bottom — today's layout
  - Files/Git left, Claude right (large), Terminal bottom
  - Terminal bottom-left, Files/Git above it, Claude right
  - The mirror of that one
- A custom layout: choose which panel occupies each region
- `projectsHome`, and whatever else has accumulated by then

This turns the hard-coded grid in `Workbench` into a described layout — a set of
regions and an assignment of panels to them — which is the piece the presets and
the custom option both need.

## Milestone 3 — Claude

- The real `claude` CLI in a PTY, one session per project
- Colours, interactive prompts, permissions, selection, scroll — untouched
- Restart Claude, stop project, switch without losing sessions
- Activity states on tabs: working, idle, dev server, stopped, error

## Milestone 4 — Files

- File tree: expand, open, create, rename, duplicate, copy, paste, delete
- Copy path, copy relative path, copy contents, reveal, show hidden files
- CodeMirror 6: highlighting, edit, save, search and replace, go to line
- A dedicated `.env` view — values masked by default, copy key, copy `KEY=value`

## Milestone 5 — Git

- Status, current branch, diff for the selected file
- Stage, unstage, commit, push, pull
- Driven by the `git` CLI from Rust

## Milestone 6 — UX

- Command palette (`⌘K`) and quick open (`⌘P`)
- User-configurable keyboard bindings
- Panel fullscreen, refined resizing, workspace accent polish

## Milestone 6.5 — Detached panels

- Pop a panel out of the window into its own floating window, and back
- Multi-window Tauri, one webview per detached panel

Milestone 2 already did the hard part: a session is addressed by id, its output
is buffered in the backend, and any view can attach to it and replay losslessly.
A detached window is another attachment, not another session — so this is window
management, not a change to how sessions work.

## Milestone 7 — Session daemon

- Move `SessionManager` into a background process
- The UI attaches and detaches; closing the window leaves sessions alive
- Reattach on relaunch; investigate resuming Claude across a machine restart

## Explicitly out of scope

Debugger, plugin marketplace, full LSP, remote SSH, Docker UI, database client,
notebooks, collaboration, and any attempt to reimplement Claude Code.
