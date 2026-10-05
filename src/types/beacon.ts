/**
 * The wire format between Rust and the UI.
 *
 * These types mirror the `serde` output of `beacon-core`; they are hand-written
 * rather than generated because the surface is small and a generator is one
 * more thing to keep alive. Any change on the Rust side must be reflected here.
 */

export type ProjectKind =
  | 'git'
  | 'node'
  | 'pnpm'
  | 'yarn'
  | 'bun'
  | 'deno'
  | 'rust'
  | 'python'
  | 'go'
  | 'tauri'
  | 'docker'

export interface Project {
  id: string
  name: string
  /** Resolved for this machine — never persisted in this form. */
  absolutePath: string
  /** Short, human-facing form, e.g. `Personal/beacon-split`. */
  displayPath: string
  kinds: ProjectKind[]
  /**
   * Whether each agent works in a git worktree of its own.
   *
   * Off unless asked for. When on it applies to every agent including the only
   * one: agents work in their own checkouts, and this one stays yours.
   */
  agentWorktrees: boolean
}

export interface Workspace {
  id: string
  name: string
  /** `#rrggbb`, validated by the backend. */
  accent: string
  icon: string | null
  projects: Project[]
}

/** The panels Beacon can place. */
export type PanelId = 'claude' | 'codex' | 'editor' | 'files' | 'git' | 'terminal'

export type SplitDirection = 'row' | 'column'

/**
 * How the window is divided. A binary split tree, so every preset and any
 * custom arrangement are the same structure.
 */
export type LayoutNode =
  | { type: 'panel'; panel: PanelId }
  | {
      type: 'split'
      direction: SplitDirection
      /** Share of the space given to `first`, 0..1. */
      fraction: number
      first: LayoutNode
      second: LayoutNode
    }

/** An action that can be given a keyboard shortcut. */
export interface ActionBinding {
  action: string
  /** What it is bound to now, e.g. `mod+shift+p`. */
  binding: string
  /** What it would be with nothing configured. */
  defaultBinding: string
}

export type LayoutPreset =
  | 'claude-left'
  | 'claude-right'
  | 'claude-right-tall'
  | 'claude-left-tall'
  | 'custom' 

/** One payload that fully describes the app. Every mutation returns a fresh one. */
export interface Snapshot {
  workspaces: Workspace[]
  activeWorkspace: string | null
  activeProject: Record<string, string>
  layout: LayoutNode
  preset: LayoutPreset
  /** Panels toggled off. They keep their place in the tree. */
  hidden: PanelId[]
  /** Every bindable action with the shortcut it answers to. */
  bindings: ActionBinding[]
  appearance: Appearance
  /** `null` means the account's own shell, started as a login shell. */
  shell: ShellSpec | null
  /** Whether Beacon may raise a system notification when a project waits. */
  notifications: boolean
  /** Which moments make a sound. */
  sounds: Sounds
  /** What this build is. */
  version: string
  /** Releases the user has not been shown, newest first. */
  unseenReleases: Release[]
  /** Whether a new version announces itself, rather than waiting to be asked. */
  releaseNotices: boolean
  /** Whether the welcome guide has been through. False only on a fresh install. */
  welcomed: boolean
  /** Whether the file tree lists dotfiles. */
  showHiddenFiles: boolean
  /** Whether Beacon offers its own subagents to the sessions it starts. */
  claudeAgents: boolean
  projectsHome: string
}

export type HostPlatform = 'macos' | 'linux' | 'windows' | string

// ---- sessions ---------------------------------------------------------------

/** What runs inside a session. Both are real processes in a PTY. */
export type SessionKind = 'shell' | 'claude' | 'codex'

export interface SessionInfo {
  id: string
  project: string
  kind: SessionKind
  /** Which of a project's sessions of this kind. Claude has one; terminals can
   *  have several. */
  slot: number
  cwd: string
  running: boolean
  /**
   * The grid the process believes it is drawing on.
   *
   * What a terminal has to be built at before replaying this session's output:
   * the same bytes laid out at a different width wrap in the wrong places, and
   * the result is a panel of shredded text.
   */
  cols: number
  rows: number
}

export interface ScrollbackSnapshot {
  /** Base64-encoded bytes. */
  data: string
  /** Stream offset just past the snapshot. */
  endOffset: number
}

export interface SessionOutput {
  id: string
  /** Travels with the event so the UI knows which project is busy. */
  project: string
  /** Where this chunk starts in the session's lifetime stream. */
  offset: number
  /** Base64-encoded bytes. */
  data: string
}

export interface SessionExit {
  id: string
  project: string
  code: number | null
}

/**
 * What a project appears to be doing, shown on its tab.
 *
 * `working`, `idle` and `stopped` are inferred from the session stream.
 * `waiting` and `done` come from Claude Code's own hooks, when they are
 * installed — the difference between guessing and being told.
 */
export type Activity = 'working' | 'idle' | 'stopped' | 'waiting' | 'done'

/**
 * What Claude Code reports through a hook.
 *
 * `idle` is a session saying it is there and claiming nothing else — it has
 * just started, resumed, or been cleared. It is how a tab stops repeating a
 * `waiting` or `done` that has quietly stopped being true.
 */
export type ClaudeActivity = 'working' | 'waiting' | 'done' | 'idle' | 'ended'

/**
 * What macOS will currently do with a notification from Beacon.
 *
 * `unavailable` is not a refusal: it means this build is not a bundled
 * application — every `tauri dev` run — so macOS has nothing to attribute a
 * notification to. It calls for different advice than `denied`, which is why it
 * is not folded into it.
 */
export type NotificationPermission =
  | 'notDetermined'
  | 'denied'
  | 'authorized'
  | 'provisional'
  | 'unavailable'

export interface SessionActivity {
  project: string
  activity: ClaudeActivity
  /** The tool it just started, when there is one worth naming. */
  detail: string | null
  /** The turn's final reply, when this is `done` and Claude Code said it. */
  reply?: string | null
}

export interface Sounds {
  waiting: boolean
  done: boolean
}

/**
 * A session that started without something it was meant to have.
 *
 * Deliberately not an error: the session is running and the user can work. The
 * daemon writes `summary` for a person to read, because it is the only layer
 * that knows what actually went missing.
 */
export interface SessionDegraded {
  project: string
  summary: string
}

/**
 * The daemon started a session again by itself — after somebody signed in to
 * Claude Code, for the panels left on its sign-in screen. Any view of it is
 * showing a process that has gone, and asks for the session afresh.
 */
export interface SessionRestarted {
  project: string
  kind: SessionKind
  slot: number
}

/**
 * What a clip is, so the drawer can label it and pick a typeface.
 *
 * `command` and `variable` are shown monospaced and never wrapped: a line break
 * inside a command does not stay cosmetic once it is pasted into a shell.
 */
export type ClipKind = 'text' | 'command' | 'variable' | 'email'

/** Something Claude produced for you to paste somewhere else. */
export interface Clip {
  id: string
  /** The project whose session produced it. */
  project: string
  title: string
  /** Exactly what the copy button puts on the clipboard. Never reformatted. */
  body: string
  kind: ClipKind
  /** Unix seconds. */
  createdAt: number
}

/** Whether Beacon's hooks are registered with Claude Code. */
export type HookStatus = 'installed' | 'stale' | 'notInstalled'

/** Which palette the window uses. `system` follows the operating system. */
export type Theme = 'system' | 'dark' | 'light'

/**
 * How the window looks.
 *
 * Theme is a design choice; the other two are taste — how much of the desktop
 * shows through, and how far it is pushed out of focus.
 */
/**
 * A shell and how to start it.
 *
 * Beacon is the terminal emulator, so this is a shell — zsh, fish, nu — and not
 * another emulator. Arguments are configurable because "login shell" is spelled
 * differently enough to matter.
 */
export interface ShellSpec {
  program: string
  args: string[]
}

export interface Appearance {
  theme: Theme
  /** 0.5..1. Never lower: a window you cannot read is not a preference. */
  windowOpacity: number
  /**
   * Whether what shows through the window is frosted rather than sharp.
   *
   * Applied by the shell as a window effect, not by CSS: a backdrop filter can
   * only blur what is behind an element within the page, and it never reaches
   * the desktop. macOS Only.
   */
  frosted: boolean
}

/** What changed in a version, shipped inside the build. */
export interface Release {
  version: string
  date: string
  summary?: string
  changes: string[]
}

export type Importance = 'required' | 'recommended'

export interface InstallOption {
  label: string
  command: string
}

/** Something Beacon needs from the machine, and how to get it. */
export interface Requirement {
  id: string
  name: string
  importance: Importance
  /** Where it was found, resolved the way a session would resolve it. */
  path?: string
  version?: string
  whatBreaks: string
  install: InstallOption[]
  note?: string
}

export interface Integration {
  hooks: HookStatus
  hookCommand: string
  statusLine: boolean
  statusLineCommand: string
}

/** Whether Beacon's Codex plugin is generated, and still names this build. */
export type PluginStatus = 'installed' | 'stale' | 'notInstalled'

export interface CodexIntegration {
  plugin: PluginStatus
  /** Where Beacon writes the marketplace it offers Codex. */
  marketplace: string
  capabilities: CodexCapabilities
}

/**
 * What the installed Codex can do.
 *
 * Read from the program itself — `codex --help` and `codex features list` —
 * rather than from a table of versions.
 */
export interface CodexCapabilities {
  version?: string
  /** Absent from every Codex so far, and why Beacon must be told a session's id. */
  assignedSessionId: boolean
  namedSessions: boolean
  resume: boolean
  fork: boolean
  workingDir: boolean
  sandbox: boolean
  inlineTui: boolean
  configOverride: boolean
  /** Everything Beacon shows about a live Codex session hangs off this. */
  hooks: boolean
  worktree: boolean
}

/**
 * A subagent starting or finishing inside a Claude session.
 *
 * Kept nowhere: it is activity, not history. An agent that ran for twelve
 * seconds is worth seeing while it runs and worth nothing afterwards.
 */
export interface AgentActivity {
  project: string
  /** Claude Code's id, so a start and a stop pair up. */
  agent: string
  /** Which agent it is. Claude Code sometimes reports this empty, hence absent. */
  agentType?: string
  running: boolean
  /** One line of what it found, on the way out. */
  summary?: string
}

/**
 * One piece of work, and the Claude conversation that belongs to it.
 *
 * The id is the conversation's, chosen by Beacon and handed to Claude Code with
 * `--session-id`. Nothing here is read out of a transcript.
 */
/**
 * Which checkout a git command acts on.
 *
 * Bundled rather than passed beside the project so that every call has to say
 * it. A command that quietly kept the default would be reading — or staging —
 * the wrong copy of the repository, and nothing on screen would look wrong.
 */
export interface Checkout {
  workspaceId: string
  projectId: string
  /** Absent is the project's own directory, which is where everything starts. */
  agent?: AgentKind | undefined
}

/** Which agent a conversation belongs to. */
export type AgentKind = 'claude' | 'codex'

export interface Workstream {
  id: string
  project: string
  /**
   * Which agent's conversation this is.
   *
   * Absent from a book written before Beacon ran more than one, where every
   * conversation is Claude Code's.
   */
  agent?: AgentKind
  /**
   * What it was called, if it was called anything.
   *
   * Absent rather than defaulted: a name Beacon invented would be
   * indistinguishable from one you chose.
   */
  name?: string
  /** Unix seconds. */
  createdAt: number
  lastActiveAt: number
  /** The conversation this was forked from. */
  forkedFrom?: string
  /**
   * Whether the conversation exists as far as Claude Code is concerned.
   *
   * Not the same as "Beacon has started a process with this id": Claude Code
   * writes nothing until the first exchange, so a session opened and never
   * typed into leaves nothing to resume or fork.
   */
  resumable: boolean
  /**
   * The id the agent itself uses, when it is not the one Beacon chose.
   *
   * Codex names its own conversations and reports the name in a hook, so this
   * is empty until its first turn has been taken.
   */
  agentSessionId?: string
  model?: string
  contextUsedPercentage?: number
}

/** A project's conversations, most recently active first. */
export interface Workstreams {
  workstreams: Workstream[]
  /** Which one the project is in, when it is in one. */
  current?: string
}

/** One conversation, and the Claude session now running it. */
export interface OpenedWorkstream {
  workstream: Workstream
  session: SessionInfo
}

/** What to show for a conversation nobody named. */
export function workstreamLabel(workstream: Workstream): string {
  return workstream.name ?? workstream.id.split('-')[0]!
}

/**
 * What the installed Claude Code offers Beacon.
 *
 * Read out of the CLI's own `--help`, not out of a table of version numbers, so
 * a feature that is missing hides itself instead of failing. Some things cannot
 * be asked about at all — which hook events exist, whether a task list id is
 * honoured — and are deliberately absent here: the honest test for those is
 * whether anything ever arrives.
 */
export interface ClaudeCapabilities {
  /** Exactly what `claude --version` printed. */
  version?: string
  parsedVersion?: { major: number; minor: number; patch: number }
  /** `--session-id`: Beacon chooses the conversation's id when it starts one. */
  assignedSessionId: boolean
  /** `-n, --name` */
  namedSessions: boolean
  resume: boolean
  forkSession: boolean
  worktree: boolean
  /** `--agents`: agents that live for one session and write nothing to the repo. */
  sessionAgents: boolean
  sessionSettings: boolean
  appendSystemPrompt: boolean
  effort: boolean
  model: boolean
  /** `claude agents --json`: which sessions exist, said rather than inferred. */
  sessionListing: boolean
}

/** Whether named, resumable workstreams are possible at all. */
export function supportsWorkstreams(capabilities: ClaudeCapabilities): boolean {
  return capabilities.assignedSessionId && capabilities.namedSessions && capabilities.resume
}

/**
 * What a Claude session is costing, as Claude Code reports it.
 *
 * Every field is optional because Claude Code fills in what it knows. A missing
 * number is shown as missing, never as zero — zero would read as "none used".
 */
export interface UsageReport {
  project: string
  /**
   * Which agent this is about.
   *
   * Absent from a daemon that predates Codex reporting anything, and from
   * Claude Code's status line, which does not know there is another agent —
   * both of which mean Claude.
   */
  agent?: AgentKind
  /** The conversation Claude Code is in. How a session becomes addressable. */
  sessionId?: string
  /**
   * The name set with `--name` or `/rename`.
   *
   * Absent for an automatic display name like `beacon-split-b7`, so this says
   * "the user named it", not "it has a name".
   */
  sessionName?: string
  model?: string
  /** The model's identifier, for decisions that need something stable. */
  modelId?: string
  /** Reasoning effort, when the model has one. */
  effort?: string
  thinking?: boolean
  contextUsedPercentage?: number
  /** What is left, as Claude Code states it rather than as `100 - used`. */
  contextRemainingPercentage?: number
  contextUsedTokens?: number
  contextSize?: number
  /** Absent until there has been an API response to observe. */
  promptCache?: PromptCache
  fiveHourUsedPercentage?: number
  /** Unix seconds. */
  fiveHourResetsAt?: number
  sevenDayUsedPercentage?: number
  sevenDayResetsAt?: number
  spendLimitUsedPercentage?: number
  spendLimitResetsAt?: number
  /** The worktree the session is working in, when it is in one. */
  worktree?: string
  /** Time spent waiting on the API, in ms. Grows only when a response arrives. */
  apiDurationMs?: number
  /** Unix ms when the daemon heard this. Absent from an older daemon. */
  reportedAt?: number
  /**
   * Unix ms when the rate limits here were last new. Older than `reportedAt`
   * when the report only repeats an earlier one's.
   */
  limitsSeenAt?: number
  /**
   * `limitsSeenAt` is only when the daemon first heard these limits: the first
   * report from a conversation it did not know, as after it restarts. They may
   * be older than that.
   */
  limitsAgeUnknown?: boolean
}

/**
 * What the prompt cache is doing.
 *
 * The number that changes a decision is `recacheTokensIfCold`: a large context
 * whose cache has gone cold is paid for again on the next turn, and that is the
 * moment a clean workstream is worth more than continuing.
 */
export interface PromptCache {
  warm?: boolean
  /** 0..1, not a percentage. */
  hitRatio?: number
  /** Unix seconds when a warm cache goes cold. */
  expiresAt?: number
  recacheTokensIfCold?: number
  misses?: number
  expectedRebuilds?: number
}

// ---- files ------------------------------------------------------------------

export type EntryKind = 'file' | 'directory' | 'symlink'

export interface DirEntry {
  name: string
  /** Relative to the project root, always with `/` separators. */
  path: string
  kind: EntryKind
  hidden: boolean
}

/** What came back from opening a file. */
export type FileContents =
  | { kind: 'text'; text: string }
  | { kind: 'binary'; size: number }
  | { kind: 'tooLarge'; size: number }

/**
 * A file as it was when Beacon read it.
 *
 * The revision is what makes writing it back safe: Claude edits files that are
 * open, so saving a buffer is a request to overwrite whatever happened in
 * between, and nobody means that.
 */
export type FileRead = FileContents & { revision?: number }

/**
 * How a write ended.
 *
 * A successful write carries the revision the file now has: reading it back in
 * a second call would leave a window in which someone else's write becomes the
 * stamp Beacon believes is its own.
 */
export type WriteOutcome =
  | { outcome: 'written'; revision?: number }
  | { outcome: 'stale' }

/**
 * One assignment in a `.env` file.
 *
 * Held in component state for as long as the view is open and nowhere else:
 * never in the persisted store, never logged, never sent anywhere.
 */
export interface EnvEntry {
  key: string
  value: string
  /** 1-based, so the editor can jump to it. */
  line: number
}

// ---- git --------------------------------------------------------------------

export type FileState =
  | 'unmodified'
  | 'modified'
  | 'added'
  | 'deleted'
  | 'renamed'
  | 'copied'
  | 'typeChanged'
  | 'untracked'
  | 'ignored'
  | 'conflicted'

export interface GitEntry {
  /** Relative to the repository root, as git reports it. */
  path: string
  /** Where a rename or copy came from. */
  originalPath?: string
  /** What is staged for the next commit. */
  staged: FileState
  /** What has changed since, in the working tree. */
  unstaged: FileState
  /**
   * Unmerged: the path has conflict stages in the index rather than one entry.
   * Reported by the backend because the two states alone do not say so — `AA`
   * and `DD` are conflicts that never mention `U`.
   */
  conflicted: boolean
}

export interface GitStatus {
  /** `null` when the head is detached. */
  branch: string | null
  upstream?: string
  ahead: number
  behind: number
  /** True before the first commit. */
  unborn: boolean
  entries: GitEntry[]
}

/**
 * A project's file listing, and whether it is all of it.
 *
 * Beacon stops at fifty thousand. Past that the window would spend longer
 * ranking the list than the user spends reading it — but a listing that is
 * quietly short is a search that quietly cannot find a file that exists, so
 * it says when it stopped.
 */
export interface ProjectFiles {
  files: string[]
  truncated: boolean
}
