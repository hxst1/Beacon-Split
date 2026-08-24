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

## Milestone 2 — Terminal / PTY

- `portable-pty` behind a `SessionManager` in `beacon-core`
- xterm.js, resize handling, one shell per project, opened at the project root
- Output buffering so switching projects does not lose scrollback

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

## Milestone 7 — Session daemon

- Move `SessionManager` into a background process
- The UI attaches and detaches; closing the window leaves sessions alive
- Reattach on relaunch; investigate resuming Claude across a machine restart

## Explicitly out of scope

Debugger, plugin marketplace, full LSP, remote SSH, Docker UI, database client,
notebooks, collaboration, and any attempt to reimplement Claude Code.
