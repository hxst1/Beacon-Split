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

## ADR-019: Deleting means the trash

**Context.** The file tree needs a delete, and Beacon's whole promise about a
user's files is that it does not destroy them.

**Decision.** Delete moves the entry to the system trash. There is no operation
that removes a file outright. The menu item is separated from the rest, asks
first, and says "Recoverable".

**Why.** The cost of being wrong is unbounded, and a trash that the user can
open in their own file manager turns an irreversible mistake into an annoying
one. This is the same reasoning as ADR-006, applied to the one place Beacon does
touch the disk.

**Consequence.** A dependency on the platform trash, which behaves differently
on macOS and Linux and can fail on some volumes. A failure surfaces as an error
rather than silently falling back to a real delete.

## ADR-020: Every file path is confined to its project

**Context.** File commands take a path from the frontend. A bug, or a crafted
value, must not be able to reach outside the project.

**Decision.** Commands take a project and a path relative to it; absolute paths
and `..` are refused outright, and the resolved path is then checked against the
canonical project root. Resolution walks up to the deepest existing ancestor, so
creating a file still works.

**Why.** Checking the string is not enough: `a/../../b` and a symlink pointing
elsewhere both look innocent as text. Canonicalising first is what makes the
rule mean what it says — a test covers the symlink case specifically.

**Consequence.** A project that is itself a symlink resolves to its target, and
paths are compared against that. Every file operation pays one `canonicalize`,
which is not measurable next to the I/O it guards.

## ADR-021: Adding a panel repairs stored layouts instead of invalidating them

**Context.** The editor is a fifth panel. Layouts stored by earlier versions
place four, and the original rule required a layout to place every panel.

**Decision.** Validation requires only that no panel is placed twice and that
Claude is placed at all. A layout missing a panel is repaired on load: the panel
is attached beside Claude and starts hidden, and the repaired document is
written back.

**Why.** Refusing an arrangement somebody made because a new version added a
panel is the same failure as discarding their settings on a format change —
ADR-015 again. Hidden on arrival means nothing moves until they ask for it, and
an empty editor pane never takes up room.

**Consequence.** "Starts hidden" applies at the moment a panel is introduced and
not afterwards; once repaired, the layout is the user's. Adding a panel in
future needs only an entry in `PanelId::ALL`.

## ADR-022: `.env` values are read for one render and kept nowhere

**Context.** The `.env` view exists to get a secret onto the clipboard. That
means the value crosses the IPC boundary and is held in the UI.

**Decision.** The file is read fresh on every open, parsed in the backend, and
the entries live in the view's own state for as long as it is mounted — never in
the persisted store, never in a log line, never anywhere else. Values are masked
until asked for, one at a time. The commands that carry file contents are marked
so nobody adds logging to them later.

**Why.** The file is the only place these belong. Anything that caches them
turns one secret into two.

**Consequence.** Reopening the view re-reads the file, which also means it never
shows a stale value. Nothing derived from a value is stored, so there is no
search or filter over them.

## ADR-023: Git is read through `--porcelain=v1 -z`

**Context.** The Git panel needs status, and status output has to be parsed.

**Decision.** `git status --porcelain=v1 -z --branch --untracked-files=all`,
parsed by a pure function with its own tests, plus an integration suite that
runs real repositories.

**Why.** `--porcelain` is the format git promises not to change; the human one
is explicitly not. `-z` matters more than it looks: without it a path containing
a space, a quote or a newline comes back quoted and escaped, and every consumer
has to unescape it correctly. NUL separators make that whole class of bug
impossible. There is a test with a filename containing spaces.

**Consequence.** Renames arrive as two records rather than one line, which the
parser has to know about — also tested. The integration tests are what catch git
changing its output from under the unit tests' assumptions; the unstage case
before the first commit was found exactly that way.

## ADR-024: Git never gets to ask a question

**Context.** `push` and `pull` talk to a network and may want credentials. There
is no terminal attached to these commands.

**Decision.** Git runs with `GIT_TERMINAL_PROMPT=0`, empty askpass variables and
`GIT_PAGER=cat`. Push and pull run on the blocking pool rather than an IPC
worker. `pull` is `--ff-only`.

**Why.** A prompt with nowhere to appear is a hang, not a question, and a hung
command on an IPC worker takes the window's responsiveness with it. `--ff-only`
because starting a merge or a rebase from a side panel — with no way to see or
resolve a conflict there — leaves the repository somewhere the user did not ask
to be.

**Consequence.** A repository needing an interactive credential fails with git's
own message instead of hanging, which is the right trade. A pull that is not a
fast-forward is refused, and the terminal panel is where that gets sorted out.

## ADR-025: Beacon polls instead of watching the filesystem

**Context.** The Git panel and the file tree showed whatever was true when they
mounted. A file created in a terminal did not appear.

**Decision.** No filesystem watcher. Git status is re-read on a short interval
while the window is focused, and the file tree re-reads its open directories
when the window regains focus. Both have an explicit refresh.

**Why.** A recursive watch is cheap on macOS and expensive on Linux, where
inotify takes a watch per directory and a large `node_modules` can exhaust the
system limit — on the machine this has to run on. `git status` is milliseconds
on a normal repository, so polling it costs less than the machinery to avoid
polling it. The tree is focus-only because re-reading on a timer would fight
with scrolling and selection.

**Consequence.** Up to a couple of seconds of lag on git, and none of it while
the window is in the background. A watcher becomes worth revisiting if a
repository large enough to make `git status` slow turns up.

## ADR-026: Commands live in one registry

**Context.** The palette needs a list of everything Beacon can do, and the
keyboard layer needs the same list.

**Decision.** One registry, built on demand. The palette renders it; bindings
resolve against its ids.

**Why.** Two lists drift. A command with a shortcut and no palette entry is
undiscoverable; a palette entry that does something different from its shortcut
is worse. Building it on demand rather than at startup is what lets an entry
read "Hide Files" or "Show Files" depending on which is true right now.

**Consequence.** Making bindings user-editable is a settings surface over the
same registry rather than a rework, which is why it could be deferred without
painting us into a corner.

## ADR-027: Sessions live in a daemon, not in the window

**Context.** Closing Beacon killed every session it was showing. The whole
arrangement since Milestone 0 — `beacon-core` with no dependency on Tauri — was
for this.

**Decision.** `beacon-daemon` is a separate process that owns `SessionManager`.
The Tauri app holds a `DaemonClient` where it used to hold the manager, with the
same method shapes. The daemon starts on demand, detached with `setsid` so it is
not a child that dies with the app.

**Why.** Nothing else makes a session outlive the window. Keeping the client's
surface identical to the manager's is what kept the change to one layer: the
commands, the UI and the protocol for reattaching were already right.

**Consequence.** A second process to package, and a socket to keep compatible.
The daemon stops itself after five minutes idle so it does not accumulate, and
can be stopped from the palette. A packaged build still needs the binary added
to the bundle.

## ADR-028: The protocol is newline-delimited JSON, and versioned

**Context.** The window and the daemon have to agree on messages, across
versions that may be built weeks apart.

**Decision.** One JSON object per line over a unix socket. A `Hello` carries a
protocol version; a client meeting a daemon that speaks a different one asks it
to stop and starts one it understands.

**Why.** The traffic is small and the contents are worth being able to read with
`nc` when something is wrong. Version checking matters more than it looks: a
daemon left running from an older build answers in shapes the client
half-understands, and a half-understood session is worse than a new one.

**Consequence.** Two serde shapes bit us and both are now covered by tests over
every variant. A unit enum variant serialises without its content field and
`#[serde(flatten)]` cannot read it back — so requests carry bodies they do not
need. An internally tagged enum cannot hold a bare sequence, and serde only
finds that out at runtime — so the session list is a struct variant. Neither
failure was visible until a request timed out, which is why the daemon now
answers what it cannot parse and never drops a reply it cannot encode.

## ADR-029: The daemon ships as a Tauri sidecar

**Context.** A packaged Beacon has to carry its daemon; a build that produces an
application unable to start a session is not a build.

**Decision.** `externalBin`, with the binary staged as
`beacon-daemon-<target-triple>` before bundling. Tauri strips the triple and
places it beside the main executable, which is where the client already looks.

**Why.** The triple in the name is the point: it makes it impossible for a
bundle to pick up a daemon built for a different machine. Using it also means
the nested binary is signed with the bundle on macOS, which a plain resource
copy would not be.

**Consequence.** One staging step before bundling, and a `binaries/` directory
that is built rather than committed. The path resolution needed no special case
for bundles — verified by building one and looking, not by assuming.

## ADR-030: Only overridden shortcuts are stored

**Context.** Shortcuts became editable, which means deciding what a settings
file remembers.

**Decision.** `settings.json` holds only the bindings that differ from their
default. Binding an action to what it already was removes the entry rather than
writing it. The catalogue of bindable actions and their defaults lives in the
backend; what each one does lives in the frontend, keyed by the same ids.

**Why.** Writing out every default freezes them: change one in a later version
and nobody who never touched it would ever see the change. Keeping the catalogue
in the backend is what lets conflict checking be a single rule rather than two
implementations that can disagree — and a conflict names the action that already
has the shortcut, because silently leaving one of two bindings dead gives the
user no way to tell which.

**Consequence.** An action in the catalogue with no handler does nothing, which
would look like a broken shortcut. `missingHandlers` reports that where a
developer sees it. The palette lists dynamic commands too — switching to one
particular project — and those stay unbindable, since a binding has to mean the
same thing next week.

## ADR-031: Beacon does not correct the user's shell

**Context.** A session shows zsh's `%` end-of-line mark. It is emitted by the
user's own prompt configuration, and could be suppressed by setting
`PROMPT_EOL_MARK` when spawning.

**Decision.** Leave it. Beacon strips the launcher's per-process state
(ADR-016) and changes nothing else about the environment.

**Why.** The line between the two is the whole rule: state that belongs to
whatever started Beacon is not the user's choice, and their shell configuration
is. Once Beacon starts overriding one prompt setting because it looks untidy in
our window, a session stops being the shell they configured.

**Consequence.** Anything their shell does, it does here too. Where that is
undesirable it is theirs to change — `PROMPT_EOL_MARK=""` in this case.

## ADR-032: The daemon connection repairs itself

**Context.** `DaemonClient` connected once. If the daemon was restarted — for an
upgrade, or because someone stopped it — the window went permanently deaf, and
only restarting Beacon brought it back.

**Decision.** The connection is replaceable. Losing it drains everything waiting
for a reply rather than letting it time out, reports the detachment, and starts
a reconnect loop with a backoff that levels off instead of giving up. Coming
back raises a separate event, because the daemon on the other end may not be the
one that issued the session ids the window is holding — so every terminal view
is rebuilt and asks for its session again.

**Why.** A daemon whose whole purpose is to outlive the window is worth little
if the window cannot outlive the daemon. Retrying indefinitely is right for
something that may sit open overnight; giving up after N attempts would mean a
window that is alive but silently useless.

**Consequence.** Reconnecting is not resuming: sessions the old daemon held are
gone with it. What the window recovers is the ability to work, not the work. The
status bar says so while it is trying.

## ADR-033: The daemon socket is an argument, not a constant

**Context.** One socket path per user meant the test suite talked to the same
daemon as a running Beacon — and one test shuts the daemon down, so running the
tests could stop somebody's real sessions. It also made the tests fight each
other.

**Decision.** The daemon takes its socket directory as an argument and the
client takes a socket path. The defaults are unchanged, so nothing about normal
use differs; tests give each case a socket of its own under a temporary
directory.

**Why.** A test that can reach production state is not isolated, however careful
it is. Making the socket explicit fixes that at the root rather than by
sequencing tests around each other, and it makes a second, independent Beacon
possible — which is a reasonable thing to want.

**Consequence.** One more argument on two functions, and a wrong socket now
produces a second daemon rather than an error. That is the correct behaviour for
a path that names which daemon you mean.
