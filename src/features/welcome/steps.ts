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
  action?: 'basics' | 'addProject' | 'signIn'
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
      'Files is the project’s tree, marked with what git says has changed. Click one to open it in the editor.',
    ],
  },
  editor: {
    title: 'Editor',
    paragraphs: [
      'The editor is for the small change you would rather make yourself than ask for: open a file, change it, save it, without leaving Beacon.',
    ],
  },
  terminal: {
    title: 'Terminal',
    paragraphs: [
      'The terminal is your own shell in the project’s folder, as many as you like. Beacon is the terminal here, so it is the shell you already use.',
    ],
  },
  git: {
    title: 'Git',
    paragraphs: [
      'Git is what has changed and on which branch: read a diff, stage, commit, pull and push.',
    ],
  },
}

/**
 * The panels a step is spent on one at a time, and the ones that share one.
 *
 * An agent is why somebody opened Beacon, so it is worth a step of its own.
 * The other four explain themselves from their names, and four cards in a row
 * saying "this is X" is the stretch of a guide people skip — so they are
 * introduced together, in a sentence each.
 *
 * A test holds both lists against the panels that exist, so a panel added
 * later cannot slip into neither.
 */
export const ALONE: PanelId[] = ['claude', 'codex']
export const TOGETHER: PanelId[] = ['files', 'editor', 'terminal', 'git']

/** "Files, editor, terminal and git" — only the first keeps its capital. */
function listed(titles: string[]): string {
  const [first, ...rest] = titles
  if (first === undefined) return ''
  const lower = rest.map((title) => title.toLowerCase())
  const last = lower.pop()
  if (last === undefined) return first
  return `${[first, ...lower].join(', ')} and ${last}`
}

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

  // Panels are only pointed at once there is a project for them to be in, and
  // a hidden one gets no step at all. Offering to show it made the guide
  // longer the less of Beacon somebody had chosen to see, and Codex is hidden
  // until it is asked for — so a first minute spent offering a second agent to
  // somebody who has not met the first one. The keyboard step says where the
  // hidden ones are instead.
  if (facts.hasProject) {
    for (const panel of ALONE) {
      if (facts.hidden.includes(panel)) continue
      const { title, paragraphs } = PANEL_STEPS[panel]
      steps.push({ id: `panel.${panel}`, title, paragraphs, anchor: { panel } })
    }

    const together = TOGETHER.filter((panel) => !facts.hidden.includes(panel))
    const [firstOfThem] = together
    if (firstOfThem !== undefined) {
      steps.push({
        id: 'panel.panes',
        title: listed(together.map((panel) => PANEL_STEPS[panel].title)),
        // The first line of each, which is the line that says what it is.
        paragraphs: together.flatMap((panel) => PANEL_STEPS[panel].paragraphs.slice(0, 1)),
        // Files when it is there, and whichever of them is otherwise: a step
        // about four panels should point at one the reader can actually see.
        anchor: { panel: firstOfThem },
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
  const away = [...ALONE, ...TOGETHER].filter((panel) => facts.hidden.includes(panel))
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
      // Said once, here, rather than as a step each: a panel somebody has
      // hidden is not a thing to walk them through, only a thing to be able
      // to find again.
      ...(away.length > 0
        ? [
            `${listed(away.map((panel) => PANEL_STEPS[panel].title))} ${
              away.length === 1 ? 'is' : 'are'
            } hidden right now; the palette brings ${
              away.length === 1 ? 'it' : 'them'
            } back.`,
          ]
        : []),
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
