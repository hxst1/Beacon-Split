# Decisions

Short records of choices that would be expensive to reverse. Context, decision,
consequence — nothing more.

## ADR-001: Tauri instead of Electron

**Context.** Beacon must run on macOS Apple Silicon and Arch Linux x86_64, feel
instant, and eventually supervise long-lived child processes.

**Decision.** Tauri v2 with a Rust backend.

**Why.** Process supervision, PTY handling and filesystem work are the backend's
real job here, and that is Rust work either way. Tauri lets the same code own
the window and the processes without a Node runtime in between. Bundle size and
memory are secondary benefits.

**Consequence.** The webview is the system one — WebKit on macOS, WebKitGTK on
Linux — so CSS support is whatever those ship, and rendering differs slightly
between platforms. We accept that; nothing in Beacon's UI needs Chromium.

## ADR-002: A cargo workspace with `beacon-core` separate from `src-tauri`

**Context.** Milestone 7 requires sessions to survive the window closing, which
means a background process that is not the UI.

**Decision.** All domain logic, configuration and persistence live in
`crates/beacon-core`, which does not depend on Tauri. `src-tauri` only sets up
the window and translates IPC.

**Why.** A daemon is a different host for the same logic. If that logic starts
inside Tauri command handlers, extracting it later touches everything.

**Consequence.** A little indirection today — commands are thin wrappers — in
exchange for the daemon being a new binary rather than a rewrite. It also makes
the core testable without a window, which is where most of the tests are.

## ADR-003: Commands return a full snapshot

**Context.** The frontend needs to stay in step with the backend across
workspace, project and layout mutations.

**Decision.** Every mutating command returns the complete application state, and
the store replaces itself with it.

**Why.** The dataset is tiny and the mutations are rare in machine terms.
Optimistic updates and partial patches would buy latency we do not need and cost
us a class of divergence bugs we would rather not have.

**Consequence.** If the state ever grows large enough for this to be felt, this
is the decision to revisit. Panel dragging already opts out: it updates locally
and commits once on release.

## ADR-004: JSON files, not SQLite

**Context.** Beacon needs to persist settings, workspaces and UI state.

**Decision.** Three JSON documents in the platform config directory, written
atomically, each carrying a `schemaVersion`.

**Why.** The entire dataset is a handful of kilobytes. JSON is inspectable,
hand-editable, diffable and syncable. SQLite would add a dependency, a migration
story and a binary blob for no benefit at this size.

**Consequence.** Reasonable up to a few hundred projects. Revisit if session
history or file indexes need to be stored — those are the workloads that would
justify a database.

## ADR-005: Portable project paths

**Context.** The same configuration should work on macOS (`/Users/x/projects`)
and Linux (`/home/x/projects`).

**Decision.** Projects under the configured projects home are stored relative to
it; anything else is stored absolute. Stored paths always use `/`.

**Why.** It makes the common case portable without failing the uncommon one. A
scheme that forced everything to be relative would break projects outside the
root, which is a real case.

**Consequence.** Moving your projects home relocates every relative project at
once — usually what you want, occasionally surprising. Absolute paths are still
absolute and do not travel.

## ADR-006: Removing a project never touches the disk

**Context.** "Remove" is ambiguous in tools that manage folders, and getting it
wrong destroys someone's work.

**Decision.** Removing a project or deleting a workspace only edits Beacon's own
configuration. No Beacon operation deletes a repository.

**Why.** The cost of being wrong is unbounded and unrecoverable. There is no
version of this feature worth that risk.

**Consequence.** The menu item says "Files are kept" at the point of decision
rather than behind a confirmation dialog. Any genuinely destructive action added
later must be visually separated from everything else.

## ADR-007: The git CLI, not libgit2

**Context.** Milestone 5 needs status, diff, branch, stage, commit, push, pull.

**Decision.** Shell out to `git`.

**Why.** It is already installed, already configured — credentials, hooks,
signing, `includeIf` — and its behaviour is exactly what the user sees in their
own terminal. libgit2 would re-implement a subset of that and diverge on the
details that matter.

**Consequence.** We parse text output, so parsers need to use porcelain formats
with explicit versions. Revisit only if a specific operation proves too slow.

## ADR-008: Shortcuts target a "primary modifier"

**Context.** ⌘ on macOS, Ctrl on Linux, one binding table.

**Decision.** Bindings are declared against the primary modifier, resolved once
from the platform the backend reports.

**Why.** Sniffing the user agent per handler is how platform bugs get in. One
resolution point, one table.

**Consequence.** Genuinely platform-specific bindings need an explicit escape
hatch. None exist yet.

## ADR-009: The accent is one colour, everything else derives

**Context.** Each workspace has a visual identity that should be recognisable
peripherally without being loud.

**Decision.** A workspace stores a single hex colour. The frontend sets
`--accent`; every tint, line and glow derives from it with `color-mix`.

**Why.** Adding a workspace must never mean adding CSS, and a derived palette
cannot drift out of step with itself.

**Consequence.** Depends on `color-mix`, which both target webviews support.
Accents are validated as `#rrggbb` in the backend, because a malformed value
would break every derived surface at once.

## ADR-010: Sessions are owned by the backend and addressed by id

**Context.** A session must survive switching projects, and eventually must
survive the window closing and be renderable from a detached window.

**Decision.** `SessionManager` in `beacon-core` owns every PTY. Views never hold
a process — they attach to a session id, receive its retained output, and then
follow the live stream. The event sink is a trait, so the manager does not know
whether it is talking to a webview or a daemon transport.

**Why.** Every requirement that is still ahead of us — the daemon, detached
panels, more than one view of the same session — is the same requirement: the
process must not belong to whoever is currently looking at it.

**Consequence.** Output crosses a boundary as bytes and must be encoded (see
ADR-011). A view is cheap to destroy and rebuild, which is what makes project
switching and, later, popping a panel out, straightforward.

## ADR-011: Session output carries stream offsets

**Context.** Reattaching to a session means replaying a snapshot and then
joining a live stream. Naively, chunks that arrive between taking the snapshot
and subscribing are either lost or written twice — and a chunk can straddle the
boundary, so accepting or dropping whole chunks is not enough either.

**Decision.** The scrollback counts every byte it has ever seen. Each output
event carries the offset where its chunk starts, and a snapshot carries the
offset just past its contents. A client writes the snapshot, then trims each
incoming chunk to the part it has not consumed.

**Why.** It is the only version of this that is actually correct, and it costs a
`u64` per event. The alternatives all reduce to hoping the race does not happen.

**Consequence.** The offset is part of the event contract, so the daemon
transport must preserve it. Output is base64-encoded rather than sent as a
string: PTY bytes are not guaranteed to be valid UTF-8 at a chunk boundary, and
coercing them would corrupt escape sequences.

## ADR-012: Sessions are stopped by the backend, not by the UI

**Context.** Removing a project or deleting a workspace leaves its processes
with nothing to render them and no way to be reached again.

**Decision.** The commands that remove a project or a workspace stop its
sessions first, in the backend, before touching the configuration.

**Why.** If the UI were responsible for this, every future caller — the command
palette, a keyboard shortcut, the daemon's own cleanup — would have to remember.
Putting it behind the boundary makes an orphaned PTY unreachable by construction.

**Consequence.** Removal is no longer a pure configuration edit. That is the
right trade: the alternative is a leaked process per removed project.
