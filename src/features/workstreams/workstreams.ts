import { create } from 'zustand'

import { useBeacon } from '@/app/store'
import { errorMessage, ipc } from '@/ipc'
import type { OpenedWorkstream, Workstream } from '@/types/beacon'

interface ProjectWorkstreams {
  list: Workstream[]
  /** Which one the project is in, when it is in one. */
  current: string | null
}

interface WorkstreamState {
  byProject: Record<string, ProjectWorkstreams>
  /** Which project is being switched, so the menu can say so and not race. */
  busy: string | null
}

/**
 * A project's Claude conversations, as the daemon knows them.
 *
 * A feature store rather than part of the application state: the window can go
 * a whole session without opening the menu, and nothing else needs to know.
 */
export const useWorkstreams = create<WorkstreamState>(() => ({ byProject: {}, busy: null }))

function accept(project: string, opened: { list: Workstream[]; current: string | null }): void {
  useWorkstreams.setState((state) => ({
    byProject: { ...state.byProject, [project]: opened },
  }))
}

/** What a project has, from the daemon. */
export async function loadWorkstreams(project: string): Promise<void> {
  try {
    const answer = await ipc.listWorkstreams(project)
    accept(project, { list: answer.workstreams, current: answer.current ?? null })
  } catch {
    // Not knowing the list is not worth an error in the way: the session is
    // running either way, and the menu simply has nothing to offer.
  }
}

/**
 * Runs one of the three actions that replace a project's Claude.
 *
 * The window's half of the switch is the same for all of them — the daemon has
 * already replaced the process, so the view has to be thrown away and rebuilt
 * or it would go on rendering a session that no longer exists.
 */
async function switching(
  project: string,
  action: () => Promise<OpenedWorkstream>,
): Promise<Workstream | null> {
  if (useWorkstreams.getState().busy) return null
  useWorkstreams.setState({ busy: project })

  try {
    const opened = await action()
    useBeacon.getState().rebuildSession(project, 'claude', 0)
    await loadWorkstreams(project)
    return opened.workstream
  } catch (error) {
    // Said out loud, unlike a failed listing: this one was asked for, and a
    // refusal — the conversation is open in another Claude — is the answer.
    useBeacon.getState().notify(errorMessage(error))
    return null
  } finally {
    useWorkstreams.setState({ busy: null })
  }
}

/** Starts a new conversation and moves the project's Claude into it. */
export function startWorkstream(
  workspace: string,
  project: string,
  name: string | null,
): Promise<Workstream | null> {
  return switching(project, () => ipc.startWorkstream(workspace, project, name, 80, 24))
}

/** Returns the project to a conversation it already has. */
export function resumeWorkstream(
  workspace: string,
  project: string,
  id: string,
): Promise<Workstream | null> {
  return switching(project, () => ipc.resumeWorkstream(workspace, project, id, 80, 24))
}

/** Starts a new conversation carrying another's history. */
export function forkWorkstream(
  workspace: string,
  project: string,
  from: string,
  name: string | null,
): Promise<Workstream | null> {
  return switching(project, () => ipc.forkWorkstream(workspace, project, from, name, 80, 24))
}

/** Renames one without touching the session it is running in. */
export async function renameWorkstream(
  project: string,
  id: string,
  name: string | null,
): Promise<void> {
  try {
    await ipc.renameWorkstream(project, id, name)
    await loadWorkstreams(project)
  } catch (error) {
    useBeacon.getState().notify(errorMessage(error))
  }
}

export function useProjectWorkstreams(project: string): ProjectWorkstreams {
  return useWorkstreams((state) => state.byProject[project] ?? EMPTY)
}

/** A stable reference, so a project with no conversations does not re-render. */
const EMPTY: ProjectWorkstreams = { list: [], current: null }

/** The conversation a project is in, if the window knows about one. */
export function useCurrentWorkstream(project: string): Workstream | null {
  return useWorkstreams((state) => {
    const held = state.byProject[project]
    if (!held?.current) return null
    return held.list.find((stream) => stream.id === held.current) ?? null
  })
}

/**
 * `just now`, `12m`, `3h`, `2d` — how long ago a conversation was worked in.
 *
 * Short because it sits at the end of a menu row, where the name is what is
 * being read and this is only a way to tell two of them apart.
 */
export function lastWorkedIn(at: number, now: number): string {
  const minutes = Math.floor((now - at * 1000) / 60_000)
  if (minutes < 1) return 'just now'
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h`
  return `${Math.floor(hours / 24)}d`
}
