import { toggleDrawer } from '@/features/clips/clips'
import { ipc, pickFolder } from '@/ipc'
import { useEditor } from '@/features/editor/openFiles'
import { shortcutLabel } from '@/lib/platform'
import { describeBinding } from './keymap'
import type { LayoutPreset, PanelId } from '@/types/beacon'
import { useBeacon } from './store'

export interface Command {
  id: string
  title: string
  /** Grouping shown beside the title. */
  group: string
  /** Rendered on the right; the same shortcut the keyboard layer resolves to. */
  hint?: string | undefined
  run: () => void | Promise<void>
}

/**
 * Everything Beacon can be told to do, in one place.
 *
 * The palette lists this, and the keyboard layer resolves bindings against the
 * same ids — so a command cannot exist with a shortcut but no way to discover
 * it, or appear in the palette and do something different from its shortcut.
 * Making bindings user-editable later means changing one table, not every call
 * site.
 */
export function buildCommands(): Command[] {
  const store = useBeacon.getState()
  const snapshot = store.snapshot
  const workspace = snapshot?.workspaces.find((w) => w.id === snapshot.activeWorkspace)
  const projects = workspace?.projects ?? []
  const activeProjectId = workspace ? snapshot?.activeProject[workspace.id] : undefined
  const project = projects.find((p) => p.id === activeProjectId) ?? projects[0]

  /** The shortcut an action currently answers to, whatever the user set. */
  const hintFor = (action: string): string | undefined => {
    const binding = snapshot?.bindings.find((entry) => entry.action === action)
    return binding ? describeBinding(binding.binding) : undefined
  }

  const commands: Command[] = [
    {
      id: 'app.settings',
      title: 'Open settings',
      group: 'Beacon',
      ...(hintFor('settings.open') ? { hint: hintFor('settings.open') } : {}),
      run: () => store.setOverlay('settings'),
    },
    {
      id: 'app.clips',
      title: 'Show clips to copy',
      group: 'Beacon',
      run: () => toggleDrawer(),
    },
    {
      id: 'app.stopDaemon',
      title: 'Stop all sessions and the session daemon',
      group: 'Beacon',
      run: async () => {
        await ipc.stopDaemon()
      },
    },
  ]

  // ---- projects ----

  commands.push({
    id: 'project.add',
    title: 'Add project…',
    group: 'Project',
    run: async () => {
      const folder = await pickFolder('Add project', snapshot?.projectsHome)
      if (folder) await store.addProject(folder)
    },
  })

  projects.forEach((candidate, index) => {
    commands.push({
      id: `project.switch.${candidate.id}`,
      title: `Switch to ${candidate.name}`,
      group: 'Project',
      ...(index < 9 ? { hint: shortcutLabel(String(index + 1)) } : {}),
      run: () => store.selectProject(candidate.id),
    })
  })

  if (project) {
    commands.push(
      {
        id: 'project.restartClaude',
        title: 'Restart Claude',
        group: 'Session',
        ...(hintFor('session.restartClaude') ? { hint: hintFor('session.restartClaude') } : {}),
        run: () => store.restartSession(project.id, 'claude'),
      },
      {
        id: 'project.restartTerminal',
        title: 'Restart terminal',
        group: 'Session',
        run: () => store.restartSession(project.id, 'shell'),
      },
      {
        id: 'project.stop',
        title: 'Stop this project’s processes',
        group: 'Session',
        run: () => store.stopProject(project.id),
      },
      {
        id: 'project.reveal',
        title: 'Reveal project folder',
        group: 'Project',
        run: () => store.revealProject(project.id),
      },
      {
        id: 'project.remove',
        title: 'Remove project from workspace',
        group: 'Project',
        run: () => store.removeProject(project.id),
      },
    )
  }

  // ---- workspaces ----

  for (const candidate of snapshot?.workspaces ?? []) {
    if (candidate.id === workspace?.id) continue
    commands.push({
      id: `workspace.switch.${candidate.id}`,
      title: `Switch to ${candidate.name} workspace`,
      group: 'Workspace',
      run: () => store.selectWorkspace(candidate.id),
    })
  }

  // ---- panels ----

  const panels: Array<{ panel: PanelId; label: string }> = [
    { panel: 'files', label: 'Files' },
    { panel: 'git', label: 'Git' },
    { panel: 'terminal', label: 'Terminal' },
    { panel: 'editor', label: 'the editor' },
  ]

  for (const { panel, label } of panels) {
    const hidden = snapshot?.hidden.includes(panel) === true
    commands.push({
      id: `panel.toggle.${panel}`,
      title: `${hidden ? 'Show' : 'Hide'} ${label}`,
      group: 'Panels',
      ...(hintFor(`panel.toggle.${panel}`) ? { hint: hintFor(`panel.toggle.${panel}`) } : {}),
      run: () => store.togglePanel(panel),
    })
  }

  commands.push({
    id: 'panel.fullscreen',
    title: store.fullscreenPanel ? 'Leave fullscreen' : 'Fullscreen the Claude panel',
    group: 'Panels',
    ...(hintFor('panel.fullscreen') ? { hint: hintFor('panel.fullscreen') } : {}),
    run: () => store.toggleFullscreen(store.fullscreenPanel ?? 'claude'),
  })

  // ---- appearance ----

  if (snapshot) {
    const themes: Array<{ theme: 'system' | 'dark' | 'light'; label: string }> = [
      { theme: 'system', label: 'System' },
      { theme: 'dark', label: 'Dark' },
      { theme: 'light', label: 'Light' },
    ]

    for (const { theme, label } of themes) {
      if (theme === snapshot.appearance.theme) continue
      commands.push({
        id: `appearance.theme.${theme}`,
        title: `Theme: ${label}`,
        group: 'Appearance',
        run: () => store.setAppearance({ ...snapshot.appearance, theme }),
      })
    }
  }

  // ---- layout ----

  const presets: Array<{ preset: LayoutPreset; label: string }> = [
    { preset: 'claude-left', label: 'Claude left' },
    { preset: 'claude-right', label: 'Claude right' },
    { preset: 'claude-right-tall', label: 'Tall right' },
    { preset: 'claude-left-tall', label: 'Tall left' },
  ]

  for (const { preset, label } of presets) {
    commands.push({
      id: `layout.${preset}`,
      title: `Layout: ${label}`,
      group: 'Layout',
      run: () => store.setPreset(preset),
    })
  }

  // ---- editor ----

  if (project) {
    const open = useEditor.getState().byProject[project.id] ?? []
    for (const file of open) {
      commands.push({
        id: `editor.focus.${file.path}`,
        title: `Go to ${file.name}`,
        group: 'Editor',
        run: () => {
          useEditor.getState().activate(project.id, file.path)
          void store.showPanel('editor')
        },
      })
    }
  }

  return commands
}
