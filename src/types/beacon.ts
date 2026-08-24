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
 * Only these three are derived today. `devServer` and `error` need output
 * inspection and are recorded in the roadmap rather than guessed at.
 */
export type Activity = 'working' | 'idle' | 'stopped' 

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
