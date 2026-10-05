import type { PanelId } from '@/types/beacon'

/**
 * What a step points at.
 *
 * Only hooks that exist for their own reasons — a panel's `data-panel`, which
 * focus and layout already rely on, and a `data-region` on the two bars — so
 * the guide is not one more thing to keep in step with how a screen is built.
 * When the thing is not on screen, the step is shown on its own, in the middle.
 */
export type Anchor = { panel: PanelId } | { region: 'titlebar' | 'statusbar' }

export interface GuideStep {
  id: string
  title: string
  paragraphs: string[]
  anchor?: Anchor
  /** A step that does something rather than only saying something. */
  action?: 'basics' | 'addProject' | 'showPanel' | 'signIn'
}

/** What the steps depend on, so they can be worked out without a window. */
export interface GuideFacts {
  hasProject: boolean
  hidden: readonly PanelId[]
  /** The shortcut an action answers to, as shown to the user, if it has one. */
  hint: (action: string) => string | undefined
  claudeInstalled: boolean
}

/**
 * Every panel the guide introduces, in the order it introduces them.
 *
 * A test holds this against the panels that exist, so a panel added later
 * cannot go missing from the guide without somebody deciding it should.
 */
export const PANEL_STEPS: Record<PanelId, { title: string; paragraphs: string[] }> = {
  claude: {
    title: 'Claude',
    paragraphs: [
      'Claude Code itself: the real claude command, running in this project’s folder, exactly as it would in your own terminal.',
      'The project’s tab shows whether it is working, waiting for you or done, so you can leave it and come back. When a turn ends, its last reply stays above the terminal until the next one starts.',
    ],
  },
  codex: {
    title: 'Codex',
    paragraphs: [
      'OpenAI’s Codex, run the same way, for whoever uses it. Close the panel if you do not; Claude is enough on its own.',
    ],
  },
  files: {
    title: 'Files',
    paragraphs: [
      'The project’s files, marked with what git says has changed. Click one to open it in the editor.',
    ],
  },
  editor: {
    title: 'Editor',
    paragraphs: [
      'For the small change you would rather make yourself than ask for: open a file, edit it, save it, without leaving Beacon.',
    ],
  },
  terminal: {
    title: 'Terminal',
    paragraphs: [
      'Your own shell in the project’s folder, as many as you like. Beacon is the terminal here, so it is the same shell you already use.',
    ],
  },
  git: {
    title: 'Git',
    paragraphs: [
      'What has changed and on which branch: read a diff, stage, commit, pull and push.',
    ],
  },
}

const PANEL_ORDER: PanelId[] = ['claude', 'codex', 'files', 'editor', 'terminal', 'git']

/** The guide, worked out for the window as it is now. */
export function guideSteps(facts: GuideFacts): GuideStep[] {
  const steps: GuideStep[] = [
    {
      id: 'intro',
      title: 'Welcome to Beacon',
      paragraphs: [
        'Beacon keeps several projects open at once, each with its own Claude Code, terminals and files, and tells you when one of them needs you.',
        'A minute to set the basics and see what is where. Skip it whenever you like; it is in the command palette for later.',
      ],
    },
    {
      id: 'basics',
      title: 'A few basics',
      paragraphs: [
        'How Beacon looks, and how it gets your attention when a project you are not looking at needs it.',
      ],
      action: 'basics',
    },
  ]

  if (!facts.hasProject) {
    steps.push({
      id: 'project',
      title: 'Add your first project',
      paragraphs: [
        'A project is a folder you work in. Beacon opens Claude Code there, with a terminal and the files beside it.',
        'Add as many as you like; each keeps running while you look at another.',
      ],
      action: 'addProject',
    })
  }

  steps.push({
    id: 'titlebar',
    title: 'Workspaces and projects',
    paragraphs: [
      'Along the top: the workspace you are in, then a tab for each of its projects, with + to add another.',
      'A workspace groups the projects you switch between and gives them a colour, so you know where you are before you read anything.',
    ],
    anchor: { region: 'titlebar' },
  })

  // Panels are only pointed at once there is a project for them to be in.
  if (facts.hasProject) {
    for (const panel of PANEL_ORDER) {
      const { title, paragraphs } = PANEL_STEPS[panel]
      const hidden = facts.hidden.includes(panel)
      const hint = facts.hint(`panel.toggle.${panel}`)
      steps.push({
        id: `panel.${panel}`,
        title,
        paragraphs: hidden
          ? [
              ...paragraphs,
              hint
                ? `It is hidden right now. ${hint} shows or hides it, or show it from here.`
                : 'It is hidden right now. Show it from here.',
            ]
          : paragraphs,
        anchor: { panel },
        ...(hidden ? { action: 'showPanel' as const } : {}),
      })
    }
  }

  steps.push({
    id: 'statusbar',
    title: 'The status bar',
    paragraphs: [
      'The project’s path, and what Beacon has to tell you: a missing program, an update, what is new in each version.',
    ],
    anchor: { region: 'statusbar' },
  })

  const palette = facts.hint('palette.open')
  const quickOpen = facts.hint('quickOpen.open')
  const settings = facts.hint('settings.open')
  steps.push({
    id: 'keyboard',
    title: 'Everything, from the keyboard',
    paragraphs: [
      palette
        ? `${palette} opens the command palette: everything Beacon can do, from adding a project to showing a panel.`
        : 'The command palette has everything Beacon can do, from adding a project to showing a panel.',
      ...(quickOpen ? [`${quickOpen} finds a file in the project.`] : []),
      settings
        ? `${settings} opens Settings, where all of this and more can be changed later. This guide is in the palette too, as “Show the welcome guide”.`
        : 'Settings has all of this and more. This guide is in the palette too, as “Show the welcome guide”.',
    ],
  })

  steps.push({
    id: 'signIn',
    title: 'Last: sign in to Claude Code',
    paragraphs: facts.claudeInstalled
      ? [
          'The first time Claude Code runs, it asks you to sign in, with its own screen inside the Claude panel.',
          'Click into the panel, choose how to sign in and press Enter. A browser page opens to authorise it, and that is all: Claude Code remembers it for every project.',
        ]
      : [
          'Claude Code is not installed yet, and the Claude panel needs it. The panel shows how to install it.',
          'Once it is, the panel asks you to sign in the first time: choose how, press Enter and authorise it in the browser.',
        ],
    anchor: { panel: 'claude' },
    action: 'signIn',
  })

  return steps
}

/** The CSS selector for what a step points at. */
export function anchorSelector(anchor: Anchor): string {
  return 'panel' in anchor ? `[data-panel="${anchor.panel}"]` : `[data-region="${anchor.region}"]`
}

export interface Box {
  top: number
  left: number
  width: number
  height: number
}

/**
 * Where the card goes: beside what it points at, on whichever side has room,
 * and never off the window.
 *
 * Right, then left, then below, then above. A target that leaves room on no
 * side — the Claude panel often fills most of the window — gets the card in
 * its own lower corner, which is where a terminal is emptiest.
 */
export function placeCard(
  target: Box | null,
  card: { width: number; height: number },
  viewport: { width: number; height: number },
  gap = 14,
  margin = 16,
): { top: number; left: number } {
  const clampTop = (top: number): number =>
    Math.max(margin, Math.min(top, viewport.height - card.height - margin))
  const clampLeft = (left: number): number =>
    Math.max(margin, Math.min(left, viewport.width - card.width - margin))

  if (!target) {
    return {
      top: clampTop((viewport.height - card.height) / 2),
      left: clampLeft((viewport.width - card.width) / 2),
    }
  }

  const right = target.left + target.width
  const bottom = target.top + target.height

  if (right + gap + card.width <= viewport.width - margin) {
    return { top: clampTop(target.top), left: right + gap }
  }
  if (target.left - gap - card.width >= margin) {
    return { top: clampTop(target.top), left: target.left - gap - card.width }
  }
  if (bottom + gap + card.height <= viewport.height - margin) {
    return { top: bottom + gap, left: clampLeft(target.left) }
  }
  if (target.top - gap - card.height >= margin) {
    return { top: target.top - gap - card.height, left: clampLeft(target.left) }
  }
  return {
    top: clampTop(bottom - card.height - margin),
    left: clampLeft(right - card.width - margin),
  }
}
