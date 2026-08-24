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

## Milestone 5 — Git ✅

- Status split into staged and unstaged, with the branch and how far ahead or
  behind it is
- Diff for the selected file, including untracked ones
- Stage, unstage, stage all, commit, push, pull
- Driven by the `git` CLI, with `--porcelain=v1 -z` so a path containing spaces
  or quotes is never misread
- Push and pull run off the IPC thread, and git is configured never to stop and
  ask for a password — there is no terminal here for it to ask on

Not planned: branch switching, history, rebase, conflict resolution. The
terminal panel is two keystrokes away and is better at all of them.

## Milestone 6 — UX ✅

- Command palette (`⌘K`): every command in one registry, filtered as you type,
  built fresh on open so it reflects the current state
- Quick open (`⌘P`): fuzzy file search, listed through `git ls-files` where
  there is a repository so the project's own ignore rules apply
- Both fully keyboard-driven, and reachable from inside each other
- Git and the file tree keep up with changes made outside Beacon
- `vitest` for the pure frontend logic — matching and layout maths

Shortcuts are editable from the settings screen: click one, press the new one.
The catalogue of bindable actions and their defaults lives in the backend so
conflict checking has one source of truth; only the shortcuts you change are
stored, so a later change to a default reaches anyone who never overrode it.

Cross-project search is still not planned.

## Milestone 6.5 — Detached panels

- Pop a panel out of the window into its own floating window, and back
- Multi-window Tauri, one webview per detached panel

Milestone 2 already did the hard part: a session is addressed by id, its output
is buffered in the backend, and any view can attach to it and replay losslessly.
A detached window is another attachment, not another session — so this is window
management, not a change to how sessions work.

## Milestone 7 — Session daemon ✅

- `beacon-daemon` owns every PTY. The window is a client: it attaches, renders
  what the daemon has, and detaches. Closing it ends nothing
- Newline-delimited JSON over a unix socket in the per-user temporary directory,
  with the directory's permissions as the access control
- The client starts a daemon if none is listening, and replaces one speaking a
  different protocol rather than guessing at its replies
- Reattaching replays the scrollback and joins the live stream losslessly, using
  the offsets from Milestone 2
- The daemon stops itself after five minutes with no sessions and nobody
  attached, and can be stopped deliberately from the palette
- The connection repairs itself: a restarted daemon is found again, and the
  window rebuilds its terminals rather than going quietly deaf
- The socket is an argument rather than a constant, so tests cannot reach a
  daemon someone is working in

Packaged as a Tauri sidecar: the bundler names the binary with its target
triple so a bundle can never pick up one built for another machine, and places
it beside the main executable — which is exactly where the client looks, in a
bundle and in development alike. `pnpm app:build` builds and stages it.

Resuming Claude across a machine restart is a separate question — a process
cannot survive a shutdown, so it means using Claude's own resume, and that
belongs with whatever comes next.

## Later — not scheduled

Both of these were on the original "do not build" list. They are here because
the list was ours to change, and they are worth doing eventually; neither is
small, and both are their own milestone.

### Docker

Connect to the Docker the user already runs — its unix socket speaks an HTTP
API — and show containers for the current project: what is up, logs, and
starting or stopping them. Beacon would not manage Docker, only give the
containers a place beside the work. Hidden by default, like the editor.

### A small database viewer

Read-first, PostgreSQL only to begin with, because that is what gets used.
Browse tables, look at rows, run a query. Connection details picked up from the
project where they already exist — a `.env`, a Prisma schema, a Supabase config
— rather than configured twice. Adding another engine later is a driver, not a
redesign, provided the first one does not assume Postgres everywhere.

The line to hold: neither of these becomes the point of the application. If
either starts wanting its own window, it was the wrong feature.

## Explicitly out of scope

Debugger, plugin marketplace, full LSP, remote SSH, Docker UI, database client,
notebooks, collaboration, and any attempt to reimplement Claude Code.
