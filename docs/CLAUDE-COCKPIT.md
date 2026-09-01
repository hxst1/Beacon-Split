# Claude Cockpit — audit and plan

The goal is not to add Claude Code features to Beacon. It is to make working
through Beacon cheaper and better organised than working in a bare terminal,
using Claude Code's own interfaces and inventing nothing.

Audited against **Claude Code 2.1.252** (`~/.local/share/claude/versions/2.1.252`,
native arm64 binary). Every capability below was verified against that build or
the current official documentation, not assumed.

---

## Phase 1 — Audit

### 1.1 How Beacon is put together today

Three Rust crates and a React frontend.

| Piece | Job |
| --- | --- |
| `beacon-core` | Domain, sessions, PTYs, settings, git, files, protocol. No Tauri anywhere in it |
| `beacon-daemon` | Owns every PTY. One binary, four modes selected by `argv[1]`: `hook`, `mcp`, `statusline`, or the daemon itself |
| `src-tauri` | A thin command layer. Mostly forwards to `beacon-core` or to the daemon |
| `src/` | React + zustand + CSS modules. Panels in a binary split tree |

The daemon listens on a unix socket in the per-user temporary directory, speaks
newline-delimited JSON with a version handshake (`Hello { version }`), broadcasts
`Event`s to every attached client, and stops itself after five minutes with no
sessions and nobody attached. The window is a client; closing it ends nothing.

That the daemon binary is also the hook, the MCP server and the status line is
the single most useful fact in this document: **every new Claude Code integration
point already has a place to live, with no new process to package.**

### 1.2 How Claude is started

`beacon-core/src/session.rs`, `SessionManager::spawn`:

```
resolve_program("claude")
  --mcp-config=<runtime dir>/mcp.json        # the clip tool, per session, never installed
  cwd = project root
  env: sanitised (STRIPPED_ENV), TERM=xterm-256color, TERM_PROGRAM=Beacon,
       UTF-8 locale if none inherited,
       BEACON_SOCKET + BEACON_PROJECT (Claude sessions only)
```

Sessions are keyed by `(ProjectId, SessionKind, slot)`. Claude is always slot 0.

**This is the whole gap.** A Claude session in Beacon has no identity beyond
"the Claude of this project". It has no id Beacon knows, no name, no history, and
no way back. `Restart` kills the process and starts a new one — the conversation
is gone. Everything in sections 1, 2, 5, 6, 8–14, 21, 23 and 28 of the brief
follows from that one missing concept.

### 1.3 Hooks

Installed deliberately from Settings into `~/.claude/settings.json`, user level,
marked by the string `beacon-daemon` so they can be found and removed again.
Eight events: `PermissionRequest`, `Notification`, `PreToolUse`,
`UserPromptSubmit`, `Stop`, `StopFailure`, `SessionStart`, `SessionEnd`.

`hook::run()` reads stdin, maps the event to a `ClaudeActivity`, writes one line
to the socket, **prints nothing to stdout and always exits 0.** It is inert
without `BEACON_SOCKET`, so a Claude started outside Beacon is unaffected.

Beacon's hook is already correct against the contract in §20 of the brief. The
failure that prompted the brief was a third-party plugin (claude-mem) emitting
two concatenated JSON objects on stdout from one `SessionStart` hook. Worth
regression tests here anyway, because the next hook we add is the one that could
get it wrong.

### 1.4 Status line

Beacon takes the slot non-destructively (ADR-037): the previous command is
recorded under `beacon-splitPreviousStatusLine`, handed to Beacon's status line
as `argv[2]`, executed, and its stdout is what Claude Code shows. Uninstalling
puts it back exactly.

`statusline::interpret()` currently reads seven fields: model display name,
context used percentage / tokens / size, and the two rate-limit windows. The
daemon keeps the last report per project and broadcasts it; `src/features/usage`
holds it in a store with a 15-minute staleness rule; `UsageMeter` shows it in the
title bar with a popover.

The transport asked for in §3 **already exists end to end.** What is missing is
fields, not plumbing.

### 1.5 What of the brief is already built

| Brief | State |
| --- | --- |
| §3 statusline → daemon → UI transport | Built |
| §4 do not destroy the user's status line | Built, and correct |
| §7 five-hour and seven-day rate limits | Built |
| §20 hook infrastructure, stdout discipline | Built |
| §22 an efficiency popover | Built, in the title bar |
| §27 Beacon is not a wrapper | Already the stated architecture (ADR-034, ADR-054) |
| §31 nothing leaves the machine | True today |
| §32 the status line path is cheap | True: parse, one socket write, exit |
| Everything else | Not built |

### 1.6 What Claude Code 2.1.252 actually offers

Verified by `claude --help`, `claude agents --help`, and string inspection of the
installed binary.

**Flags that matter**

- `--session-id <uuid>` — Beacon can *assign* the session id. This removes any
  need to discover it afterwards, and any temptation to read transcripts.
- `-n, --name <name>` — a display name, shown in the prompt box, the `/resume`
  picker and the terminal title.
- `-r, --resume [value]`, `--fork-session`, `-w, --worktree [name]`
- `--agents <json>` — custom agents defined **for the session only**, no files
  written into the user's repository. Exactly what §11 asks for.
- `--settings <file-or-json>` — additional settings per session.
- `--model`, `--effort <low|medium|high|xhigh|max>`, `--append-system-prompt`

**`claude agents --json`** prints every active session — interactive and
background — as JSON, without needing a TTY:

```json
[{ "pid": 15990, "cwd": "/Users/eya/projects/personal/beacon-split",
   "kind": "interactive", "startedAt": 1788242858772,
   "sessionId": "b57bf9d0-…", "name": "beacon-split-b7", "status": "busy" }]
```

This is the official answer to "which sessions exist and is one already open".
It is what §1's no-duplicate-session rule should be enforced against.

**Hook events present in this build**

`SubagentStart`, `SubagentStop`, `TaskCreated`, `TaskCompleted`, `PreCompact`,
`PostCompact` — all of them, plus `PermissionDenied`, `PostToolUseFailure`,
`FileChanged`, `WorktreeCreate/Remove`, `PreModelSwitch/PostModelSwitch` and
more. Subagent events carry `agent_id` and `agent_type`; task events carry
`task_id`. Both task events can *block* on exit 2 — Beacon's hook always exits
0, so that risk is already closed.

**Environment variables present in this build**

`CLAUDE_CODE_TASK_LIST_ID`, `CLAUDE_CODE_ENABLE_TASKS`,
`CLAUDE_CODE_SUBAGENT_MODEL`, `CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION`,
`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`, `CLAUDE_CODE_SESSION_NAME`.

**The status line payload, in full**

`session_id`, `session_name`, `prompt_id`, `cwd`, `version`, `model.{id,
display_name}`, `workspace.{current_dir, project_dir, added_dirs, git_worktree,
repo}`, `worktree.{name, path, branch, original_cwd, original_branch}`,
`output_style.name`, `agent.name`, `pr.*`, `cost.{total_cost_usd,
total_duration_ms, total_api_duration_ms, total_lines_added,
total_lines_removed}`, `context_window.{total_input_tokens, total_output_tokens,
context_window_size, used_percentage, remaining_percentage, current_usage}`,
`exceeds_200k_tokens`, `fast_mode`, `effort.level`, `thinking.enabled`,
`vim.mode`, `prompt_cache.{warm, caching_observed, ttl, expires_at, requests,
misses, expected_rebuilds, hit_ratio, cache_write_tokens, miss_recache_tokens,
last_miss_at, recache_tokens_if_cold}`, `rate_limits.{five_hour, seven_day,
spend_limit}`.

Absence rules that matter: `rate_limits` only for Pro/Max and only after the
first API response, each window independently, and a window is dropped once its
`resets_at` passes. `prompt_cache` only after the first API response.
`session_name` only when a name was set with `--name` or `/rename`, or once an
AI-generated title exists — an auto-generated display name like `beacon-split-b7`
does **not** populate it. `effort` only when the model supports it.
`context_window.current_usage` is `null` before the first API call and again
after `/compact`.

The status line runs on events — a new assistant message, `/compact` finishing, a
permission-mode change, a rate-limit or cache expiry — debounced at 300ms, with
an optional `refreshInterval`. It is cheap and it is not polling.

### 1.7 A bug found during the audit

`crates/beacon-daemon/src/statusline.rs` reads `context_window.current_usage`
with `as_u64()`. That field is an **object** (`input_tokens`, `output_tokens`,
`cache_creation_input_tokens`, `cache_read_input_tokens`), never a number. The
call returns `None` and falls through to `total_input_tokens`, which happens to
be the value we want — so nothing is visibly broken. But the unit test fixture
writes `"current_usage": 74800`, a shape Claude Code has never sent, so the test
asserts against fiction. Fix the fixture to the documented shape and read
`total_input_tokens` deliberately rather than by accident.

### 1.8 Risks and open questions

- ~~**`--settings` merge semantics are unverified.**~~ **Settled by experiment:
  it merges.** A `--settings` file supplying `hooks.SessionStart` fired
  alongside the user's own plugin hooks, both visible in the same debug log. So
  Beacon can supply hooks and agents per session without writing anything into
  `~/.claude/settings.json`, which is what the subagent block will use.
- **`CLAUDE_CODE_TASK_LIST_ID` is in the binary but not in the public docs.**
  Treat it as capability-detected and opt-in; if it stops working, the feature
  disappears and nothing else breaks.
- **Making `Restart` resume instead of starting fresh changes existing
  behaviour.** It should become two distinct actions, not a silent change.
- **More hook events means more processes per turn.** Subagent events are the
  chattiest. Register them behind the capability check and keep the hook doing
  exactly what it does now: parse, one write, exit.
- **claude-mem injects roughly 19,700 tokens at every session start** on this
  machine. That works directly against §5 and §6. Beacon should make it visible —
  never disable it, never reconfigure it.
- **Duplicate sessions.** Once sessions have ids, resuming one that
  `claude agents --json` reports as running must be refused, with fork or
  worktree offered instead.

---

## Phase 2 — Plan

Ordered so each item lands on its own, with typecheck, `cargo fmt`, clippy and
tests green before the next one starts. Nothing here requires the item after it.

### Foundations

- [x] **1. Fix the `current_usage` shape** and its test fixture. Small, isolated,
      and it makes the fixture the documented payload every later task extends.
- [x] **2. `ClaudeCapabilities`** — parse `claude --version` once, resolve a
      feature set (`namedSessions`, `sessionId`, `agentsFlag`, `taskListId`,
      `taskHooks`, `subagentHooks`, `promptCacheStats`, `rateLimits`), expose it
      over IPC. Everything after this is gated on it, and a missing feature hides
      its UI rather than breaking.
- [x] **3. Widen `UsageReport`** to the rest of the documented payload:
      `session_id`, `session_name`, effort, thinking, prompt cache, remaining
      percentage, spend limit, worktree, agent. Every field `Option`. Extend
      `interpret()` and its tests against the real fixture.

### Session identity — the core of the change

- [x] **4. `ClaudeSession` in `beacon-core`**: project, uuid, name, model, last
      activity, last known context percentage, status. Persisted in
      `claude-sessions.json` with the daemon as its only writer, exactly as the
      clip book is.
- [x] **5. Beacon assigns the session.** Spawn with
      `--session-id <uuid> -n <name>`. No discovery, no transcript reading.
- [x] **6. Reconcile against `claude agents --json`** — on demand and cached,
      never in a hot path. This is what makes "already open elsewhere" knowable.
- [x] **7. Protocol and daemon**: list a project's workstreams, start a named
      one, resume, fork. Refuse to resume a session id reported running; offer
      fork or worktree instead.

**Decided:** `Restart` splits into two actions rather than changing meaning.
`Resume` returns to the same conversation and becomes the header's default;
`New workstream` starts clean and asks for a name. A button that already exists
does not quietly start doing something else.

### Session UI

- [x] **8. The Claude panel header** gains a session chip — name, context, model
      — and a popover: recent workstreams, new, rename, resume, fork, compact.
      One row, no new chrome.
- [x] **9. Context health and cache** in the existing usage popover: bands rather
      than a bare number, cache warm/cold with `recache_tokens_if_cold`.
- [x] **10. Recommendations**, one at a time, dismissible, never automatic:
      clean workstream when the context is large and the task is done; compact
      only when continuity is needed; large cold context.

### Shared task list

- [~] **11–12. The shared task list — not buildable yet.** See *The task list*
      below. `CLAUDE_CODE_TASK_LIST_ID` exists in the binary and the task tools
      exist by name, but no session could be made to offer them, so there is
      nothing to build against. Left here rather than dropped: the moment the
      feature is reachable, the plan is unchanged.

### Subagents

- [x] **13. Three agents via `--agents`**, session-scoped, nothing written into
      the repository: `beacon-explorer` (Haiku, read-only, returns conclusions
      and locations), `beacon-tester` (Sonnet, runs tests, returns the command,
      the failures and the root error), `beacon-reviewer` (Sonnet, fresh context,
      read-only, reviews the diff). No debugger until the first three earn it.
      `maxTurns` on each.
- [x] **14. A routing policy** of five lines via `--append-system-prompt`, with a
      setting to turn it off. If it grows past a short paragraph it has stopped
      paying for itself.
- [x] **15. `SubagentStart` / `SubagentStop`** → an ephemeral agent row in the
      Claude header. Not persisted; it is activity, not history.

### Closing

- [x] **16. Hook stdout contract tests** — every Beacon hook mode, asserting
      empty or single-valid-JSON stdout and exit 0, for every event.
- [x] **17. ADRs and ROADMAP** — this becomes Milestone 11.

### Deliberately deferred

The configuration audit screen (§17), test-output filtering (§19), Agent Teams
(§24), parallel sessions beyond worktrees (§25), and automating the
writer/reviewer loop (§26). Each is worth building; none should come before a
session has an identity.


---

## Progress

### Foundations — done

`interpret()` now reads the documented payload rather than a shape Claude Code
never sends, and its fixture is the published one. `beacon-core::claude` answers
what the installed CLI can do by reading its own `--help`, exposed to the
frontend as `claudeCapabilities()`. `UsageReport` carries the session id and
name, the model id, effort, thinking, the room left, the prompt cache and the
spend limit — every one optional, absent meaning unknown.

The report outgrew the messages carrying it, so `Request::ReportUsage` and
`Event::Usage` box it. `Box` is transparent to serde: the wire format is
unchanged, and the protocol version did not have to move.

Gates at the end of the block: `cargo fmt --check`, `clippy --all-targets`,
`cargo test --workspace` (258 tests), `tsc --noEmit`, `vitest` (147 tests) — all
clean.


### Session identity — done

Beacon chooses the conversation. `WorkstreamId` is a hyphenated UUID because
that is what `--session-id` takes, and it is settled before the process exists —
no transcript is ever read to find out what Beacon is talking to.

Four things were checked against the real CLI rather than assumed, and two of
them changed the design:

- `--session-id <uuid>` on a *new* conversation works, and `--name` may be
  passed alongside it, so Beacon's name and the one Claude Code shows in its own
  prompt box stay in step.
- `--resume <parent> --fork-session --session-id <new>` works, and the fork
  carries the parent's history. So a fork lands on an id Beacon chose too, and
  there is no pending state waiting to learn what Claude picked.
- **`--session-id` on a conversation that already exists is refused** —
  *"Session ID … is already in use"*. Every start after the first has to resume.
  That is why `Workstream::started` is persisted rather than remembered in the
  process: a daemon that came back after a restart and guessed would meet that
  error where the session should have been.
- `--settings` merges with the user's settings rather than replacing them.

`WorkstreamBook` holds each project's conversations and which one it is in,
capped at thirty per project, with the current one never dropped to make room.
The daemon is its only writer, as it is for the clip book. What the status line
reports is folded in by session id — so a Claude somebody started in their own
terminal is ignored rather than written onto whichever conversation happened to
be current — and reaches disk at most every thirty seconds, plus once on the way
out.

Five new requests, so `PROTOCOL_VERSION` moved to 5. The cost is known and was
paid once before: upgrading replaces a running daemon and the sessions it holds.

Resuming a conversation that `claude agents --json` reports as running is
refused, with forking offered instead. When Claude Code cannot be asked, the
resume goes ahead rather than being blocked by a question nobody can answer.

Gates: `cargo fmt --check`, `clippy --all-targets`, 291 Rust tests, `tsc`,
147 vitest — all clean. Nothing is visible in the UI yet; that is the next block.


### Session UI — done, and a design correction found by running it

The Claude panel header gains a chip: the workstream's name, and the context
percentage, tinted only once it starts to mean something. Everything else —
new, rename, fork, and the list to go back to — is in its popover, because the
header is read constantly and used rarely. On a Claude Code without the flags,
the chip is not there at all.

`Restart` became `Resume`, and now says what it does: start the process again
and carry on in the same conversation. The old word stays where the old
behaviour does, on a Claude Code that cannot do better.

Context health is four bands named for what you would do about them, sharing
its upper two boundaries with the allowance gauge so the same number does not
mean two things. The usage popover gained the cache: warm or cold, the hit
ratio, when a warm one goes cold, and what a cold one would cost to rebuild.
Advice is at most one thing at a time, dismissible, and never acted on.

**What running it against a real Claude Code changed.** The daemon was driven
over its socket with a real `claude` in a PTY, and the resume path answered:

> No conversation found with session ID: cafb8c86-…

`Workstream::started` meant *"Beacon has started a process with this id"*, and
that is not the same as *"the conversation exists"*. Claude Code writes nothing
until the first exchange, so a session opened and never typed into leaves
nothing to resume — while a session that has had one turn refuses
`--session-id`. The flag now means what it has to and is called `resumable`,
set by proof from inside the session: a hook event that can only have happened
during a turn, or a status line report showing tokens in the window. Opening a
session is explicitly not proof, which is the case that was wrong.

`Request::Report` carries the conversation's id for that reason, and the hook
reads `session_id` out of its payload.

Verified end to end afterwards: start a workstream, have a real exchange in it,
switch to a second one, come back. `claude agents --json` showed the resumed
conversation running under the id and name Beacon chose, and its scrollback
carried the earlier exchange — with Claude Code's own footer showing the same
name Beacon shows.

Gates: `cargo fmt --check`, `clippy --all-targets`, 296 Rust tests, `tsc`,
158 vitest — all clean.


### Subagents — done, with one part that could not be built

**The task list (11–12), and why it is not here.**

`CLAUDE_CODE_TASK_LIST_ID` is a real string in the 2.1.252 binary, and so are
`TaskCreate`, `TaskList` and `TaskUpdate`. But no session could be made to
offer those tools: not `claude -p`, not an interactive session in a PTY, with
`CLAUDE_CODE_ENABLE_TASKS=1`, and with Agent Teams switched on as an
experiment. Asked directly, Claude answered that the only `Task*` tools it had
were `TaskOutput` and `TaskStop`. No `TaskCreated` hook ever fired. The
settings reference documents none of it.

So there is nothing to verify a shared task list against, and building a
`Tasks 3/7` badge on an environment variable that cannot be shown to do
anything would be exactly the invented optimisation this work is supposed to
avoid. It stays on the list, unbuilt, with a note.

**The three agents (13).** `beacon-explorer` on Haiku, read-only, twelve turns;
`beacon-tester` on Sonnet with a shell, twelve turns; `beacon-reviewer` on
Sonnet, read-only, eight — fresh context and no memory, because a review worth
having is one that cannot be talked into agreeing. No debugger: an agent that is
rarely the right answer still costs tokens in every session that never uses it.

Passed with `--agents`, so they live for one session and nothing is written into
the user's repository. Descriptions are one line each and there is a test that
keeps them that way, because every description sits in the main conversation's
context whether it is used or not. The detail is in the prompts, which are read
only when an agent runs.

Verified against a real session started by the daemon: the command line carried
`--session-id`, `--name`, `--mcp-config`, `--agents` and
`--append-system-prompt`, and asked what agents it had, Claude answered
`beacon-explorer, beacon-reviewer, beacon-tester` **alongside** the user's own.
`--agents` merges; it does not replace.

**The routing policy (14)** is four sentences, and there is a test that fails if
it grows: it is in the context of every turn, so whatever it saves in delegated
output it has to save several times over. Both it and the agents are behind one
switch in Settings, because they are not free.

**Agent activity (15).** `SubagentStart` and `SubagentStop` joined the
registered events. From outside, a session that has handed a large search to a
subagent looks exactly like one that has gone quiet — the panel shows nothing
and the turn does not end — and the header row is the difference. It says what
is running and for how long, then goes away six seconds after it finishes, and
nothing is persisted.

The payload was read off a real `SubagentStop` rather than guessed:
`agent_id`, `agent_type`, `last_assistant_message`. `agent_type` came back as
an empty string, so the hook turns empty into absent — an empty name on screen
reads as a nameless agent rather than an unnamed one.

**A design smell caught by clippy.** Starting a session had grown to eight
positional arguments. They are now one `SessionPrefs`, which is also the honest
name for what they are: what a session should be started as, sent by the client
because the daemon reads no settings.

Gates: `cargo fmt --check`, `clippy --all-targets`, 309 Rust tests, `tsc`,
170 vitest — all clean.


### Closing — done

A hook contract suite runs the real binary against every registered event and
every malformed payload: nothing on stdout, nothing on stderr, exit zero. One of
its tests is an invariant rather than a case — every event Beacon registers must
be one Beacon reads, because a registered event nothing handles is a process
Claude Code starts for nothing on every turn, forever.

ADR-066 through ADR-071 record the six decisions worth keeping: choosing the
conversation rather than discovering it; what makes a conversation exist;
capabilities read from `--help` rather than a version table; subagents passed
per session; advice that is never acted on; and a hook that prints nothing.

ROADMAP gains Milestone 11, including what could not be built and why.

Gates: `cargo fmt --check`, `clippy --all-targets`, 318 Rust tests, `tsc`,
170 vitest — all clean.
