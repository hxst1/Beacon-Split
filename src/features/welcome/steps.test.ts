import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { PANEL_LABELS } from '@/lib/layout'
import {
  ALONE,
  anchorSelector,
  guideSteps,
  PANEL_STEPS,
  placeCard,
  TOGETHER,
  type GuideFacts,
} from './steps'

const HINTS: Record<string, string> = {
  'palette.open': '⌘K',
  'quickOpen.open': '⌘P',
  'settings.open': '⌘,',
  'panel.toggle.codex': '⇧⌘O',
}

const facts = (overrides: Partial<GuideFacts> = {}): GuideFacts => ({
  hasProject: true,
  hidden: [],
  hint: (action) => HINTS[action],
  claudeInstalled: true,
  ...overrides,
})

const ids = (f: GuideFacts): string[] => guideSteps(f).map((step) => step.id)

describe('the welcome guide', () => {
  it('introduces every panel there is', () => {
    // A panel added to Beacon has to be added here as well, or this fails.
    expect(Object.keys(PANEL_STEPS).sort()).toEqual(Object.keys(PANEL_LABELS).sort())
  })

  it('has something to say about each of them', () => {
    // The shared step takes the first line of each panel it names, so a panel
    // described with nothing would quietly drop out of a step it is in the
    // heading of.
    for (const [panel, step] of Object.entries(PANEL_STEPS)) {
      expect(step.title, panel).not.toBe('')
      expect(step.paragraphs.length, panel).toBeGreaterThan(0)
      expect(step.paragraphs[0], panel).not.toBe('')
    }
  })

  it('gives every panel somewhere to be introduced', () => {
    // Having words for a panel is not the same as showing them to anybody: a
    // panel in neither list is described and never reached. Nothing may be in
    // both, or it would be introduced twice.
    expect([...ALONE, ...TOGETHER].sort()).toEqual(Object.keys(PANEL_LABELS).sort())
  })

  it('is short enough to be the minute it promises', () => {
    // The run this is really about: a fresh install, where there is no project
    // yet and so no panel to point at.
    expect(ids(facts({ hasProject: false }))).toHaveLength(7)

    // Then with a project, and Codex and the editor away, which is how Beacon
    // starts — `HIDDEN_BY_DEFAULT` in `crates/beacon-core/src/layout.rs`.
    expect(ids(facts({ hidden: ['codex', 'editor'] }))).toHaveLength(8)

    // And with every panel open. The panels are the part that grows: before
    // this they had a card each, which made twelve. A new panel belongs in the
    // step the others share, not in a step of its own.
    expect(ids(facts())).toHaveLength(9)
  })

  it('asks for a first project, and points at no panel, until there is one', () => {
    const steps = ids(facts({ hasProject: false }))
    expect(steps).toContain('project')
    expect(steps.some((id) => id.startsWith('panel.'))).toBe(false)
  })

  it('walks round the panels once there is a project', () => {
    const steps = ids(facts())
    expect(steps).not.toContain('project')
    // An agent each, then the four that explain themselves, together.
    expect(steps.filter((id) => id.startsWith('panel.'))).toEqual([
      'panel.claude',
      'panel.codex',
      'panel.panes',
    ])
  })

  it('introduces the four together, naming each of them, and points at one', () => {
    const panes = guideSteps(facts()).find((step) => step.id === 'panel.panes')

    expect(panes?.title).toBe('Files, editor, terminal and git')
    expect(panes?.paragraphs).toHaveLength(4)
    // Each line says which one it is about, because the heading no longer can.
    expect(panes?.paragraphs[0]).toContain('Files')
    expect(panes?.paragraphs[1]).toContain('editor')
    expect(panes?.paragraphs[2]).toContain('terminal')
    expect(panes?.paragraphs[3]).toContain('Git')
    expect(panes?.anchor).toEqual({ panel: 'files' })
  })

  it('points the shared step at one that is on screen', () => {
    // Pointing at Files when Files is hidden would spotlight an empty
    // rectangle, so it points at the first of them that is there.
    const panes = guideSteps(facts({ hidden: ['files'] })).find((s) => s.id === 'panel.panes')
    expect(panes?.title).toBe('Editor, terminal and git')
    expect(panes?.anchor).toEqual({ panel: 'editor' })
  })

  it('starts with the basics and ends at signing in, in the Claude panel', () => {
    const steps = guideSteps(facts())
    expect(steps[1]?.action).toBe('basics')
    const last = steps.at(-1)
    expect(last?.id).toBe('signIn')
    expect(last?.anchor).toEqual({ panel: 'claude' })
  })

  it('leaves a hidden panel out, and says in the keyboard step where it went', () => {
    // Codex is hidden until somebody asks for it, so a guide that stopped at
    // every hidden panel spent a new user's first minute offering them a
    // second agent before they had met the first.
    const steps = guideSteps(facts({ hidden: ['codex', 'git'] }))

    expect(steps.map((step) => step.id)).not.toContain('panel.codex')
    expect(steps.find((step) => step.id === 'panel.panes')?.title).toBe(
      'Files, editor and terminal',
    )
    expect(steps.find((step) => step.id === 'keyboard')?.paragraphs.at(-1)).toBe(
      'Codex and git are hidden right now; the palette brings them back.',
    )
  })

  it('says it in the singular for one hidden panel', () => {
    const keyboard = guideSteps(facts({ hidden: ['git'] })).find((s) => s.id === 'keyboard')
    expect(keyboard?.paragraphs.at(-1)).toBe(
      'Git is hidden right now; the palette brings it back.',
    )
  })

  it('says nothing about hidden panels when none are', () => {
    const keyboard = guideSteps(facts()).find((step) => step.id === 'keyboard')
    expect(keyboard?.paragraphs.join(' ')).not.toContain('hidden')
  })

  it('skips the shared step when all four are hidden', () => {
    const steps = ids(facts({ hidden: ['files', 'editor', 'terminal', 'git'] }))
    expect(steps).not.toContain('panel.panes')
    expect(steps).toContain('panel.claude')
  })

  it('names the shortcuts the user actually has', () => {
    const keyboard = guideSteps(facts()).find((step) => step.id === 'keyboard')
    expect(keyboard?.paragraphs.join(' ')).toContain('⌘K')
    expect(keyboard?.paragraphs.join(' ')).toContain('⌘,')

    // Unbound, it still says where things are, without a shortcut to press.
    const unbound = guideSteps(facts({ hint: () => undefined })).find((s) => s.id === 'keyboard')
    expect(unbound?.paragraphs.join(' ')).toContain('command palette')
    expect(unbound?.paragraphs.join(' ')).not.toContain('undefined')
  })

  it('says how to get Claude Code when it is not installed', () => {
    const signIn = guideSteps(facts({ claudeInstalled: false })).at(-1)
    expect(signIn?.paragraphs[0]).toContain('not installed')
  })

  it('points at hooks that are really there', () => {
    const source = (path: string): string =>
      readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8')

    expect(source('../../app/TitleBar.tsx')).toContain('data-region="titlebar"')
    expect(source('../../app/StatusBar.tsx')).toContain('data-region="statusbar"')
    expect(source('../../app/panels/Panel.tsx')).toContain('data-panel={id}')
    expect(anchorSelector({ panel: 'git' })).toBe('[data-panel="git"]')
    expect(anchorSelector({ region: 'titlebar' })).toBe('[data-region="titlebar"]')
  })
})

describe('placing the card', () => {
  const viewport = { width: 1200, height: 800 }
  const card = { width: 300, height: 200 }

  it('centres it when there is nothing to point at', () => {
    expect(placeCard(null, card, viewport)).toEqual({ top: 300, left: 450 })
  })

  it('puts it beside the target, right first', () => {
    const narrow = { top: 100, left: 40, width: 200, height: 500 }
    expect(placeCard(narrow, card, viewport)).toEqual({ top: 100, left: 254 })
  })

  it('goes left when the right has no room', () => {
    const atTheRight = { top: 100, left: 900, width: 280, height: 500 }
    expect(placeCard(atTheRight, card, viewport)).toEqual({ top: 100, left: 586 })
  })

  it('goes below a bar across the top', () => {
    const titlebar = { top: 0, left: 0, width: 1200, height: 42 }
    expect(placeCard(titlebar, card, viewport)).toEqual({ top: 56, left: 16 })
  })

  it('goes above a bar across the bottom', () => {
    const statusbar = { top: 776, left: 0, width: 1200, height: 24 }
    expect(placeCard(statusbar, card, viewport)).toEqual({ top: 562, left: 16 })
  })

  it('sits inside a target that fills the window, in its lower corner', () => {
    const everything = { top: 42, left: 0, width: 1200, height: 734 }
    expect(placeCard(everything, card, viewport)).toEqual({ top: 560, left: 884 })
  })

  it('never leaves the window', () => {
    const tiny = { width: 400, height: 300 }
    const big = { width: 380, height: 280 }
    for (const target of [null, { top: 0, left: 0, width: 400, height: 300 }]) {
      const { top, left } = placeCard(target, big, tiny)
      expect(top).toBeGreaterThanOrEqual(0)
      expect(left).toBeGreaterThanOrEqual(0)
    }
  })
})
