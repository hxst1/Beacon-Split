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
export type PanelId = 'claude' | 'editor' | 'files' | 'git' | 'terminal'

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
  /** What this build is. */
  version: string
  /** Releases the user has not been shown, newest first. */
  unseenReleases: Release[]
  /** Whether a new version announces itself, rather than waiting to be asked. */
  releaseNotices: boolean
  projectsHome: string
}

export type HostPlatform = 'macos' | 'linux' | 'windows' | string

// ---- sessions ---------------------------------------------------------------

/** What runs inside a session. Both are real processes in a PTY. */
export type SessionKind = 'shell' | 'claude'

export interface SessionInfo {
  id: string
  project: string
  kind: SessionKind
  /** Which of a project's sessions of this kind. Claude has one; terminals can
   *  have several. */
  slot: number
  cwd: string
  running: boolean
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

/** What Claude Code reports through a hook. */
export type ClaudeActivity = 'working' | 'waiting' | 'done' | 'ended'

export interface SessionActivity {
  project: string
  activity: ClaudeActivity
  /** The tool it just started, when there is one worth naming. */
  detail: string | null
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

/**
 * What a Claude session is costing, as Claude Code reports it.
 *
 * Every field is optional because Claude Code fills in what it knows. A missing
 * number is shown as missing, never as zero — zero would read as "none used".
 */
export interface UsageReport {
  project: string
  model?: string
  contextUsedPercentage?: number
  contextUsedTokens?: number
  contextSize?: number
  fiveHourUsedPercentage?: number
  /** Unix seconds. */
  fiveHourResetsAt?: number
  sevenDayUsedPercentage?: number
  sevenDayResetsAt?: number
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

export type WriteOutcome = 'written' | 'stale' 

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
