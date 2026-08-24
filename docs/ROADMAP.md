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

Sessions get a sanitised environment: Beacon declares its own terminal identity
rather than inheriting the launching terminal's, which otherwise triggers that
terminal's shell integrations inside ours.

Still to do here: coalescing very fast output into fewer IPC events, and
multiple terminals per project.

## Milestone 2.5 — Settings and layouts ✅

Done before Claude on purpose: three of the four panels were still placeholders,
so generalising the grid was cheap. Every milestone after this one puts real
content in a region and would have made the change more expensive.

- Settings behind a gear in the title bar
- The hard-coded grid is gone. Layouts are a binary split tree, so a preset and
  a hand-arranged layout take the same path through the renderer
- Four presets, previewed from the very tree that would be applied:
  Claude left · Claude right · Tall right · Tall left
- Files, Git and Terminal toggle independently (`⌘E`, `⌘G`, `⌘J`). A hidden
  panel keeps its place in the tree, so showing it puts it back where it was
- Every split is resizable, including Files against Git
- `ui-state.json` migrates from schema 1 and is rewritten once

Remaining: rearranging panels by hand, which is what turns the preset into
`custom`, and per-workspace layouts.

## Milestone 3 — Claude ✅

- The real `claude` CLI in a PTY, one session per project. Colours, prompts,
  permissions, selection and scrolling are whatever the CLI does, because it is
  the CLI
- A project runs its shell and its Claude at the same time, tracked separately
- Restart Claude or the terminal from the panel header or the tab menu; stop a
  project's processes without removing it
- Switching projects keeps every session running and replays nothing
- `claude` is located through the user's interactive login shell, so Beacon is
  no pickier about where it lives than the terminal it was installed from
- Activity on tabs: working, idle, stopped

Still to do here: `dev server` and `error` states, which need understanding what
a session printed rather than that it printed. Detecting them by pattern is
guesswork, so they wait for something better.

## Milestone 4 — Files ✅

- A conventional file tree, loading a level at a time on expand
- New file, new folder, rename, duplicate, copy, paste, copy path, copy
  contents, reveal in Finder / file manager, show or hide dotfiles
- Delete moves to the system trash, is separated from everything else, and asks
  first. Beacon has no operation that removes a file outright
- Every path is confined to its project: absolute paths and `..` refused, and
  the resolved path checked against the canonical root, so a symlink pointing
  outside is refused too
- The editor is a fifth panel, hidden until a file is opened. CodeMirror 6 with
  a theme built from Beacon's own tokens, syntax highlighting loaded on demand,
  history, search and replace, line numbers, `⌘S`
- A `.env` view: values masked, show or copy one at a time, copy `KEY=value`.
  Read fresh each time and kept nowhere else

Still to do here: go-to-line, and following file changes made outside Beacon.

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
