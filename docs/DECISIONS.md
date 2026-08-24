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

## ADR-013: Layouts are a binary split tree

**Context.** Beacon needs several arrangements of its panels — Claude left,
Claude right, a tall column beside it — plus a custom one, without becoming a
tiling window manager.

**Decision.** A layout is a tree of splits and panels. Each split has a
direction and a fraction. The presets are four such trees; a custom layout is a
fifth. The renderer recurses over the tree and knows nothing about which panel
is which.

**Why.** The alternative — named regions with panels assigned to them — needs a
new region set for every arrangement that is not a variation of the first one,
and his fourth preset already is not. A tree costs about the same code and
covers arrangements nobody has asked for yet.

**Consequence.** Split fractions live in the tree, so there is one place a size
is stored and one shape to migrate. A tree can express a layout that makes no
sense, so `validate` rejects any that drops or repeats a panel, and the backend
refuses to store one.

## ADR-014: Panel visibility is separate from the layout

**Context.** Toggling Files off and on again should put it back where it was,
not somewhere reasonable.

**Decision.** Hidden panels stay in the tree. The renderer prunes them just
before drawing, collapsing any split left with one child.

**Why.** Removing a panel from the tree loses the only record of where it
belonged. Reconstructing that on the way back is guesswork.

**Consequence.** The stored tree always contains all four panels, which is also
what makes `validate` a simple rule. Hiding every panel is ignored rather than
producing an empty window.

## ADR-015: Stored documents migrate rather than reset

**Context.** Moving to the layout tree changed the shape of `ui-state.json`.

**Decision.** `UiState` is loaded through a version-aware reader that upgrades
an older document — old panel fractions become the equivalent tree — and writes
the result back immediately.

**Why.** The layout is something the user arranged by hand. Discarding it
because the format moved is the kind of thing that teaches people not to trust
their settings. Writing back on load means the upgrade happens once rather than
on every launch.

**Consequence.** Each schema bump needs a migration path from the previous one.
That is the cost of the promise, and it is small while the documents are this
size.

## ADR-016: Sessions get a sanitised environment, not an inherited one

**Context.** Beacon inherits the environment of whatever started it. Launched
from a shell inside Terminal.app, that includes `TERM_PROGRAM=Apple_Terminal`
and `TERM_SESSION_ID`. macOS's `/etc/zshrc` reads those, decides it is resuming
a Terminal.app session, and sources `~/.zsh_sessions/$TERM_SESSION_ID.session` —
a file belonging to a different terminal, which may not even exist.

**Decision.** Spawned sessions have terminal-identity variables removed, and
Beacon declares its own: `TERM_PROGRAM=Beacon`. Stale geometry (`COLUMNS`,
`LINES`) and the `npm_*` group a package script injects are stripped too.

**Why.** A terminal emulator that claims to be a different terminal will hit
that terminal's integrations. The `npm_*` group is the same mistake in another
direction: launching Beacon through `pnpm app:dev` would otherwise push that
script's configuration into every project shell, so what a command does would
depend on how Beacon happened to be started.

Claude Code is the same problem from another direction. Beacon started from
inside a session inherits that session's markers, and the `claude` it launches
then sees `CLAUDE_CODE_CHILD_SESSION`, concludes it is nested, and turns
transcript saving off. The parent's messaging socket and token are a private
channel and have no business in a project shell either.

**Consequence.** The strip list is a denylist, so a new leak from a new launcher
would need adding to it. An allowlist would be stricter but would break the
user's own configuration, which is the environment we are here to preserve —
and that distinction is the whole rule: per-process state of the launcher is
stripped, configuration such as `ANTHROPIC_API_KEY` or `CLAUDE_CODE_USE_BEDROCK`
is passed through. That is why the Claude Code entries are listed individually
rather than matched as a `CLAUDE_*` prefix.

## ADR-017: `claude` is located through the user's interactive login shell

**Context.** A GUI application starts with a minimal PATH. Launching `claude`
by name would fail for anyone whose PATH is set up by their shell configuration
— which, with a framework or a version manager, is most people.

**Decision.** The path is resolved once, by asking the user's login shell
interactively, and cached. A non-interactive login shell is the fallback, and
Beacon's own PATH the last resort. The session then runs the resolved binary
directly.

**Why.** Only an interactive shell reads `.zshrc`, and that is where the PATH
usually comes from — on this machine a non-interactive login shell fails to find
`claude` at all. Running the resolved binary rather than launching through the
shell keeps whatever the user's startup files print out of the Claude panel.

**Consequence.** One subprocess on first use, running the user's full
interactive init. The shell prints more than the answer — a themed prompt writes
a terminal title escape onto the same line — so the probe marks its answer and
the marker is what is parsed, never the bare line.

## ADR-018: Activity is derived from the session stream, not from output

**Context.** Tabs should show what a project is doing: working, idle, a dev
server, an error.

**Decision.** Working, idle and stopped are derived from session events —
whether a project has a live session, and whether it printed anything recently.
`dev server` and `error` are not implemented.

**Why.** The first three follow from facts Beacon already has. The other two
require understanding what was printed, and a regex over terminal output would
be wrong often enough to be worse than showing nothing.

**Consequence.** Sessions are keyed by id in the activity store rather than
counted per project: opening a session is idempotent on the backend, so a
counter would drift every time a panel remounted.
