# Architecture

## The shape of the problem

Beacon runs several projects at once, each with a long-lived Claude session and
a shell, and it must survive the window closing without those processes dying.
That single requirement — process lifetime outliving the UI — is what the layout
below is built around.

## Layers

```
┌──────────────────────────────────────────────┐
│ React UI                    src/             │
│   components, stores, styles                 │
├──────────────────────────────────────────────┤
│ IPC boundary                src/ipc/         │
│   the only module that calls invoke()        │
├──────────────────────────────────────────────┤
│ Tauri shell                 src-tauri/       │
│   window setup, command handlers             │
├──────────────────────────────────────────────┤
│ Core                        crates/beacon-core/
│   domain, config, persistence,               │
│   later: session + process management        │
└──────────────────────────────────────────────┘
```

Two rules keep the layers honest:

1. **`beacon-core` never depends on Tauri.** It compiles as a plain library, is
   tested without a window, and is what moves into a daemon in Milestone 7.
2. **The UI never imports `@tauri-apps/api` outside `src/ipc/`.** When the
   transport changes from in-process commands to a daemon socket, one directory
   changes.

## State flow

Every mutating command returns a complete `Snapshot` of the application state,
and the frontend replaces its store with it:

```
component → src/ipc → tauri command → beacon-core → JSON on disk
                                          │
                                      Snapshot
                                          ▼
                                   zustand store → re-render
```

This is deliberately not optimistic. Beacon's mutations are cheap — a couple of
small file writes — and always rendering what the backend actually holds removes
a whole class of "the UI thinks it renamed something" bugs. If a write fails, the
previous snapshot stays on screen and the error appears in the status bar.

The one exception is panel dragging: sizes update locally at pointer rate and
are committed once, on release, so a drag is never a burst of file writes.

## Persistence

Three JSON documents, split by how often they change and how much they matter:

| File | Changes | Loss impact |
| --- | --- | --- |
| `settings.json` | Rarely | Preferences |
| `workspaces.json` | On edit | Your project list |
| `ui-state.json` | Constantly | Where the panels were |

Each carries a `schemaVersion`. Reading a document written by a newer build is a
hard error rather than a silent misparse. Writes go to a temporary file and are
renamed into place, so a crash mid-write cannot truncate your workspace list.

JSON rather than SQLite: the whole dataset is a few kilobytes, it is
hand-editable, and it diffs. See ADR-004.

## Portable project paths

A project path is stored as one of two things:

```jsonc
{ "base": "projectsHome", "relative": "Personal/beacon-split" }  // portable
{ "base": "absolute", "path": "/opt/src/thing" }                 // outside the root
```

Anything under your projects home is stored relative to it, so the same config
file works on macOS (`/Users/you/projects`) and Linux (`/home/you/projects`).
Paths outside that root stay absolute — correctness first, portability where it
is free. Stored paths always use `/` separators.

Resolution happens on every read, in `Beacon::snapshot`. The frontend receives
absolute paths and never reconstructs them itself.

## Layout

The window is described by a tree, not a grid:

```jsonc
{ "type": "split", "direction": "column", "fraction": 0.72,
  "first":  { "type": "split", "direction": "row", "fraction": 0.74,
              "first":  { "type": "panel", "panel": "claude" },
              "second": { "type": "split", "direction": "column", "fraction": 0.6,
                          "first":  { "type": "panel", "panel": "files" },
                          "second": { "type": "panel", "panel": "git" } } },
  "second": { "type": "panel", "panel": "terminal" } }
```

`LayoutView` recurses over it, rendering each split as a three-track grid —
first child, splitter, rest — so a splitter is the border rather than sitting
beside one. Presets are trees the backend serves, which is also what the
settings previews are drawn from: a preview cannot describe an arrangement that
would not actually be applied.

Hidden panels stay in the tree and are pruned at render time, so showing one
again returns it to its place. A tree that drops or repeats a panel is rejected
by the backend.

## The accent system

A workspace declares one colour. The frontend writes it to `--accent` on the
document root, and every tint in the stylesheet derives from it with
`color-mix`. Adding a workspace never means adding CSS.

The window-edge signal is a 1px inset hairline plus a wide, very low-opacity
bloom, dimmed when the window loses focus. It has to be recognisable in
peripheral vision and invisible when you are reading — anything heavier fails
both tests.

## Shortcuts

Bindings are written against "the primary modifier", resolved once from the
platform reported by the backend, so one table is correct on macOS and Linux.
The indirection exists now so that making bindings user-configurable in
Milestone 6 does not touch every call site.

## What is not here yet

PTY handling, session management and the daemon are Milestones 2, 3 and 7. The
constraint that shapes today's code is that none of them may require moving
domain logic out of the UI later — which is why that logic is already in
`beacon-core` rather than in Tauri command handlers.

Planned shape:

```
UI  ──IPC──▶  Tauri shell  ──▶  SessionManager (beacon-core)
                                      │
                                 portable-pty
                                      │
                              shell / claude process
```

`SessionManager` is designed to be hostable either in-process (today's plan for
Milestones 2–3) or in a daemon the UI attaches to (Milestone 7). Nothing above
it should be able to tell the difference.
