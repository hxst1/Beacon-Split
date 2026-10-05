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

**Amended (0.5.2).** Focus was the wrong signal for the case Beacon exists for.
A Claude writing a file, or a shell creating one, is *inside this window* — so
the window never loses the focus it would need to get back, and the tree stayed
wrong until someone clicked away and returned or pressed refresh. It now also
re-reads when the project's own sessions report something: Claude Code saying
it started a tool or stopped, and any session writing to its terminal. Neither
is a filesystem event and neither proves a file was created, but between them
they cover every way a file appears without anyone leaving the window, and a
directory listing is cheap enough to be wrong about. Reports are coalesced —
they settle for 400ms, and cannot hold the tree back for more than two seconds
— so a streaming turn costs one re-read rather than hundreds. The decision above
stands: still no watcher, and still nothing on a blind timer.

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

**Consequence.** The version has to move when a message is *added*, not only
when one changes shape — the case that is easy to forget, because an older
daemon simply rejects the new request and the check that exists to replace it
never fires. That happened once; the round-trip tests now assert how many
messages there are, so adding one fails until the version moves with it.

Two serde shapes bit us and both are now covered by tests over
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

## ADR-034: Activity comes from Claude Code, not from reading its output

**Context.** Milestone 3 left `dev server` and `error` unbuilt because inferring
them from terminal output is guesswork. The state that actually matters —
Claude stopped and waiting for an answer — was not available at all.

**Decision.** Claude Code's hooks report it. `PermissionRequest` and
`Notification` mean waiting, `PreToolUse` means working and names the tool,
`Stop` means finished. A report always beats the output heuristic, which stays
as the fallback for sessions with no hooks installed.

**Why.** These are facts Claude Code already knows and is willing to tell us. A
regex over terminal output would be a worse answer to a question that has an
exact one.

**Consequence.** The states depend on hooks being installed, so the heuristic
has to remain. Only events that change what someone would *do* about a project
get a state: every hook is a process Claude has to start, and a tab that
flickers through six states per turn tells you less than one that does not.

## ADR-035: The daemon binary is the hook

**Context.** The hook has to be a command Claude Code can run, present wherever
Beacon is.

**Decision.** `beacon-daemon hook`. It reads the event from stdin, finds
`BEACON_SOCKET` and `BEACON_PROJECT` in its environment, writes one line, and
exits zero — always zero, whatever happened.

**Why.** No extra file to ship, sign or keep beside the app; the daemon is
already there and already knows the protocol. Always exiting zero matters more
than it looks: a hook that fails is a hook that interferes with someone's work,
and knowing what a tab is doing is not worth that.

**Consequence.** A hook that cannot reach the daemon says nothing rather than
complaining. That is also what makes it safe to register once, globally: outside
Beacon there is no socket in the environment, so it exits immediately.

## ADR-036: Beacon asks before editing another application's configuration

**Context.** The integration works by adding hooks to `~/.claude/settings.json`,
which belongs to Claude Code, not to Beacon.

**Decision.** Never on startup, never silently. A settings section shows the
exact command that would be registered, installs it when asked, and removes it
cleanly — including the empty scaffolding it created. Everything else in the
file is preserved, and it is written atomically.

**Why.** The same line as ADR-031: Beacon strips what belongs to whoever
launched it and changes nothing else. A useful feature is not a licence to edit
files the user did not point us at. A truncated `~/.claude/settings.json` would
break Claude Code everywhere on the machine, not only here.

**Consequence.** The feature is off until asked for, and installing it is
visible rather than something that happened. Hooks pointing at a Beacon that
moved are reported as stale rather than as missing, since one needs installing
and the other needs replacing.

## ADR-037: Usage comes from the status line, and Beacon lends the slot back

**Context.** How much of the five-hour allowance is left, and how full a
context is, are the two numbers that decide what to work on next. Neither is
written to disk anywhere: Claude Code reports them only through its status line.

**Decision.** Beacon registers as the status line and runs whatever was already
configured, passing the payload through and printing that command's output. What
Claude Code shows does not change. Removing the integration restores the
previous command exactly.

**Why.** A status line is one slot, not a list like hooks. Taking it outright
would mean removing something the user had in exchange for a feature they asked
to add. Delegating costs one subprocess per render and keeps the trade honest.

**Consequence.** Both halves of the integration are installed separately,
because they cost different things — hooks are additive, the status line is
displacement. A user with no status line gets a plain default line rather than
an empty one.

## ADR-038: A report is a fact with a date on it

**Context.** Hooks and the status line only speak while a session is healthy.
Claude Code signing someone out mid-session, or crashing, means the reports
simply stop — and the last thing said would otherwise stand indefinitely.

**Decision.** Reports carry when they were heard. A `working` state with no
output and no report behind it for two minutes falls back to inference. Usage
older than fifteen minutes is dimmed, labelled with its age, and its countdown
is dropped. `waiting` and `done` do not expire, because both are states a
session legitimately sits in for hours.

**Why.** A stale number is worse than no number: "62% of your allowance left" is
exactly what somebody would plan an afternoon around, and a tab claiming to be
working on something it abandoned an hour ago is a lie the interface is telling
confidently.

**Consequence.** The interface says less when it knows less, which is the
intended trade. Old numbers are shown greyed rather than hidden — what they said
is still worth something, as long as it does not claim to be current.

## ADR-039: Modules do not subscribe on import

**Context.** The usage store called `watchActivity` at module scope, so
importing it opened a Tauri event subscription. A test that only wanted the
arithmetic could not load the file.

**Decision.** Each module exports a `start…` function returning its
unsubscribe, and the application calls them once, in one place.

**Why.** A module that reaches for a transport as a side effect of existing
cannot be used anywhere that transport is absent — a test, a different frontend,
a future headless build. It was a test that surfaced this, which is the argument
for having written it.

**Consequence.** One more line at startup, and subscriptions that can be torn
down rather than lasting as long as the module system does.

## ADR-040: Selectors must return stable references

**Context.** A release build opened a window and painted nothing. The backend
was fine behind it, the stylesheet had loaded, and `#root` was empty.

**What it actually was.** React error #185 — an infinite render loop. Several
Zustand selectors built their value on the way out: `s.snapshot?.bindings ?? []`
hands back a fresh array every call, and `s.missing.filter(...)` always does.
Zustand compares with `Object.is`, concludes the state changed, re-renders, and
never stops. React unmounts the whole tree, which is what an empty `#root` with
working styles looks like.

**Decision.** Selectors return references that only change when the data does:
stable module-level empties for the absent case, and any filtering done in the
component rather than in the selector. Tests assert that calling a selector
twice with the same state returns the same reference.

**Why it took so long to find.** A controlled experiment pointed at
minification — unminified it rendered, minified it did not — and that was true
but not the cause. Minification only changed how the loop failed: minified, it
exhausted memory and macOS killed WebKit's content process; unminified, React
threw first. A reproducible experiment can still support the wrong conclusion,
and this one cost an evening. Minification is back on.

**Consequence.** The frontend now reports uncaught render errors to the backend
log. A release build has no inspector to open, and a tree that unmounts itself
says nothing about why — this is the difference between a blank window and one
that names its own error.


## ADR-041: Two palettes, one design

**Context.** Beacon shipped dark-only, with colours written into forty places
across the stylesheets.

**Decision.** Every surface, line and text colour is a token, defined twice —
once per palette — with everything else shared. A light theme is not a second
design: the material inverts, so surfaces darken the window instead of
lightening it, and the structure is untouched.

**Why.** The whole look is built out of translucency over the window rather than
flat fills, which is exactly what makes inverting it work. Had the palette been
opaque blocks, a light theme would have meant redrawing the interface.

**Consequence.** Two things draw their own colours and cannot read CSS: xterm
paints to a canvas, and CodeMirror compiles a theme into a stylesheet when a
view is created. Both are rebuilt when the palette changes, which means the
resolved theme has to be state something can watch — hence `resolvedTheme` in
the store rather than only on the document.

## ADR-042: Translucency and blur are settings, not decisions

**Context.** How much of the desktop shows through a window is taste. Beacon had
one answer baked in.

**Decision.** Two numbers in settings, applied as CSS variables: the window's
own opacity and the backdrop blur. They move live as the slider is dragged and
are written to disk when it is released.

**Why.** Nobody knows what translucency they want in the abstract — you find it
by moving it and looking. A control that only shows its result after a round
trip is a control you have to guess with. Writing on release keeps the file
quiet: a drag is one save, not sixty.

**Consequence.** Opacity is floored at a half and blur capped, in the backend
rather than only in the slider: a window nobody can read is not a preference,
and a settings file is editable by hand. `clamp` alone returns NaN unchanged, so
non-finite values are refused before they can reach `blur(NaNpx)`.

## ADR-043: A session is identified by project, kind and slot

**Context.** One terminal per project is not enough the moment something is
running in it: a dev server holds a terminal, and stopping it to run a test is
exactly the friction Beacon exists to remove.

**Decision.** Sessions are keyed by project, kind and a slot number. Claude
uses slot zero and only ever has one; terminals can have several. Slots are
numbers rather than names, and the tabs show their position.

**Why.** A name is a thing to invent, and a second terminal is usually "the
other one" rather than anything worth titling. The slot also survives a
restart, which is what lets a reattaching window find the right session rather
than a session of the right kind.

**Consequence.** The protocol carries a slot, so the version moved with it.
Which slots are open is window state rather than something persisted: the
sessions themselves survive, and reopening a project gives one terminal until
another is asked for.

## ADR-044: The shell is a setting; kitty is not

**Context.** "Can I use kitty?" is a reasonable question with a confusing
answer.

**Decision.** Beacon runs a shell, chosen in settings, defaulting to the
account's own started as a login shell. It does not run another terminal
emulator.

**Why.** Beacon *is* the emulator — xterm.js drawing a PTY is what kitty does.
Running kitty inside it would be an emulator inside an emulator. What people
actually want from the question is their shell: fish rather than zsh, or a
specific build of one. That is what is configurable.

**Consequence.** Anything specific to another emulator — kitty's graphics
protocol, its keyboard protocol — is not available, and saying so is better
than a setting that appears to offer it. Arguments are configurable alongside
the program, because "login shell" is `-l` for zsh, bash and fish and nothing
at all for nushell.

## ADR-045: The client sends the shell, the daemon does not read it

**Context.** The daemon spawns sessions; settings live with the application.

**Decision.** The shell travels with the request to start a session.

**Why.** A session should start with the shell configured now. A daemon that
read settings when it started would keep serving the old one until it was
restarted — and the daemon is deliberately long-lived, so that could be days.

**Consequence.** The protocol carries a little more, and the daemon stays free
of any opinion about where configuration lives, which is what lets it be the
thing that outlives everything else.

## ADR-046: A save is refused if the file moved underneath it

**Context.** Beacon exists to work beside Claude, and Claude edits files that
are open in the editor. Saving a buffer was an unconditional write, so it would
overwrite Claude's work with text read minutes earlier and say nothing.

**Decision.** Reading a file returns a revision — modification time mixed with
size — and writing sends back the one it was working from. A file that has
moved since is refused, and the editor says so with both ways out: take theirs,
or keep mine. An explicit overwrite sends no revision at all.

**Why.** This is the likeliest data loss in the application, and it is silent.
Of the two things it could do without asking, reloading loses your typing and
overwriting loses Claude's — so it asks, and only when there is something to
lose: a clean buffer just reloads.

**Consequence.** Time and size together rather than time alone: filesystem
timestamps have a granularity, and a same-length edit inside that window is
exactly what an editor must not miss. After a successful write the revision is
re-read, since otherwise the next save would be refused against a stamp we made
obsolete ourselves. A file deleted underneath counts as changed — recreating it
silently is not what saving meant either.

## ADR-047: A notification is an interruption, so it is narrow

**Context.** Knowing that Claude is waiting is worth little if it stays inside a
window nobody is looking at.

**Decision.** A system notification when a project starts waiting, once per
wait, and not when that project is already on screen in a focused window. It can
be turned off, and permission is asked the first time there is something to say
rather than at startup.

**Why.** An interruption that was not worth it teaches people to dismiss the
next one without reading, and the next one might be the one that mattered. The
tab is already pulsing for the project you are looking at; the notification is
for the two you are not.

**Consequence.** Anything other than `waiting` clears the record, so a second
wait notifies again. A permission prompt before the application has done
anything is a prompt about nothing, so it waits.

## ADR-048: Floating layers are portalled out of the interface

**Context.** Menus opened from the title bar were visible and completely inert:
clicking an item did nothing, so a workspace could not be deleted.

**Decision.** `Popover` renders into `document.body` through a React portal
rather than where it is written.

**Why.** The title bar has a `backdrop-filter`, and that makes an element both
the containing block for `position: fixed` descendants and a stacking context.
A menu left in place was therefore confined to a 42-pixel-tall bar and painted
inside the header's stacking context — so the panels below it, later in the
document, drew on top. It looked correct and every click landed on whatever was
above it.

The original code said "deliberately not a portal library", which confused a
dependency with the platform. A portal is part of React; what a library would
have added is positioning logic that fits in ten lines.

**Consequence.** Anything floating belongs outside the interface it floats over.
Worth remembering that `backdrop-filter` — used on most of Beacon's chrome — has
this effect at all: it is not a paint-only property.

## ADR-049: No native vibrancy behind the window

**Context.** Lowering the opacity slider appeared to do very little.

**Decision.** The macOS window effect is gone. What is behind the window is the
desktop, and `--window-alpha` and `--blur-px` are the only things between them.

**Why.** `underWindowBackground` paints a frosted sheet of its own behind the
webview, so making the webview more transparent revealed *that* rather than
anything underneath — a control with nothing to control. Having made
translucency a setting, it has to be the setting that decides it.

**Consequence.** A window at full opacity looks the same as before; below that
it now behaves the way the slider claims.

**Superseded in part by [ADR-050](#adr-050-frosting-is-a-window-effect-and-therefore-a-switch).**
The claim that Beacon could draw its own blur instead was wrong, and removing
the window effect left the blur setting with nothing to act on. Translucency
still belongs to `--window-alpha`; frosting has gone back to the window server,
where it is the only place it could ever have worked.

## ADR-050: Frosting is a window effect, and therefore a switch

**Context.** The blur slider did nothing at any position. Opacity worked.

**Decision.** `Appearance.blur`, a radius in pixels, becomes
`Appearance.frosted`, a boolean. It is applied by the shell as a macOS window
effect rather than by CSS, and the three `backdrop-filter` rules that used to
read the old setting are gone.

**Why.** A backdrop filter composites what is behind an element *within the
page*. Behind Beacon's chrome is `--surface-1`, a flat colour, and blurring a
flat colour returns the same flat colour. It could never have reached the
desktop: the backdrop is built from the document, and a transparent window is
not a hole the filter can see through. Only the window server can blur what is
behind a window, and it does not expose a radius — it picks the material. An
amount was therefore a control with one working position and forty that lied,
and a switch is the shape the platform actually offers.

**Why it composes with opacity, which [ADR-049](#adr-049-no-native-vibrancy-behind-the-window)
said it would not.** `underWindowBackground` frosts what is behind the window
without tinting it as a panel material would, so the two settings answer
different questions: opacity is how much comes through, frosting is whether it
arrives sharp. The mistake in ADR-049 was treating the effect as a competitor
to translucency rather than as what translucency reveals.

**Consequence.** An old configuration's `blur` number still loads, and anything
above zero means it was meant to be on. Linux has no window effects, so the
switch will do nothing there until it is given something to do — which is worth
knowing before it is shipped, and is why the field says so.

**Applied without `WebviewWindow::set_effects`,** which is wrong for this twice
over. On macOS it only ever adds: handing it `None` runs a branch that exists
for Windows alone, so an effect applied at startup can never be taken off and
the switch is dead in one direction. And it drops the result inside a closure
it schedules on the main thread, so the `Result` it returns says the work was
queued rather than that it worked — which is how a log with no warning in it
was read as confirmation of something it had never been asked. `window-vibrancy`
is called directly instead, in both directions, and the outcome is logged.

## ADR-051: A terminal and its process are resized together, or neither is

**Context.** The Claude panel would collapse into a narrow, garbled column and
stay there. The daemon's log said the process had the right size, and it did:
142 columns, correct, at the same moment the panel was unreadable.

**Decision.** `nextGrid` decides on a measurement before anything is applied,
using the fit addon's `proposeDimensions` rather than `fit`. Either both the
grid and the process are resized, or neither is.

**Why.** `fit()` resizes the terminal and *then* reports what it chose. The
guard against implausible measurements ran after that, so a panel caught
mid-layout left xterm two cells wide while the process kept its real width. The
process was never told anything had changed, so no redraw ever came to repair
it, and full-width output went on being folded into two columns until the
terminal was rebuilt from scratch. The guard was protecting the wrong side of a
pair that has to agree.

**Consequence.** The policy is a pure function in `features/terminal/grid.ts`
and is tested against every way a measurement has gone wrong so far — a
collapsing panel, a hidden one, and a cell measured as zero, which divides into
infinity rather than into something small and so slips past a range check.

## ADR-052: Beacon never touches a Claude credential

**Context.** Beacon runs Claude Code, which puts it inside the conditions
Anthropic sets for products that do.

**Decision.** Beacon starts the unmodified `claude` on the user's `PATH` and
stops there. It has no account system, no server, no login screen, and no code
path that reads, stores, forwards or proxies a Claude credential or session
token. Every authentication variable the user has set — `ANTHROPIC_API_KEY`,
`ANTHROPIC_BASE_URL`, the Bedrock and Vertex switches — is passed through
untouched, and the environment Beacon strips is per-process state only, listed
one variable at a time rather than by a `CLAUDE_*` prefix, precisely so that
nothing to do with signing in can be caught by accident.

**Why.** Anthropic's terms for running Claude Code in another product are
specific: the binary must not be modified, no authentication method may be
removed or restricted, and each end user must authenticate themselves. A
developer may not offer Claude.ai login inside their own application, nor
collect, store or intermediate credentials. The architecture that satisfies all
of that is the one Beacon already had for other reasons — it is a terminal, and
the session inside it is the user's own.

**Consequence.** The remote access on the roadmap has a hard boundary drawn
around it before it is built: reaching your own machine from your own phone is
one person using their own subscription, and putting a signed-in Beacon
somewhere a colleague can use it is not — that is account sharing, whoever pays
for the server. Anything shared has to be each person running their own Beacon,
signed in as themselves.

**Also.** The product, its name and its icon carry nothing of Anthropic's.
Naming what a panel contains is saying what it runs, which is allowed; naming
the product after it would not be.

## ADR-053: Claude gets a tool, not another hook

**Context.** Some of what Claude produces is not meant for the terminal at all.
An environment variable, a command to run on another machine, the body of an
email — you read it once and paste it somewhere else. Selecting it out of a PTY
is fiddly and it is easy to take a line too few.

**Decision.** Beacon exposes one MCP tool, `save_clip`, which files a titled
piece of text in a drawer with a copy button. Hooks were not extended to cover
it.

**Why.** A hook is reactive: Claude Code fires it on a lifecycle event and
Claude has no say in it. Nothing Claude *decides* to do can ever travel that
way, and deciding is the whole feature — only Claude knows that what it has
just written is meant to leave the conversation. MCP is the only channel where
the model initiates, with arguments, and hears whether it worked.

**Consequence.** It is not deterministic. Claude calls the tool when it
recognises the moment, and "write me an email" will not always reach the drawer
unless asked. The tool description is therefore load-bearing, and is written to
name the artefacts — a variable, a command, an email — rather than the occasion.

## ADR-054: The MCP server is passed per session, never installed

**Context.** Beacon already writes to `~/.claude/settings.json` to register its
hooks, and asks before doing it (ADR-036). The clip tool could have been
registered the same way.

**Decision.** It is not registered anywhere. Beacon starts each Claude session
with `--mcp-config <its own file>`, written beside the daemon socket in the
per-user temporary directory.

**Why.** There is nothing to install, so there is nothing to uninstall, nothing
to go stale when Beacon moves, and no way for a Beacon that was deleted to leave
an entry behind pointing at a binary that is gone. A `claude` the user runs in
their own terminal is completely unaffected — which is also the honest scope of
the feature, since a clip has nowhere to go if no window is showing.

**Also.** `--strict-mcp-config` is deliberately not passed. It would switch off
every MCP server the user configured themselves, which is not a trade Beacon
gets to make on their behalf in exchange for a drawer. And the flag is written
joined, `--mcp-config=…`, because it takes a list and the separated form would
swallow whatever argument came after it.

## ADR-055: The drawer is write-only

**Context.** Claude could as easily read the drawer as write to it — "take the
third clip and rewrite it" is an obvious next request.

**Decision.** There is one tool and it only files clips. Nothing in the protocol
lets a session read the book back, and the Tauri layer has no command to add
one.

**Why.** Clips are drafted email, API keys and environment values by
construction. A tool that reads them turns every page Claude fetches and every
repository it reads into a possible instruction to send the contents somewhere —
the drawer would become the most concentrated exfiltration target in the app,
in exchange for a convenience. Write-only closes it completely rather than
mitigating it.

**Consequence.** Reworking a clip means asking Claude for a new one. That is a
worse workflow and an acceptable price.

## ADR-056: The daemon owns the clip book, and is its only writer

**Context.** Clips have to survive the window closing, and two windows can be
open at once.

**Decision.** The daemon holds the book, writes `clips.json` on every change,
and answers `Clips` and `ForgetClips`. Windows display what they are told and
ask for removals; the frontend never writes the file.

**Why.** One writer means no merge, no last-write-wins, and no window that has
been open all day quietly overwriting a clip filed a minute ago by another.
Every change is broadcast as the whole book rather than as a delta, because it
is small and a drawer rebuilt from the truth cannot drift from one that missed
an event.

**Consequence.** The daemon now writes to Beacon's configuration directory,
which the tests had no way to redirect. `BEACON_CONFIG_DIR` was added for the
same reason the socket became an argument (ADR-033): a test that can rewrite the
real clip book can delete somebody's work.

## ADR-057: Finishing is announced only when the turn was long

**Context.** Knowing that Claude is waiting was never the whole problem. With
the window behind a browser, the other thing worth knowing is that a long turn
has finished — and the `Stop` hook already reports it.

**Decision.** A finished turn is announced, but only when Claude had been
working for at least thirty seconds. The clock starts at the turn's first
`working` report and is not stopped by a permission prompt.

**Why.** `Stop` fires at the end of every turn, and most turns are seconds
long — you were watching those, and announcing them would teach you to dismiss
the next notification without reading it, which costs the waiting ones too.
Duration is the only honest proxy for "you left" that does not require watching
where the user is looking. Thirty seconds is roughly the point past which people
switch windows.

**Consequence.** A turn that ends just under the threshold says nothing, and the
tab pulse remains the only signal for it. The alternative — announcing
everything and letting the user filter — degrades the notification that matters
most.

## ADR-058: A notification points at a project, it does not open one

**Context.** The obvious next step from a notification is landing on the project
it names.

**Decision.** Notifications name the workspace and project and do nothing on
click beyond what macOS does for free, which is bringing Beacon to the front.

**Why.** Reacting to a click needs a notification-response delegate registered
with `UNUserNotificationCenter`, which means owning an Objective-C delegate
class for the lifetime of the process — a large piece for a small gain. The
available substitute — switching to the last announced
project when the window regains focus — would move the user's project under them
whenever they came back for an unrelated reason. Guessing wrong here is worse
than not guessing.

**Consequence.** The title carries the routing instead: `workspace › project`,
because with the same repository open in two workspaces the project name alone
does not say where to look.

## ADR-059: macOS notifications bypass the plugin entirely

**Context.** Notifications never arrived on macOS 26, and Beacon had no way to
know. `dev.beacon.split` was not among the applications registered with the
notification centre at all, while the application believed every notification
had been delivered.

**Decision.** On macOS, Beacon talks to `UserNotifications.framework` directly
through `objc2-user-notifications`: it reads the real authorisation state,
raises the system prompt once, and posts through `UNUserNotificationCenter`.
`tauri-plugin-notification` stays, scoped to every other platform.

**Why.** The plugin cannot do either thing that matters here. Its desktop
implementation returns `Granted` from both `permission_state()` and
`request_permission()` without consulting the system, so an application using it
cannot distinguish being allowed from being silenced — and it posts through
`NSUserNotificationCenter`, deprecated in macOS 11, which never raises an
authorisation prompt. An application that has never been authorised and never
asks is an application macOS has no reason to deliver for.

**Consequence.** Four Objective-C dependencies on macOS, and one behaviour that
has no equivalent elsewhere: an unbundled build reports `unavailable` rather
than a permission. That is not an evasion — `tauri dev` runs the binary straight
out of `target/`, with no bundle identity for macOS to attribute a notification
to, so the honest answer is that there is nothing to ask about. Testing this
feature means installing a build.

## ADR-060: The application bundle is signed, even if only ad-hoc

**Context.** Releases shipped with whatever signature the linker left behind:
identity `beacon_split-<hash>` rather than `dev.beacon.split`, `Info.plist` not
bound, and no sealed resources.

**Decision.** `bundle.macOS.signingIdentity` is `-`, so `tauri build` signs the
bundle ad-hoc as a bundle.

**Why.** macOS keys a notification authorisation — and everything else in TCC —
to the code signature, not to the path. A bundle whose signature does not seal
its `Info.plist` has no stable identity to grant anything to, which is the
difference between a permission that survives the next update and one that has
to be granted again, or cannot be granted at all. An ad-hoc signature is enough
for that; a Developer ID would additionally remove the Gatekeeper warning on
first launch, and is a separate decision with a separate price.

**Consequence.** The identity is stable across builds from the same
configuration, so an allowed Beacon stays allowed. Changing the bundle
identifier would still read as a different application to macOS, and would cost
the user their answer.

## ADR-061: A file is replaced, never truncated and refilled

**Context.** Saving called `fs::write`, which truncates the file and then writes
the new contents into it. Between those two steps the file on disk is short or
empty, and a crash, a full disk or a lost power cable in that window leaves the
user with a fragment and no copy of what was there. Beacon then read the new
revision back in a second IPC call, so anything that touched the file between
the write and the stat became the stamp Beacon believed was its own.

**Decision.** Writes go to a temporary file in the same directory, take the
original's permissions, are flushed to disk, and are renamed over the target.
The revision is read inside the same call and returned with the outcome.

**Why.** A rename within one filesystem is atomic: a reader sees either the old
file or the new one, never a half-written one. Carrying the permissions over
matters because the rename would otherwise hand the file the temporary's, and a
saved shell script or git hook would come back not executable. Returning the
revision with the write closes a window that was milliseconds wide and could
silently license the next save to overwrite Claude's work.

**Consequence.** Saving costs one extra file creation and an `fsync`. Editing a
file whose directory is not writable now fails at save time rather than at
write time, which is a clearer failure than a truncated file.

## ADR-062: The text on screen lives in the store, not in the editor

**Context.** Buffers were held only by the mounted CodeMirror instance, and that
instance is destroyed constantly: switching tabs, hiding the editor panel, going
fullscreen on another panel, switching project. Everything typed since the last
save went with it. A separate `dirty` flag survived, so a tab could claim
unsaved changes for text that no longer existed anywhere — and "Keep mine",
reading a single ref shared by every tab, could write one file's text into
another, or truncate a file to nothing.

**Decision.** Each open file carries its `draft` — what is on screen — beside
`saved`, what is believed to be on disk. CodeMirror is seeded from the draft and
reports every change back to it. Dirtiness is `draft !== saved`, derived rather
than tracked.

**Why.** The view is disposable and the text is not. Deriving dirtiness removes
a whole class of bug on its own: a flag kept next to the text can disagree with
it, and every one of those disagreements is a way for work to go missing —
including the case where a save completes while the user is still typing, and
the tab reports clean for a buffer that never reached the disk.

**Consequence.** Undo history and cursor position are still lost when the editor
is unmounted; the text is not, which is the part that cannot be reconstructed.
Beacon holds one copy of each open file in memory, bounded by the same 2 MB
limit that decides what is editable at all.

## ADR-063: Quitting is the one thing Beacon asks about

**Context.** Beacon deliberately has no confirmation dialogs. Removing a project
leaves the repository alone, trashing a file is recoverable from Finder, and the
hint says so at the point of decision rather than in a modal. But quitting with
unsaved buffers discarded them without a word, and nothing else in the
application can put them back.

**Decision.** Closing the window is intercepted while any open file has a draft
that is not on disk. Beacon names the files, and offers to save them all, to
quit anyway, or to stay.

**Why.** The rule was never "no dialogs"; it was "do not ask about things that
are not destructive". Quitting is the exception that proves it — it is the only
action in Beacon that destroys work existing nowhere else, and it cannot be
undone afterwards from Finder or from git.

**Consequence.** A save that is refused because the file moved on disk does not
quit: the window stays, and the tab says what happened. Quitting is otherwise
untouched, and a session with nothing unsaved never sees this.

## ADR-064: A conflict is shown, not quietly resolved

**Context.** Unstaging ran `git restore --staged`. On an unmerged path that
exits 0 and leaves the file as an ordinary modification — git considers the
conflict resolved — while `<<<<<<<`, `=======` and `>>>>>>>` are still in the
file. It was then one click from being committed. `stage_all` had the same
effect through `git add --all`. Separately, a conflicted file was listed as both
staged and unstaged, so it appeared twice with opposite buttons.

**Decision.** Conflicted paths have their own section and are refused by stage,
unstage and stage-all. Staging one is allowed only once its markers are gone,
which is what "mark resolved" means. Committing is disabled while any conflict
remains.

**Why.** Beacon deliberately does not resolve conflicts — the terminal is better
at it, and that is already written down. But not doing something is different
from offering a button that appears to do it and instead destroys the evidence
that it needs doing. The failure was silent, produced a commit that compiles
nowhere, and was reachable by accident from the button next to it.

**Consequence.** Resolving still happens in the terminal or the editor. Beacon's
part is to show that a conflict exists, keep it out of the way of everything
else, and accept it once it is genuinely resolved.

## ADR-065: Every git invocation has a deadline

**Context.** Git commands ran without a timeout. A commit whose hook runs the
test suite, a push whose credential helper stalls despite `GIT_TERMINAL_PROMPT`,
or a signing commit waiting on a pinentry that has no terminal to draw on, all
left the panel with every control disabled and no way out but restarting Beacon.

**Decision.** Everything goes through one runner: 30 seconds for local commands,
180 for commit, push and pull. Output is drained on its own threads, the child
is killed when the deadline passes, and the message names the usual causes.
Auto-housekeeping is switched off so nothing git forks can outlive the command
and hold its output pipe open past that deadline.

**Why.** A hang is worse than a failure. A failure says what happened and leaves
the application usable; a hang looks like a bug in Beacon and costs the user the
window, including whatever else was running in it.

**Consequence.** A genuinely slow hook can be stopped at three minutes, which is
the wrong answer for someone whose pre-commit suite is slower than that. The
message says to run it in a terminal, which is where a commit that takes that
long belongs anyway.

## ADR-066: Beacon chooses the conversation, rather than finding out which one it got

**Context.** A Claude session in Beacon had no identity beyond "the Claude of
this project". Naming, resuming and forking all need one, and the obvious place
to find it is Claude Code's own transcript directory — which Anthropic documents
as internal and free to change.

**Decision.** Beacon generates the conversation's UUID and hands it to Claude
Code with `--session-id`, on a new session and on a fork alike. The id is known
before the process exists. Nothing reads a transcript, ever.

**Why.** The alternative is discovering afterwards what Claude picked, which
means either parsing a private file format or waiting for a report before the
session can be addressed at all. Choosing it removes both problems and a whole
class of state: there is no "pending, id unknown" for anything to handle.

**Consequence.** It only works on a Claude Code that has the flag. Everything
built on it is behind a capability check and simply is not there otherwise, so
an older install keeps exactly the behaviour it had.

## ADR-067: A conversation exists when something has been said in it, not when a process starts

**Context.** Beacon recorded a workstream as started the moment it spawned a
Claude with that id, and used that to decide between `--session-id` and
`--resume`. Driving the real CLI showed the flag was wrong in both directions:
a session opened and never typed into answered *"No conversation found with
session ID"* on resume, while one that had had a single turn refused
`--session-id` with *"already in use"*.

**Decision.** The flag means "Claude Code has a conversation under this id", is
called `resumable`, and is set only by proof from inside the session: a hook
event that can only have happened during a turn, or a status line report showing
tokens in the context window. A session merely opening is explicitly not proof.

**Why.** Claude Code writes nothing until the first exchange. Two states that
look identical from outside — a process that started, and a conversation that
exists — are on opposite sides of the boundary that decides which flag works.

**Consequence.** The proof arrives through Beacon's own hooks or its status
line. Without either installed, a workstream opened and abandoned before its
first turn can be asked to resume and will say so in the terminal. Both are one
click away in Settings, and both are what the rest of the integration already
depends on.

## ADR-068: Capabilities are read from Claude Code, not from a table of versions

**Context.** Nearly everything in the cockpit work rests on a flag or an event
that arrived at some point in Claude Code's history. The usual way to handle
that is a minimum version per feature.

**Decision.** Beacon runs `claude --help` and `claude agents --help` once and
reads the flags out of what they print. No version thresholds.

**Why.** A table of minimum versions is a list of guesses about when each flag
landed. Wrong entries fail silently — a feature quietly stops being offered, or
is offered and breaks — and nobody finds out for months. The help text is the
program describing itself, and it cannot be out of date.

**Consequence.** Three short processes on the first session, cached for the life
of the daemon. Things no help text lists — which hook events exist, whether a
task list id is honoured — are deliberately absent from the capability set; the
honest test for those is whether anything ever arrives.

## ADR-069: Subagents are passed per session, never installed

**Context.** Beacon offers three subagents. They could be written into
`~/.claude/agents/`, into each project's `.claude/agents/`, or handed to each
session with `--agents`.

**Decision.** `--agents`, per session, exactly as ADR-054 does for the MCP
server. Nothing is installed, nothing needs uninstalling, a deleted Beacon
leaves no trace, and a Claude the user starts in their own terminal is
unaffected. Confirmed against the real CLI: `--agents` merges with the agents
already configured rather than replacing them.

**Why.** Writing into a project would put Beacon's opinion into a repository
that belongs to somebody else and probably into their git history. Writing at
user level would change what every Claude on the machine does, including the
ones Beacon did not start.

**Consequence.** The agents exist only in sessions Beacon started, which is the
whole scope of the feature. A user who wants them everywhere can copy them; a
user who wants none can switch them off, because the descriptions cost context
in every session whether they are used or not.

## ADR-070: Beacon says what a number means and does nothing about it

**Context.** Beacon can now see the context window filling, the prompt cache
going cold, and how much of the allowance is left. All of it invites automation:
compact at 85%, start a clean session when the cache expires.

**Decision.** Advice only. At most one message at a time, dismissible, and no
action is ever taken on its own — no automatic compact, no automatic clear, no
session started or switched by Beacon.

**Why.** The choice between compacting and starting clean turns on whether the
next thing is the same piece of work, and Beacon cannot know that. Compacting
also costs tokens, so guessing wrong is not free. An application that acts on
its own advice has made the decision the advice existed to inform.

**Consequence.** Beacon will sit there showing 94% while the user carries on,
which is correct: they may be three minutes from finishing.

## ADR-071: A hook prints nothing

**Context.** Claude Code parses a hook's stdout — anything that starts with `{`
and ends with `}` is read as JSON. A third-party plugin on this machine emitted
two JSON objects from one `SessionStart` hook, and every session began with a
parse error at the top of the transcript.

**Decision.** Beacon's hook writes nothing to stdout, nothing to stderr, and
always exits zero, whatever it is given. Reporting is a line on a unix socket.
An integration suite runs the real binary against every registered event and
every malformed payload we could think of and asserts all three.

**Why.** There is no way to get JSON wrong if you never emit any. A hook is
registered once and then runs on every turn of every session on the machine,
including long after anyone remembers it is there, so its failure mode has to be
nothing at all.

**Consequence.** Beacon cannot use the parts of the hook contract that need
stdout — injecting context, denying a tool. It has never wanted to: it observes
sessions, it does not steer them.

## ADR-072: On Windows the daemon is reached over loopback TCP, with a token

**Context.** Everything that talks to the daemon — the window, and the `hook`,
`mcp` and `statusline` modes Claude Code starts — reaches it through a unix
socket file in a per-user directory. The standard library offers unix sockets
only on unix. Windows has had AF_UNIX since Windows 10 (1803), and that was the
first thing tried, through `uds_windows`. On the first company laptop it ran on,
`connect` failed with `WSAEINVAL` — even to a socket the same process had just
bound, outside any sandbox — because AF_UNIX goes through Winsock's provider
chain, and the security software installed there puts a provider in that chain
that does not know it. That is a common setup on exactly the machines a tool
like this is used on.

**Decision.** On Windows the transport is a loopback TCP connection on a port
the system picks. The file the other platforms bind as a socket
(`daemon.endpoint` here) holds the port and a 244-bit token instead. A client
sends the token first; the daemon answers `BEACON OK` and only then is the
connection handed to the protocol. Anything that cannot say the token is closed
without a word. Everything above `transport.rs` is unchanged: it still connects
to, and binds, a path.

**Why.** Named pipes were the other option and avoid Winsock entirely, but one
connection read on one thread and written on another needs overlapped I/O, and
a pipe needs an access list of its own to stop other users opening it. Loopback
TCP has neither problem and works wherever a browser does. What it lacks is the
filesystem permission that guards a socket file, and the token is that
permission: it lives in a file in the user's profile, which only they can read.

**Consequence.** A stale endpoint file names a port that is closed or taken by
something else; the handshake fails either way, so "can I connect" still means
"is a daemon there", which is what replacing a dead daemon's file relies on. A
local process that connects and never speaks holds the accept loop for at most
two seconds.

Windows does not refuse a connection to a closed loopback port at once: it
retries for two seconds. The temporary directory survives a reboot, so the first
start after one found the last daemon's file and waited that out twice — in the
window, and in the daemon checking for a running copy. The file therefore also
names the daemon's process; a client that finds that process gone is refused
immediately, and only a live one, or one it cannot tell about, is connected to,
with half a second to accept.

## ADR-073: On Windows the hooks run without a shell

**Context.** A hook is registered as a command line Claude Code hands to a
shell. On Windows that shell is Git Bash when it is installed and PowerShell
when it is not, and the two agree on no way of quoting a path with a space in
it: bash needs quotes, and PowerShell reads a quoted first word as a string to
print rather than a program to run. An installed Beacon lives under a folder
with a space in its name.

**Decision.** On Windows each hook is registered in exec form — the daemon's
path as `command`, `["hook"]` as `args` — which Claude Code starts directly. The
status line has no exec form, so its command is written to need no quoting in
either shell: forward slashes, and the 8.3 short name of any folder with a space
in it. The status line it displaces is run, as before, by the shell Claude Code
would have used: Git Bash, or PowerShell without it.

**Why.** Exec form removes the question instead of answering it for two shells
at once, and costs a hook nothing: there was never anything for a shell to do.

Checked against a real Claude Code on Windows: an exec-form hook whose program
lives under `C:\Program Files` fires on `SessionStart` and `Stop` with its `args`
intact.

**Consequence.** Drives other than the system one often keep no short names,
and then there is no command line both shells run. The status line is written
for the one Claude Code will use, found the way Claude Code finds it: quoted for
Git Bash, through PowerShell's call operator (`& '…'`) without it. Installing
Git afterwards changes which, and reinstalling the status line follows it.

## ADR-074: A Windows pseudo-console is answered, watched and left alone

**Context.** `portable-pty` runs sessions in a Windows pseudo-console (ConPTY),
which differs from a unix pty in three ways that matter here. It opens by asking
the terminal where the cursor is, and shows nothing until it hears back. It does
not close its output when the process inside it exits. And a program cannot be
replaced on disk while it runs, so a daemon outliving the window — the point of
it — locks its own binary against the next build.

**Decision.** The daemon answers the opening question itself, with the top left
— true of a session it has just created — and keeps it out of the scrollback. A
session's process is waited on directly, on a thread of its own, and its exit
reported from there. Before a daemon is built, a running one's binary is renamed
out of the way (`scripts/free-daemon.mjs`), which Windows allows.

**Why.** The question cannot wait for a window: sessions are started by the
daemon, often with nobody watching, and would sit silent until somebody was.
Left in the scrollback it would be answered again by every window that replays
it, into the shell's input. Waiting on the process is the only signal Windows
gives; reading to the end of the output never ends. Renaming is what macOS does
implicitly when a build replaces a running binary.

**Consequence.** The daemon also stops the window's own stdout from being
inherited by it, which Windows does by default and which kept test runners and
`tauri dev` waiting for output that never ended.

## ADR-075: On Windows the window draws its own caption buttons

**Context.** The overlay title bar that puts macOS's traffic lights inside
Beacon's own top row has no Windows equivalent in Tauri. Left decorated, the
window gets the system title bar as a second row of chrome above Beacon's, and
a transparent window under it draws badly.

**Decision.** `tauri.windows.conf.json` makes the window undecorated, and the
title bar draws minimise, maximise and close at its right edge, shaped and
placed as every Windows application has them. Closing goes through the same
close request a system button makes. Frosting is Windows 11's Mica.

**Why.** One row of chrome is the design. Mica rather than acrylic because
acrylic trails the window by a frame or more while it is dragged, and a window
that smears as it moves is worse than one that is merely a little different
from macOS's frosting.

**Consequence.** Windows 10 has no Mica, so the switch leaves the window sharp
there, and says so in the log. Snap Layouts on hovering maximise, which only
the system button offers, are not available; Win+Z and dragging to an edge
still are.

## ADR-076: A second agent is a second kind, not an agent inside the kind

**Context.** Beacon ran one agent, and `SessionKind` was `Shell` or `Claude`.
Running Codex beside it needed the kind to say which agent it was. The tidier
model is one variant carrying an agent — `Agent(AgentKind)` — and it reads
better in the place it is declared.

**Decision.** One variant per agent: `Shell`, `Claude`, `Codex`. `SessionKind`
converts to and from `AgentKind` where the two meet.

**Why.** The kind is half of the key a project's sessions are filed under, and
it is already on the wire as `"claude"`. A variant added beside that changes
nothing that already works, while reshaping it would have changed the stored
shape and every comparison at once. The coexistence the whole feature is for
then falls out for free: two agents are two kinds, and collide no more than a
shell and a Claude do.

**Consequence.** A third agent is a variant here and a row in the panel's table,
rather than one place. The conversion exists so the daemon can cross between
"conversations by agent" and "sessions by kind" without writing the match out
each time.

## ADR-077: A conversation's id may belong to the agent rather than to Beacon

**Context.** Beacon names a conversation before it exists: it generates a UUID
and hands it to Claude Code with `--session-id`, which is what makes a
workstream something it can address, resume and fork without reading a
transcript. Codex refuses to be told. It generates its own id and reports it
afterwards, and asking for the flag is an open request upstream.

**Decision.** A conversation records two ids: Beacon's own, always known, and
the agent's, learned when the agent says it. They are the same number for
Claude Code by construction. `resume_id` is the single place that knows which
to use, and it answers nothing during the window where a Codex conversation
exists for Beacon and has not yet been identified.

**Why.** The alternative is making Beacon's id optional, which spreads the
absence through naming, listing, forking and the UI — for a window that is one
report long and in which there is nothing to resume anyway.

**Consequence.** The first report from a Codex session is load-bearing: it is
the only chance to connect the two ids, so the conversation Beacon has open
with that agent in that project is the one it must belong to. Every later
report matches on the learned id, and a second session reporting a different id
afterwards is refused rather than allowed to point Beacon's conversation at
something it does not own. Without hooks there is no id at all, which is why
Codex's capability check asks for hooks and resume together.

## ADR-078: Beacon installs itself into Codex as a plugin, and cannot trust it

**Context.** Claude Code takes hooks as entries in a settings file, which Beacon
writes and removes. Codex takes hooks from plugins, and plugins from
marketplaces. It also keeps a hash of every hook it has been shown and runs
none it has not.

**Decision.** Beacon generates a marketplace with one plugin in it, into its own
configuration directory, and registers it with `codex plugin`. Trusting the
hooks is left to the user, once, in Codex's own `/hooks`. Whether it worked is
answered by whether a report ever arrives, not by reading Codex's configuration.

**Why.** The hash is a safeguard against arbitrary code, and forging it would be
defeating it on the user's behalf. There is a flag that bypasses trust for one
run; it also prints warnings into the session, so it is not a quiet workaround
either. Reading the trust record instead of watching for reports would be a
second source of truth about the same question, and the stale one.

**Consequence.** Installing achieves nothing visible until somebody trusts it,
so Settings says so at the point it becomes true rather than as a footnote. The
plugin is generated rather than shipped because the hook names the daemon on
this machine, which moves when the application does — a moved daemon reads as
stale and reinstalling is the fix.

## ADR-079: Separate checkouts are all of an agent's or none of them

**Context.** Two agents in one project write to the same files and the second
one to save wins. Git worktrees are the existing answer, but who gets moved
needs deciding: the obvious arrangement is one agent keeping the project's own
directory and the rest being placed elsewhere.

**Decision.** A per-project switch, off by default. When it is on, every agent
works in a worktree of its own — including the only one — and the project's
directory belongs to the user. They live under Beacon's configuration directory,
on branches named `beacon/<agent>`.

**Why.** "One keeps the real directory" needs an answer to which one, and every
answer is arbitrary or depends on the order somebody opened panels in. All or
none is a rule that fits in a sentence, and the sentence is the feature: agents
work in their own checkouts, yours stays yours. Under Beacon's directory rather
than beside the project, so nothing Beacon makes is scanned by a build,
committed by accident, or mistaken for something the user left there.

**Consequence.** Turning it on changes where an agent's work lands, so the
panel's header names the branch: a mode nobody can see is a mode that bites.
Turning it off leaves the checkouts where they are, because they may hold work
nobody has merged. A project that is not a git repository has no worktrees and
keeps working. The file, git and editor panels still show the user's checkout;
following the focused agent instead is a later decision and not this one.

## ADR-080: A Windows update moves the running daemon aside

**Context.** The daemon outlives its window, and every Claude session starts the
same file again as its MCP server, so when an update arrives
`beacon-daemon.exe` is usually running — often several times over. The NSIS
installer closes the window and nothing else, and Windows will not overwrite a
program that is running. The first update to 0.6.0 left a 0.5 daemon on disk
beside a 0.6 window. The window found a daemon speaking protocol 6, asked it to
stop, started the file beside it — the same old daemon — and carried on talking
protocol 7 to it. What the user saw was `could not reach the session daemon:
… (os error 10053)`, and no sessions.

**Decision.** An installer hook (`src-tauri/windows/hooks.nsh`) renames the old
binary before the new one is copied, which Windows allows for a running
program: the old daemon carries on from its new name, the new file takes the
old name, and the new window replaces the daemon over the protocol exactly as
it does on macOS. Whatever earlier updates moved aside is deleted once nothing
runs from it. The uninstaller does the same, so it can remove the rest.

And the client no longer trusts that a replacement is current. If the daemon
started in place of an old one answers in an old protocol too, it is stopped,
and the window says that the daemon on disk is from another version and that
installing Beacon again fixes it.

**Why.** Stopping the daemon from the installer would have worked too, and
would have ended every session on every update, including one that kept the
protocol — which macOS never does. Renaming changes nothing about what happens
to sessions; it only stops a file from being in the way.

**Consequence.** The MSI is built by WiX, which these hooks do not reach; it
falls back on Windows Installer's own handling of files in use, which asks to
close programs or to restart. The `-setup.exe` is what the site offers, an
installation made with it updates itself with the `-setup.exe` again, and the
updater's fallback for Windows now points at it rather than at the MSI.

## ADR-081: The last reply comes from the Stop hook, not from the terminal

**Context.** After a long turn the answer you came back for sits under a screen
of tool output, and finding it means scrolling for it. The obvious fix is to
mark where the reply starts in xterm's buffer and decorate it. Claude Code
draws full-screen on the terminal's alternate screen, so its replies never
reach xterm's scrollback: there is no line in the buffer to mark, and anything
anchored to one points at nothing a moment later.

**Decision.** Claude Code's Stop hook carries the turn's final reply as
`last_assistant_message`. The hook passes it along in its `Report`, cut at four
thousand characters, the daemon relays it with the `done` activity, and the
Claude panel shows it in a strip floating over the top of the terminal —
folded to three lines, with more, copy and dismiss. It floats rather than
taking rows: every change to the terminal's height resizes the PTY and makes
Claude Code redraw the whole screen. It goes when the next turn starts working, or the
session is cleared or ends; a permission prompt leaves it.

**Why.** It is Claude Code telling Beacon what it said, which is a fact; the
alternative was reading it back out of what was drawn, which is a guess and
breaks with every change to how Claude Code draws.

**Consequence.** It needs the hooks installed, as every other report does, and
there is nothing for Codex until something reports for it. The reply is held
in the daemon's broadcast and the window's memory only — never written to disk
— and a window opened after the turn ended does not see it.

## ADR-082: Sounds are their own setting, and synthesised

**Context.** A notification is for someone at the screen. Somebody across the
room, or in another application full-screen, misses it — and the notification
sound is the system's, the same as every other application's.

**Decision.** Two chimes, for the same moments a notification is for: Claude
waiting for you, and a turn long enough to announce finishing. Each has a
switch of its own in Settings → Notifications, beside the notification one, with a
button to hear it; both are off by default — a sound reaches the whole room,
and nobody updating should start hearing one unasked — and saved with the
other settings. They follow
the notification rule — never for the project you are looking at — but not the
notification switch or the system's permission for it. They are a few sine
notes through one shared audio context, rising for waiting and falling for
done, so they are told apart by ear and nothing has to be bundled. The context
is resumed on the window's first key or click, because WebView2 and WebKit both
start one silenced until the page has seen a gesture.

**Consequence.** Until the first key or click after the window opens, a chime
asked for may not play. That window is short, and the alternative — a sound
that needs no gesture — is not something either webview allows.

## ADR-083: One sign-in reaches every open project

**Context.** Open several projects before signing in to Claude Code and every
Claude panel sits on its own sign-in screen. Signing in from one stores the
credential where every `claude` on the machine reads it, but each reads it only
when it starts — so the rest go on waiting, and signing in means a trip to the
browser and back once per project. Beacon cannot share the sign-in itself:
ADR-052 rules out reading, storing or forwarding a credential, and rightly.

**Decision.** Beacon asks Claude Code, not the credential store. Whenever the
daemon hands out a Claude session it has not seen, a watcher asks
`claude auth status --json` and reads one field, `loggedIn`. Sessions that
started while the answer was no are waiting; the watcher asks again every few
seconds, and once the answer is yes the daemon starts each waiting session
again — the way the Resume button does, so a conversation carries on — and
tells the window, which rebuilds the panel. The panel somebody signed in from
is left alone: it is the one where Return was pressed last, because every step
of signing in ends with Return, and nothing the terminal writes on its own —
replies to the program's queries, focus moving in and out — contains one.

**Why.** It keeps ADR-052 whole. Whether someone is signed in is asked of the
program that holds the credential, in the same words the user could type, and
each restarted `claude` finds the credential by itself; Beacon never sees it.
Reading the sign-in screen out of the terminal was the alternative and was
rejected for the reason ADR-081 gives: what Claude Code draws is a guess that
breaks with every change to how it draws it, and its own answer is a fact.

**Consequence.** On a machine that is signed in — nearly always — it costs one
short `claude auth status` when sessions start, and nothing after. A Claude
Code with no `auth` command, or one that will not answer, switches it off: the
panels behave exactly as they did. Signing in outside every waiting panel — in
a terminal, say — restarts all of them, unless Return was pressed in one, which
is then taken for the panel the sign-in came from and left on its sign-in
screen for the user to restart. The reverse can happen too: Return pressed in
another waiting panel in the few seconds between signing in and the next
question makes that one the panel taken for the sign-in, and the one signed in
from is started again — which costs a moment, since its conversation is
resumed. The watcher stops asking after half an hour,
and leaves anything still waiting as it is. Signing out, or a sign-in that
expires while sessions are running, is not covered. Codex signs in on its own
terms and is not covered either.

## ADR-084: The welcome guide stands apart from the screens it explains

**Context.** Somebody installing Beacon met an empty window and a workspace to
name, and nothing said what came next — not even that the Claude panel would
ask them to sign in. A guide fixes that, and a guide is also the first thing to
go stale: it describes screens other people keep changing, and Settings in
particular is due to be rearranged.

**Decision.** A first run opens a short guide once the first workspace exists:
the basics (theme, notifications, the two sounds), a first project if there is
none, the two bars, an agent's panel in a step of its own, one step shared by
files, editor, terminal and git, the keyboard, and last the Claude panel, where
signing in happens. A panel that is hidden gets no step at all; the keyboard
step names the hidden ones and says the palette brings them back. Its switches
are its own, calling the store actions Settings calls, never Settings' rows or
their place on screen. It points at things only through hooks that exist for
other reasons — a panel's `data-panel`, and a `data-region` on each bar — and a
step whose target is not on screen is shown in the middle instead of pointing
at nothing. Every panel is introduced somewhere, alone or with the others, and
a test fails when a panel is added without being put in one or the other;
another fails if a hook it points at is removed. It counts as seen once it has
opened, is skipped with Escape, and stays in the command palette for later. A
settings file from before it existed means somebody who found their way without
it, so only a fresh install sees it on its own.

**Why.** Settings can then move a switch, rename a section or split the screen
without touching the guide, and a hidden or absent panel cannot leave a
spotlight on an empty rectangle. Counting it as seen when it opens, rather than
when it is finished, means it never comes back uninvited.

A step for every panel, hidden ones included, made twelve, and made the guide
longer the less of Beacon somebody had chosen to see. Codex is hidden until it
is asked for, so that version spent a new user's first minute offering them a
second agent before they had met the first. Files, editor, terminal and git say
what they are from their names, and four cards in a row saying "this is X" is
the stretch of a guide people skip — so they are introduced together, a
sentence each, and the step points at whichever of them is on screen.

**Consequence.** Seven steps on the run this is about — a fresh install, where
there is no project yet and so no panel to point at — eight once there is one,
with Codex and the editor away as Beacon starts them, and nine with every panel
open. What the guide says about a screen is still words that can fall behind
it; only what it points at and what its switches do are held in place by code. The macOS notification permission waits until the guide closes,
rather than opening a second prompt on top of it.

## ADR-085: The files panel searches in place of the tree

**Context.** The files panel's toolbar had room to spare, and finding a file
by walking folders is slow in a large project. Quick Open already finds one
from anywhere, but it is a palette that closes: it says nothing about where the
file sits, which is what the panel is for.

**Decision.** A search field in the panel's toolbar. While it has text, the
list of matches takes the tree's place; clearing it brings the tree back.

- The project's files are read when a search starts, from the same listing as
  Quick Open (`git ls-files` where there is a repository, so the ignore rules
  apply once), and not again on each key.
- A match on the file's name ranks ahead of any match that needs its folders,
  whatever their fuzzy scores; only the name is highlighted.
- Dotfiles follow the tree: hidden there, left out of the search.
- Opening a match reveals it in the tree and brings its row into view.

The editor's find panel and its dialogs (go to line) use the app's fields and
buttons, under selectors as specific as CodeMirror's own light and dark rules.

**Why.** Replacing the tree rather than floating over it, because the panel is
narrow and a list of matches is the tree's job for as long as the search lasts.
Read fresh, because a search that cannot find the file an agent has just
written is worse than the moment it takes to list them.

**Consequence.** Each search lists the whole project again. Nothing caps the
listing on the git path, and the command is synchronous, so a very large
repository costs a pause — the same exposure Quick Open has, made easier to
reach.

